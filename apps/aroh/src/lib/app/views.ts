import type { Clock } from '$lib/ui/clock';
import type { ActionView, CategorySlot, ChangeView, ComponentSpec, Point, SpecOf, Unit, ViewSpec } from '$lib/ui/contract';
import { daysBetween, formatDate, formatYmd, minuteOfDay, mondayOf, shiftDay } from '$lib/ui/format';
import type { AutonomyLabel, CategoryView, GoalHeader, GoalRow, InboxItem, RunView, SettingsView } from '$lib/ui/screens/types';
import { catalog } from './catalog';
import type { Action, Goal, Proposal, Snapshot, Tracker, TrackerField } from './client';
import type { RunSummary } from './engine';

const SEEDED_SLOTS: Record<string, CategorySlot> = { Learning: 1, 'Health and Fitness': 2, Financial: 3, 'Hobby and Skill': 4, 'Career and Projects': 5, Habit: 6 };
const UNITS: readonly Unit[] = ['kg', 'lb', 'cm', 'kcal', 'g', 'ml', 'L', 'min', 'reps', 'steps', 'usd', 'count'];
const OPS: readonly ChangeView['op'][] = ['move', 'resize', 'create', 'delete'];
const NO_CATEGORY: CategoryView = { name: 'No category', slot: 6 };
const RECENT = 20;

/** Autonomy level of a goal row as the screens label it. */
export const AUTONOMY_LABEL: Record<Goal['autonomy'], AutonomyLabel> = { ask: 'Ask', balanced: 'Balanced', auto: 'Auto' };

/** A check-in the owner owes now: its tracker, the field it asks and the question card. */
export type DueCheckIn = { trackerId: string; key: string; spec: ComponentSpec };

/** Every category with its colour slot: seeded names by the fixed table, others by id order. */
export function categoryViews(s: Snapshot): Map<string, CategoryView> {
	const own = s.categories.filter((c) => !(c.name in SEEDED_SLOTS)).map((c) => c.id).sort();
	return new Map(
		s.categories.map((c) => [c.id, { name: c.name, slot: SEEDED_SLOTS[c.name] ?? (((own.indexOf(c.id) % 6) + 1) as CategorySlot) }])
	);
}

/** The categories in slot order, for filters and the desktop sidebar. */
export function categoryList(s: Snapshot): CategoryView[] {
	return [...categoryViews(s).values()].sort((a, b) => a.slot - b.slot || a.name.localeCompare(b.name));
}

/**
 * A goal's "YYYY-MM-DD" deadline as the instant the screens format: local noon of that date in
 * `zone`, so the shown day never shifts, whatever the zone's UTC offset.
 */
export function deadlineAt(ymd: string, zone: string): string {
	const noonUtc = `${ymd}T12:00:00Z`;
	const offsetMin = daysBetween(ymd, formatYmd(noonUtc, zone)) * 1440 + minuteOfDay(noonUtc, zone) - 720;
	const at = new Date(Date.parse(noonUtc) - offsetMin * 60_000).toISOString();
	if (formatYmd(at, zone) !== ymd) throw new RangeError(`deadline ${ymd} moved in ${zone}`);
	return at;
}

function goalCategory(categories: Map<string, CategoryView>, goal: Goal | undefined): CategoryView {
	return (goal?.category_id && categories.get(goal.category_id)) || NO_CATEGORY;
}

/** Every action of the snapshot as the screens show it, with goal title and category slot. */
export function actionViews(s: Snapshot): ActionView[] {
	const categories = categoryViews(s);
	const goals = new Map(s.goals.map((g) => [g.id, g]));
	return s.actions.map((a) => {
		const goal = goals.get(a.goal_id);
		return { id: a.id, title: a.title, start: a.start_at, end: a.end_at, status: a.status, locked: a.locked, category: goalCategory(categories, goal).slot, goalTitle: goal?.title ?? '' };
	});
}

/** A tracker unit as a contract unit; unknown units count. */
export function unitOf(unit: string | null): Unit {
	return UNITS.find((u) => u === unit) ?? 'count';
}

function firstNumber(t: Tracker): TrackerField | undefined {
	return t.fields.find((f) => f.kind === 'number');
}

