import type { Clock } from '$lib/ui/clock';
import type { ActionView, CategorySlot, ChangeView, ComponentSpec, Point, SpecOf, Unit, ViewSpec } from '$lib/ui/contract';
import { daysBetween, formatDate, formatDay, formatTime, formatYmd, minuteOfDay, mondayOf, shiftDay } from '$lib/ui/format';
import type { AutonomyLabel, CategoryView, GoalHeader, GoalRow, InboxItem, ProfileView, RunView, SettingsView } from '$lib/ui/screens/types';
import { catalog } from './catalog';
import type { Action, Goal, Proposal, Snapshot, Tracker, TrackerEntry, TrackerField } from './client';
import { ACTIVE, type Insight, type RunSummary } from './engine';
import type { Tone } from '$lib/ui/components/StatusChip.svelte';
import { PACE_LABEL, type GoalStats } from './dashboard';

const SEEDED_SLOTS: Record<string, CategorySlot> = { Learning: 1, 'Health and Fitness': 2, Financial: 3, 'Hobby and Skill': 4, 'Career and Projects': 5, Habit: 6 };

/** The 6 category hues as stored on category rows (`#rrggbb`, the seeded colors), in slot order 1 to 6. */
export const SLOT_HUES: Record<CategorySlot, string> = { 1: '#4a6fa5', 2: '#4f7a5a', 3: '#a5803a', 4: '#8a6a9a', 5: '#5a6b7a', 6: '#7a5a4f' };

const HUE_SLOT: Record<string, CategorySlot> = Object.fromEntries(Object.entries(SLOT_HUES).map(([slot, hue]) => [hue, Number(slot) as CategorySlot]));
const UNITS: readonly Unit[] = ['kg', 'lb', 'cm', 'kcal', 'g', 'ml', 'L', 'min', 'reps', 'steps', 'usd', 'count'];
const OPS: readonly ChangeView['op'][] = ['move', 'resize', 'create', 'delete'];
const NO_CATEGORY: CategoryView = { name: 'No category', slot: 6 };
const RECENT = 20;

/** Autonomy level of a goal row as the screens label it. */
export const AUTONOMY_LABEL: Record<Goal['autonomy'], AutonomyLabel> = { ask: 'Ask', balanced: 'Balanced', auto: 'Auto' };

/** A check-in the owner owes now: its tracker, the field it asks and the question card. */
export type DueCheckIn = { trackerId: string; key: string; spec: ComponentSpec };

/** The slot of a stored category color, or undefined for a color outside the 6 hues. */
export function hueSlot(color: string): CategorySlot | undefined {
	return HUE_SLOT[color.toLowerCase()];
}

/** Every category with its colour slot: by its stored hue, else seeded names by the fixed table, else id order. */
export function categoryViews(s: Snapshot): Map<string, CategoryView> {
	const own = s.categories.filter((c) => !hueSlot(c.color) && !(c.name in SEEDED_SLOTS)).map((c) => c.id).sort();
	return new Map(
		s.categories.map((c) => [c.id, { name: c.name, slot: hueSlot(c.color) ?? SEEDED_SLOTS[c.name] ?? (((own.indexOf(c.id) % 6) + 1) as CategorySlot) }])
	);
}