function personaFor(s: Snapshot, goalId: string): string {
	return s.personas.find((p) => p.goal_id === goalId)?.name ?? 'Aroh';
}

/**
 * Check-ins due now: trackers with a cadence due today (daily, or weekly on today's weekday)
 * whose local time has passed and that have no check-in entry today, each asking its first
 * number field. Output: one card per tracker, in tracker order.
 */
export function dueCheckIns(s: Snapshot, clock: Clock): DueCheckIn[] {
	const today = formatYmd(clock.now, clock.zone);
	const weekday = formatDate(today, { weekday: 'short' });
	const minute = minuteOfDay(clock.now, clock.zone);
	return s.trackers.flatMap((t) => {
		const field = firstNumber(t);
		const cadence = t.cadence;
		if (!field || !cadence) return [];
		const [h, m] = cadence.at.split(':').map(Number);
		const due = cadence.freq === 'daily' || cadence.weekdays.includes(weekday);
		const answered = s.entries.some((e) => e.tracker_id === t.id && e.source === 'check_in' && formatYmd(e.logged_at, clock.zone) === today);
		if (!due || answered || minute < h * 60 + m) return [];
		const spec: ComponentSpec = {
			kind: 'question_card',
			persona: personaFor(s, t.goal_id),
			question: `What is today's ${field.label.toLowerCase()}?`,
			input: { kind: 'number_input', label: field.label, unit: unitOf(field.unit), value: null, step: 0.1, ...range(points(s, t.id, field.key)) }
		};
		return [{ trackerId: t.id, key: field.key, spec }];
	});
}

/**
 * Slider bounds for a check-in from the field's history: a quarter below the lowest and above
 * the highest logged value, whole numbers. No history, or only zeros, gives only `min: 0`, so no slider shows.
 */
function range(history: Point[]): { min: number; max?: number } {
	if (history.length === 0) return { min: 0 };
	const values = history.map((p) => p.value);
	const min = Math.max(0, Math.floor(Math.min(...values) * 0.75));
	const max = Math.ceil(Math.max(...values) * 1.25);
	return max > min ? { min, max } : { min: 0 };
}

/** The entry data for a check-in answer, or null when no finite number was given. */
export function checkInData(due: DueCheckIn, answer: unknown): Record<string, number> | null {
	return typeof answer === 'number' && Number.isFinite(answer) ? { [due.key]: answer } : null;
}

function points(s: Snapshot, trackerId: string, key: string): Point[] {
	return s.entries
		.filter((e) => e.tracker_id === trackerId && typeof e.data[key] === 'number')
		.map((e) => ({ at: e.logged_at, value: e.data[key] as number }))
		.sort((a, b) => a.at.localeCompare(b.at));
}

function goalTrackers(s: Snapshot, goalId: string): { tracker: Tracker; field: TrackerField; points: Point[] }[] {
	return s.trackers
		.filter((t) => t.goal_id === goalId)
		.flatMap((tracker) => tracker.fields.filter((f) => f.kind === 'number').map((field) => ({ tracker, field, points: points(s, tracker.id, field.key) })))
		.filter((t) => t.points.length > 0);
}

/** The header facts of `goal`: category, deadline, autonomy, personas and its lead tracker's latest value. */
export function goalHeader(s: Snapshot, goal: Goal, zone: string): GoalHeader {
	const category = goalCategory(categoryViews(s), goal);
	const lead = goalTrackers(s, goal.id)[0];
	return {
		id: goal.id,
		title: goal.title,
		category: category.name,
		slot: category.slot,
		deadline: goal.deadline ? deadlineAt(goal.deadline, zone) : null,
		autonomy: AUTONOMY_LABEL[goal.autonomy],
		personas: s.personas.filter((p) => p.goal_id === goal.id).map((p) => p.name),
		lead: lead ? { unit: unitOf(lead.field.unit), now: lead.points[lead.points.length - 1].value } : null
	};
}

function streak(actions: Action[], today: string, zone: string): SpecOf<'streak_heatmap'>['days'] {
	return Array.from({ length: 28 }, (_, i) => {
		const date = shiftDay(today, i - 27);
		const day = actions.filter((a) => formatYmd(a.start_at, zone) === date);
		const done = day.filter((a) => a.status === 'done').length;
		const level = done === 0 ? 0 : done === day.length ? 3 : done * 2 >= day.length ? 2 : 1;
		return { date, level: level as 0 | 1 | 2 | 3 };
	});
}