/** The categories the New Goal picker offers, with ids, in slot order then name. */
export function categoryChoices(s: Snapshot): { id: string; name: string; slot: CategorySlot }[] {
	const views = categoryViews(s);
	return s.categories.map((c) => ({ id: c.id, ...views.get(c.id)! })).sort((a, b) => a.slot - b.slot || a.name.localeCompare(b.name));
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

/** The goal's persona name, or the planner persona's when the goal has no persona row. */
function personaFor(s: Snapshot, goalId: string): string {
	return s.personas.find((p) => p.goal_id === goalId)?.name ?? WORKFLOW['new-goal'].persona;
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

/** The numeric values of field `key` of tracker `trackerId`, oldest first, as chart points. */
export function points(s: Snapshot, trackerId: string, key: string): Point[] {
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

/** The header facts of `goal`: category, deadline, autonomy and its lead tracker's latest value. */
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
		lead: lead ? { unit: unitOf(lead.field.unit), now: lead.points[lead.points.length - 1].value } : null
	};
}

/**
 * The 28 days up to `today` as heatmap days: level 0 with no done action, 3 when all of the day's
 * actions are done, 2 for at least half, else 1.
 */
export function streak(actions: Action[], today: string, zone: string): SpecOf<'streak_heatmap'>['days'] {
	return Array.from({ length: 28 }, (_, i) => {
		const date = shiftDay(today, i - 27);
		const day = actions.filter((a) => formatYmd(a.start_at, zone) === date);
		const done = day.filter((a) => a.status === 'done').length;
		const level = done === 0 ? 0 : done === day.length ? 3 : done * 2 >= day.length ? 2 : 1;
		return { date, level: level as 0 | 1 | 2 | 3 };
	});
}

/** The actions of `actions` that start in the current Monday-to-Sunday week of the clock's zone. */
export function weekActions(actions: Action[], clock: Clock): Action[] {
	const week = mondayOf(formatYmd(clock.now, clock.zone));
	return actions.filter((a) => {
		const d = formatYmd(a.start_at, clock.zone);
		return d >= week && d < shiftDay(week, 7);
	});
}

/** The first `limit` planned actions of `views` that start after the clock's now, in their order. */
export function nextPlanned(views: ActionView[], clock: Clock, limit: number): ActionView[] {
	return views.filter((a) => a.status === 'planned' && Date.parse(a.start) > Date.parse(clock.now)).slice(0, limit);
}

/** One tracker entry as a log list line: raw text or tracker name, and its first numeric value. */
export function entryView(e: TrackerEntry, tracker: Tracker | undefined): SpecOf<'log_list'>['entries'][number] {
	const field = tracker?.fields.find((f) => typeof e.data[f.key] === 'number');
	const value = field ? (e.data[field.key] as number) : undefined;
	return { at: e.logged_at, text: e.raw_text ?? tracker?.name ?? 'Entry', value, unit: field ? unitOf(field.unit) : undefined };
}

/**
 * The derived goal dashboard, shown when the goal has no plugin: countdown, this week's actions ring, one line
 * chart per number tracker field with entries, the last 28 days as a streak and the next 4
 * planned actions. Sections with nothing to show are left out.
 */
export function goalView(s: Snapshot, goal: Goal, clock: Clock): ViewSpec {
	const actions = s.actions.filter((a) => a.goal_id === goal.id);
	const today = formatYmd(clock.now, clock.zone);
	const thisWeek = weekActions(actions, clock);
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
	const next = nextPlanned(actionViews({ ...s, actions }), clock, 4);
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

/** A goal's line on the Goals list: status chip tone and label, done and due sessions to date, and its next step or null. */
export type GoalProgress = { tone: Tone; label: string; done: number; due: number; line: string | null };

const PACE_TONE: Record<GoalStats['pace'], Tone> = { on_track: 'done', ahead: 'done', behind: 'missed' };

/**
 * The status, progress and next step of every non-archived goal, by goal id. Draft is Planning,
 * paused Paused, done Done; an active goal with a proposal waiting is Waiting for you, else its
 * dashboard pace (`stats`; null while loading gives Active). The line names the next planned
 * session, else the sessions missed this week, else null.
 */
export function goalProgress(s: Snapshot, stats: GoalStats[] | null, clock: Clock): Record<string, GoalProgress> {
	const today = formatYmd(clock.now, clock.zone);
	const figures = new Map((stats ?? []).map((st) => [st.goal_id, st]));
	const waiting = new Set(s.proposals.filter((p) => p.status === 'proposed').map((p) => p.goal_id));
	const out: Record<string, GoalProgress> = {};
	for (const g of s.goals.filter((g) => g.status !== 'archived')) {
		const st = figures.get(g.id);
		const actions = s.actions.filter((a) => a.goal_id === g.id).sort((a, b) => a.start_at.localeCompare(b.start_at));
		const next = actions.find((a) => a.status === 'planned' && Date.parse(a.start_at) > Date.parse(clock.now));
		const missed = weekActions(actions, clock).filter((a) => a.status === 'missed').length;
		const status: { tone: Tone; label: string } =
			g.status === 'draft'
				? { tone: 'planned', label: 'Planning' }
				: g.status === 'paused'
					? { tone: 'skipped', label: 'Paused' }
					: g.status === 'done'
						? { tone: 'done', label: 'Done' }
						: waiting.has(g.id)
							? { tone: 'warn', label: 'Waiting for you' }
							: st
								? { tone: PACE_TONE[st.pace], label: PACE_LABEL[st.pace] }
								: { tone: 'planned', label: 'Active' };
		const when = next && (formatYmd(next.start_at, clock.zone) === today ? 'today' : formatDay(next.start_at, clock.zone));
		const line = next ? `Next: ${next.title}, ${when} ${formatTime(next.start_at, clock.zone)}` : missed ? `${missed} ${missed === 1 ? 'session' : 'sessions'} missed this week` : null;
		console.assert(!st || (st.done >= 0 && st.due >= 0), 'dashboard counts are never negative');
		out[g.id] = { ...status, done: st?.done ?? 0, due: st?.due ?? 0, line };
	}
	return out;
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
	'run-again': { title: 'Planning the goal again', persona: 'Goal planner' },
	replan: { title: 'Replanning', persona: 'Adaptation analyst' },
	'log-intake': { title: 'Sorting a log', persona: 'Log intake' },
	'learn-preference': { title: 'Learning from your edits', persona: 'Preference analyst' },
	'calendar-triage': { title: 'Sorting your calendar', persona: 'Calendar triage' }
};

const RUN_STATE: Record<string, { step: string; tone: Tone }> = {
	queued: { step: 'Waiting to start', tone: 'planned' },
	running: { step: 'Working', tone: 'planned' },
	checkpointed: { step: 'Working', tone: 'planned' },
	waiting_human: { step: 'Waiting for you', tone: 'warn' },
	waiting_budget: { step: 'Paused at a budget', tone: 'warn' },
	failed: { step: 'Stopped', tone: 'skipped' }
};

/**
 * Runs not done, newest first, one per goal and workflow, titled by workflow and goal, with a
 * status tone and `working` (spins) only while the run still moves on its own. Progress is
 * coarse by state: 0 queued, 50 working, 100 stopped at a wait, a pause or a failure. A run that
 * waits for the owner says what for (`waits`, by run id, such as "3 questions to answer") and
 * links to its goal's chat.
 */
function openRuns(s: Snapshot, runs: RunSummary[], waits: Record<string, string>): RunView[] {
	const seen = new Set<string>();
	const out: RunView[] = [];
	for (const r of runs) {
		const key = `${r.goal_id}:${r.workflow}`;
		if (r.state === 'done' || seen.has(key)) continue;
		seen.add(key);
		const goal = s.goals.find((g) => g.id === r.goal_id);
		const kind = WORKFLOW[r.workflow] ?? { title: r.workflow, persona: 'Aroh' };
		const progress = r.state === 'queued' ? 0 : r.state === 'running' || r.state === 'checkpointed' ? 50 : 100;
		const wait = r.state === 'waiting_human' ? waits[r.run_id] : undefined;
		const state = RUN_STATE[r.state] ?? { step: r.state, tone: 'skipped' };
		const step = wait ? `Waiting for you: ${wait}` : state.step;
		const working = ACTIVE.includes(r.state);
		console.assert(!working || state.tone === 'planned', 'only a working run is drawn as working');
		const href = r.state === 'waiting_human' && r.goal_id ? `/goals/${r.goal_id}/chat` : undefined;
		out.push({ persona: kind.persona, title: goal ? `${kind.title}: ${goal.title}` : `${kind.title} (${r.run_id.slice(0, 8)})`, step, progress, tone: state.tone, working, ...(href ? { href } : {}) });
	}
	return out;
}

/**
 * Device notes (newest first, from the Planner) then engine insights as insight cards: the persona
 * that acted, the title with the goal's, and the body. Cards dismissed on this device (the
 * `dismissed_insights` setting) are left out.
 */
function insightItems(s: Snapshot, insights: Insight[]): InboxItem[] {
	const dismissed = new Set((s.settings.dismissed_insights ?? '').split(','));
	const notes: InboxItem[] = (s.notes ?? []).map((n) => ({ id: n.id, spec: { kind: 'insight_card', persona: 'Planner', title: n.title, body: n.body } }));
	return notes.concat(insights.map((i) => {
		const goal = s.goals.find((g) => g.id === i.goal_id);
		const persona = i.source === 'edits' ? 'Preference analyst' : 'Adaptation analyst';
		return { id: i.id, spec: { kind: 'insight_card', persona, title: goal ? `${i.title}: ${goal.title}` : i.title, body: i.body } };
	})).filter((i) => !dismissed.has(i.id));
}

/** Joins titles as "A", "A and B" or "A, B and C". */
export function joinTitles(titles: string[]): string {
	return titles.length < 2 ? (titles[0] ?? '') : `${titles.slice(0, -1).join(', ')} and ${titles[titles.length - 1]}`;
}

/**
 * Neutral "Calendar" notes from the day of `now` on (local days in `zone`): one per Busy all-day
 * block still holding planned actions, locked ones included (id = the block id), then one per
 * locked planned action overlapping a timed busy block (id = the action id). Dismissed ones
 * (`dismissed_insights`) are left out.
 */
export function busyDayInsights(s: Snapshot, zone: string, now: string): InboxItem[] {
	const dismissed = new Set((s.settings.dismissed_insights ?? '').split(','));
	const today = formatYmd(now, zone);
	const planned = s.actions.filter((a) => a.status === 'planned').sort((a, b) => a.start_at.localeCompare(b.start_at));
	const days = (s.busy ?? [])
		.filter((b) => b.all_day && b.day_kind === 'busy' && !dismissed.has(b.id))
		.flatMap((b) => {
			const first = formatYmd(b.start_at, zone);
			const lastAt = new Date(Date.parse(b.end_at) - 1000).toISOString();
			const last = formatYmd(lastAt, zone);
			const left = planned.filter((a) => {
				const day = formatYmd(a.start_at, zone);
				return day >= today && day >= first && day <= last;
			});
			if (!left.length) return [];
			const days = first < last ? `${formatDay(b.start_at, zone)} to ${formatDay(lastAt, zone)}` : formatDay(b.start_at, zone);
			const count = `${left.length} ${left.length === 1 ? 'action' : 'actions'}`;
			const titles = left.map((a) => a.title);
			const names = titles.length > 3 ? `${titles.slice(0, 3).join(', ')} and ${titles.length - 3} more` : joinTitles(titles);
			const body = `${b.title} on ${days}: ${names} could not move. Move them by hand or change the day's kind.`;
			return [{ id: b.id, spec: { kind: 'insight_card' as const, persona: 'Calendar', title: `No free day for ${count}`, body } }];
		});
	const timed = (s.busy ?? []).filter((b) => !b.all_day);
	const clashes = planned
		.filter((a) => a.locked && !dismissed.has(a.id) && formatYmd(a.start_at, zone) >= today)
		.flatMap((a) => {
			const hit = timed.find((b) => b.start_at < a.end_at && a.start_at < b.end_at);
			if (!hit) return [];
			const body = `${a.title} on ${formatDay(a.start_at, zone)} overlaps ${hit.title}. Fixed actions are never moved: move it by hand or turn off Fixed time.`;
			return [{ id: a.id, spec: { kind: 'insight_card' as const, persona: 'Calendar', title: `Fixed action clashes: ${a.title}`, body } }];
		});
	return [...days, ...clashes];
}

/**
 * The Inbox groups: open proposals, unconfirmed preferences, engine insights then Busy-day notes
 * (local days in `zone`, from the day of `now`), and the engine runs not done, each with what it
 * waits for from `waits` (by run id).
 */
export function inboxView(
	s: Snapshot,
	runs: RunSummary[] = [],
	insights: Insight[] = [],
	zone = 'UTC',
	now = new Date().toISOString(),
	waits: Record<string, string> = {}
): { approvals: InboxItem[]; suggestions: InboxItem[]; insights: InboxItem[]; runs: RunView[] } {
	return {
		approvals: s.proposals.map((p) => ({ id: p.id, spec: { kind: 'proposal_card', persona: personaFor(s, p.goal_id), summary: p.summary, changes: changes(p) } })),
		suggestions: s.preferences.map((f) => ({ id: f.id, spec: { kind: 'proposal_card', persona: f.source, summary: f.fact, changes: [] } })),
		insights: [...insightItems(s, insights), ...busyDayInsights(s, zone, now)],
		runs: openRuns(s, runs, waits)
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
	const recent = s.entries.slice(0, RECENT).map((e) => entryView(e, trackers.get(e.tracker_id)));
	return {
		checkIns: dueCheckIns(s, clock),
		waiting: { kind: 'log_list', label: 'Waiting for the engine', entries: [...s.pending_logs].reverse().map((p) => ({ at: p.at, text: p.text })) },
		recent: { kind: 'log_list', label: 'Recent entries', entries: recent }
	};
}

/**
 * "What Aroh knows about you" from the snapshot's kept facts: each fact with its source (the
 * goal whose answer it came from, or the owner's edits) and date, newest first, and the blocked
 * topics apart with their label.
 */
export function profileView(s: Snapshot): ProfileView {
	const title = (id: string | null) => s.goals.find((g) => g.id === id)?.title;
	const live = [...s.facts].sort((a, b) => b.noted_at.localeCompare(a.noted_at));
	return {
		facts: live
			.filter((f) => !f.blocked)
			.map((f) => {
				const goal = f.origin === 'answer' ? title(f.goal_id) : undefined;
				const source = f.origin === 'edit' ? 'From your edits' : goal ? `From your answer on ${goal}` : 'From your answers';
				return { id: f.id, text: f.fact, source, date: f.noted_at, blockable: f.topic !== null };
			}),
		blocked: live.filter((f) => f.blocked).map((f) => ({ id: f.id, label: f.fact }))
	};
}