/**
 * The goal dashboard until plugin view trees exist: countdown, this week's actions ring, one line
 * chart per number tracker field with entries, the last 28 days as a streak and the next 4
 * planned actions. Sections with nothing to show are left out.
 */
export function goalView(s: Snapshot, goal: Goal, clock: Clock): ViewSpec {
	const actions = s.actions.filter((a) => a.goal_id === goal.id);
	const today = formatYmd(clock.now, clock.zone);
	const week = mondayOf(today);
	const thisWeek = actions.filter((a) => {
		const d = formatYmd(a.start_at, clock.zone);
		return d >= week && d < shiftDay(week, 7);
	});
	const progress: ComponentSpec[] = [];
	if (goal.deadline) progress.push({ kind: 'countdown', label: `Until ${formatDate(goal.deadline, { day: 'numeric', month: 'short' })}`, deadline: deadlineAt(goal.deadline, clock.zone) });
	if (thisWeek.length) {
		const done = thisWeek.filter((a) => a.status === 'done').length;
		progress.push({ kind: 'progress_ring', label: 'This week', rings: [{ label: 'Actions', unit: 'count', value: done, target: thisWeek.length, series: 1 }] });
	}
	goalTrackers(s, goal.id).forEach((t, i) =>
		progress.push({ kind: 'line_chart', label: t.field.label, unit: unitOf(t.field.unit), points: t.points, series: ((i % 6) + 1) as 1 | 2 | 3 | 4 | 5 | 6 })
	);
	if (actions.length) progress.push({ kind: 'streak_heatmap', label: 'Last 4 weeks', days: streak(actions, today, clock.zone), series: 2 });
	const views = actionViews({ ...s, actions });
	const next = views.filter((a) => a.status === 'planned' && Date.parse(a.start) > Date.parse(clock.now)).slice(0, 4);
	const sections = [
		{ title: 'Progress', items: progress },
		{ title: 'Upcoming', items: [{ kind: 'action_list', label: 'Next sessions', actions: next } as ComponentSpec] }
	];
	return { title: goal.title, sections: sections.filter((sec) => sec.items.length > 0) };
}

/** The goals of the Goals list, archived ones left out, in creation order. */
export function goalRows(s: Snapshot, zone: string): GoalRow[] {
	const categories = categoryViews(s);
	return s.goals
		.filter((g) => g.status !== 'archived')
		.map((g) => {
			const c = goalCategory(categories, g);
			return { id: g.id, title: g.title, category: c.name, slot: c.slot, deadline: g.deadline ? deadlineAt(g.deadline, zone) : null, autonomy: AUTONOMY_LABEL[g.autonomy] };
		});
}

function changes(p: Proposal): ChangeView[] {
	const list = (p.diff as { changes?: unknown } | null)?.changes;
	if (!Array.isArray(list)) return [];
	return list.flatMap((c) => {
		const op = c?.op;
		if (!OPS.includes(op) || typeof c.title !== 'string') return [];
		const text = (v: unknown) => (typeof v === 'string' ? v : null);
		return [{ op, title: c.title, before: text(c.before), after: text(c.after) }];
	});
}

const WORKFLOW: Record<string, { title: string; persona: string }> = {
	'new-goal': { title: 'Planning a new goal', persona: 'Goal planner' },
	replan: { title: 'Replanning', persona: 'Adaptation analyst' },
	'log-intake': { title: 'Sorting a log', persona: 'Log intake' }
};

const RUN_STATE: Record<string, string> = {
	queued: 'Waiting to start',
	running: 'Working',
	checkpointed: 'Working',
	waiting_human: 'Waiting for you',
	waiting_budget: 'Paused at a budget',
	failed: 'Stopped'
};

/**
 * Runs not done, newest first, one per goal and workflow, titled by workflow and goal. Progress is
 * coarse by state: 0 queued, 50 working, 100 stopped at a wait, a pause or a failure.
 */
function openRuns(s: Snapshot, runs: RunSummary[]): RunView[] {
	const seen = new Set<string>();
	const out: RunView[] = [];
	for (const r of runs) {
		const key = `${r.goal_id}:${r.workflow}`;
		if (r.state === 'done' || seen.has(key)) continue;
		seen.add(key);
		const goal = s.goals.find((g) => g.id === r.goal_id);
		const kind = WORKFLOW[r.workflow] ?? { title: r.workflow, persona: 'Aroh' };
		const progress = r.state === 'queued' ? 0 : r.state === 'running' || r.state === 'checkpointed' ? 50 : 100;
		out.push({ persona: kind.persona, title: goal ? `${kind.title}: ${goal.title}` : `${kind.title} (${r.run_id.slice(0, 8)})`, step: RUN_STATE[r.state] ?? r.state, progress });
	}
	return out;
}

/** The Inbox groups: open proposals, unconfirmed preferences, and the engine runs not done. */
export function inboxView(s: Snapshot, runs: RunSummary[] = []): { approvals: InboxItem[]; suggestions: InboxItem[]; insights: InboxItem[]; runs: RunView[] } {
	return {
		approvals: s.proposals.map((p) => ({ id: p.id, spec: { kind: 'proposal_card', persona: personaFor(s, p.goal_id), summary: p.summary, changes: changes(p) } })),
		suggestions: s.preferences.map((f) => ({ id: f.id, spec: { kind: 'proposal_card', persona: f.source, summary: f.fact, changes: [] } })),
		insights: [],
		runs: openRuns(s, runs)
	};
}

/** Items in the Inbox, for the tab badge. */
export function inboxCount(s: Snapshot): number {
	return s.proposals.length + s.preferences.length;
}

/**
 * The Settings values: provider list with the last 4 of each vendor's stored key (`last4`, from
 * `keyStatus`), search, URL, budgets, access and the account (`email` null when signed out).
 */
export function settingsView(s: Snapshot, last4: Record<string, string | null> = {}, email: string | null = null): SettingsView {
	const chosen = s.providers.find((p) => p.is_default);
	const known = catalog.find((p) => p.id === chosen?.vendor);
	const all = s.budgets.find((b) => b.scope === 'user_month');
	const perGoal = Number(s.settings.goal_budget_usd);
	return {
		providers: catalog.map((p) => ({ ...p, keySet: Boolean(last4[p.id]), last4: last4[p.id] ?? null })),
		providerId: known?.id ?? catalog[0].id,
		model: known && chosen ? chosen.model : catalog[0].models[0],
		search: [...s.search].sort((a, b) => a.priority - b.priority).map((x) => ({ name: x.label, detail: x.base_url ?? 'API key', keySet: false })),
		serverUrl: s.settings.server_url ?? '',
		budgetAll: all ? all.limit_usd_micros / 1_000_000 : null,
		budgetGoal: s.settings.goal_budget_usd !== undefined && Number.isFinite(perGoal) ? perGoal : null,
		calendars: s.settings.calendars === 'true',
		health: s.settings.health === 'true',
		account: { supabaseUrl: s.settings.supabase_url ?? '', anonKey: s.settings.supabase_anon_key ?? '', email }
	};
}

/** The Log screen: due check-ins, texts waiting for the engine and the 20 newest entries. */
export function logView(s: Snapshot, clock: Clock): { checkIns: DueCheckIn[]; waiting: SpecOf<'log_list'>; recent: SpecOf<'log_list'> } {
	const trackers = new Map(s.trackers.map((t) => [t.id, t]));
	const recent = s.entries.slice(0, RECENT).map((e) => {
		const tracker = trackers.get(e.tracker_id);
		const field = tracker?.fields.find((f) => typeof e.data[f.key] === 'number');
		const value = field ? (e.data[field.key] as number) : undefined;
		return { at: e.logged_at, text: e.raw_text ?? tracker?.name ?? 'Entry', value, unit: field ? unitOf(field.unit) : undefined };
	});
	return {
		checkIns: dueCheckIns(s, clock),
		waiting: { kind: 'log_list', label: 'Waiting for the engine', entries: [...s.pending_logs].reverse().map((p) => ({ at: p.at, text: p.text })) },
		recent: { kind: 'log_list', label: 'Recent entries', entries: recent }
	};
}
