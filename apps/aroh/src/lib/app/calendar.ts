import type { AllDayView, BusyView, DayKind, InboxItem } from '$lib/ui/screens/types';
import { formatYmd } from '$lib/ui/format';
import { joinTitles } from './views';
import * as client from './client';
import { follow } from './chat.svelte';
import { engine, type EngineResult, type RunSummary, type TriageEvent, type Waiting } from './engine';

/** The Choice option that keeps a group as busy time. */
export const KEEP = 'Keep as busy time';
/** The Choice option that ignores a group. */
export const IGNORE = 'Ignore';

const DAY_MS = 86_400_000;
const PAST_DAYS = 1;
const AHEAD_DAYS = 14;
const MAX_TRIAGE = 300;
const SAMPLES = 3;
const DONE = ['done', 'failed'];
const WORKFLOW = 'calendar-triage';

/** One triage group as the `decide` node shows it. */
export type TriageGroup = { name: string; kind: string; keep: boolean; reason: string; events: string[] };

/** One triaged event as the `decide` node shows it, in local time. */
export type TriageShown = { key: string; calendar: string; title: string; start: string; minutes: number; rrule: string | null };

/** What the connect flow needs, injectable for tests. */
export type CalendarDeps = {
	read: (from: string, to: string) => Promise<client.DeviceEvent[]>;
	importEvents: (events: client.DeviceEvent[]) => Promise<client.CalendarImport>;
	runs: () => Promise<EngineResult<RunSummary[]>>;
	triage: (events: TriageEvent[]) => Promise<EngineResult<{ goal_id: null; run_id: string }>>;
};

const KINDS: readonly DayKind[] = ['reminder', 'busy', 'away'];

/** The groups and events of a waiting calendar-triage `decide` node, or null for any other node. */
export function triageOf(w: Waiting | undefined): { groups: TriageGroup[]; events: TriageShown[] } | null {
	if (w?.node !== 'decide') return null;
	const sorted = w.data.sorted as { groups?: TriageGroup[] } | undefined;
	return { groups: sorted?.groups ?? [], events: (w.data.events as TriageShown[] | undefined) ?? [] };
}

/**
 * The day kinds the triage persona proposed for all-day events, keyed by device event id through
 * the run's `refs`. Unknown keys and kinds are dropped; any node but `decide` gives none.
 */
export function triageKinds(w: Waiting | undefined): Record<string, DayKind> {
	if (w?.node !== 'decide') return {};
	const days = ((w.data.sorted as { days?: { event: string; kind: string }[] } | undefined)?.days ?? []);
	const refs = (w.data.refs as { key: string; event_id: string }[] | undefined) ?? [];
	const out: Record<string, DayKind> = {};
	for (const d of days) {
		const ref = refs.find((r) => r.key === d.event);
		const kind = KINDS.find((k) => k === d.kind);
		if (ref && kind) out[ref.event_id] = kind;
	}
	return out;
}

/** One question card per group: name, event count and up to three titles, Keep or Ignore preselected from the suggestion. */
export function triageCards(w: Waiting | undefined): InboxItem[] {
	const t = triageOf(w);
	if (!t) return [];
	return t.groups.map((g) => {
		const titles = [...new Set(g.events.map((k) => t.events.find((e) => e.key === k)?.title ?? '').filter(Boolean))].slice(0, SAMPLES);
		const count = `${g.events.length} ${g.events.length === 1 ? 'event' : 'events'}`;
		const like = titles.length ? `, like ${joinTitles(titles)}` : '';
		return {
			id: g.name,
			spec: {
				kind: 'question_card',
				persona: 'Calendar triage',
				question: `${g.name}: ${count}${like}. Keep them as busy time?`,
				input: { kind: 'choice', label: g.reason, options: [KEEP, IGNORE], multi: false, value: [g.keep ? KEEP : IGNORE] }
			}
		};
	});
}

/**
 * The `decide` answer once every group has an entry in `answers` (keyed by group name), else null.
 * A card sent untouched answers undefined and takes the group's suggestion.
 */
export function decisionsFrom(w: Waiting | undefined, answers: Record<string, unknown>): { group: string; keep: boolean }[] | null {
	const t = triageOf(w);
	if (!t || !t.groups.every((g) => g.name in answers)) return null;
	return t.groups.map((g) => {
		const picked = answers[g.name];
		const keep = Array.isArray(picked) && picked.length ? picked[0] === KEEP : g.keep;
		return { group: g.name, keep };
	});
}

/**
 * Reads the device calendars for the last day and the next 14, imports them (kept events become
 * busy blocks), and sends the unmatched events, all-day ones flagged, to triage unless a triage
 * run is still open. Returns busy blocks kept, events sent and the open or new triage run.
 */
export async function connect(deps: CalendarDeps, now = Date.now()): Promise<{ kept: number; sent: number; run: string | null }> {
	const events = await deps.read(new Date(now - PAST_DAYS * DAY_MS).toISOString(), new Date(now + AHEAD_DAYS * DAY_MS).toISOString());
	const report = await deps.importEvents(events);
	const unmatched = report.unmatched.slice(0, MAX_TRIAGE);
	if (!unmatched.length) return { kept: report.kept, sent: 0, run: null };
	const runs = await deps.runs();
	const open = runs.ok ? runs.value.find((r) => r.workflow === WORKFLOW && !DONE.includes(r.state)) : undefined;
	if (open) return { kept: report.kept, sent: 0, run: open.run_id };
	const started = await deps.triage(unmatched.map(({ event_id, calendar_id, calendar, title, start_at, end_at, rrule, all_day }) => ({ event_id, calendar_id, calendar, title, start_at, end_at, rrule, all_day })));
	return { kept: report.kept, sent: started.ok ? unmatched.length : 0, run: started.ok ? started.value.run_id : null };
}

/** The real device and engine calls behind `connect`. */
export const deviceCalendar: CalendarDeps = {
	read: client.readCalendar,
	importEvents: client.calendarImport,
	runs: () => engine.runs(),
	triage: (events) => engine.triage(events)
};

/**
 * Runs `connect` on the device and follows a new or open triage run until it waits for the owner,
 * so the Inbox shows its groups. Off iPhone, or with access off, it rejects with the plugin's error.
 */
export async function syncCalendars(): Promise<void> {
	const { run } = await connect(deviceCalendar);
	if (run) await follow(run);
}

/**
 * After the owner decided: follows the run to its end (which syncs the new rules down), imports
 * again so kept events become busy blocks, then syncs to push them for the planner.
 */
export async function finishTriage(runId: string): Promise<void> {
	await follow(runId);
	await connect(deviceCalendar);
	await client.sync().catch(() => null);
}

/** The snapshot's timed busy blocks as neutral Calendar views, named by their triage group. */
export function busyViews(s: client.Snapshot): BusyView[] {
	return (s.busy ?? []).filter((b) => !b.all_day).map((b) => ({ id: b.id, title: b.title, start: b.start_at, end: b.end_at, group: b.group_name ?? 'Busy' }));
}

/**
 * The snapshot's all-day blocks as chips: first and last local day in `zone`, inclusive (EventKit
 * ends a day at the next midnight or at 23:59:59), the confirmed kind and, until a kind is
 * confirmed, the stored triage suggestion. Sorted by first day, then title.
 */
export function allDayViews(s: client.Snapshot, zone: string): AllDayView[] {
	const suggested = storedSuggestions(s);
	return (s.busy ?? [])
		.filter((b) => b.all_day)
		.map((b) => {
			const start = formatYmd(b.start_at, zone);
			const last = formatYmd(new Date(Date.parse(b.end_at) - 1000).toISOString(), zone);
			return { id: b.id, title: b.title, start, end: last < start ? start : last, kind: b.day_kind, suggested: b.day_kind ? null : (suggested[b.event_id] ?? null) };
		})
		.sort((a, b) => a.start.localeCompare(b.start) || a.title.localeCompare(b.title));
}

/** Saves the owner's kind for an all-day event, then syncs so the engine plans around it. */
export async function confirmDay(id: string, kind: DayKind): Promise<client.DaySet> {
	const set = await client.setDayKind(id, kind);
	await client.sync().catch(() => null);
	return set;
}

/**
 * The calendar-triage run waiting at `decide`, if any: its id and waiting node. Reads the run list
 * unless `known` is given. Offline gives null.
 */
export async function openTriage(known?: RunSummary[]): Promise<{ runId: string; waiting: Waiting } | null> {
	let runs = known;
	if (!runs) {
		const read = await engine.runs();
		runs = read.ok ? read.value : [];
	}
	const open = runs.find((r) => r.workflow === WORKFLOW && r.state === 'waiting_human');
	if (!open) return null;
	const view = await engine.run(open.run_id);
	const waiting = view.ok ? view.value.waiting.find((w) => w.node === 'decide') : undefined;
	return waiting ? { runId: open.run_id, waiting } : null;
}

/** The triage suggestions kept on this device (`day_suggestions`), by device event id; unreadable gives none. */
function storedSuggestions(s: client.Snapshot): Record<string, DayKind> {
	let parsed: unknown;
	try {
		parsed = JSON.parse(s.settings?.day_suggestions ?? '{}');
	} catch {
		return {};
	}
	if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return {};
	const out: Record<string, DayKind> = {};
	for (const [id, kind] of Object.entries(parsed)) {
		const known = KINDS.find((k) => k === kind);
		if (known) out[id] = known;
	}
	return out;
}

/**
 * The stored suggestions with the new `picks` laid over them, kept only for events that still
 * have an all-day block with no confirmed kind in `s`, at most 300.
 */
export function mergeSuggestions(stored: Record<string, DayKind>, picks: Record<string, DayKind>, s: client.Snapshot): Record<string, DayKind> {
	const open = new Set((s.busy ?? []).filter((b) => b.all_day && b.day_kind === null).map((b) => b.event_id));
	const kept = Object.entries({ ...stored, ...picks }).filter(([id]) => open.has(id));
	return Object.fromEntries(kept.slice(0, MAX_TRIAGE));
}

/** What `refreshSuggestions` needs, injectable for tests. */
export type SuggestionDeps = {
	open: () => Promise<{ runId: string; waiting: Waiting } | null>;
	save: (value: string) => Promise<unknown>;
	answer: (runId: string, value: unknown) => Promise<EngineResult<unknown>>;
	finish: (runId: string) => Promise<void>;
};

/** The real calls behind `refreshSuggestions`; `known` runs spare a run list read. */
export function deviceSuggestions(known?: RunSummary[]): SuggestionDeps {
	return {
		open: () => openTriage(known),
		save: (value) => client.setSetting('day_suggestions', value),
		answer: (runId, value) => engine.answer(runId, 'decide', value),
		finish: finishTriage
	};
}

/**
 * Keeps the triage day suggestions on this device: merges the picks of the run waiting at
 * `decide` into the stored ones, prunes confirmed or gone events and saves on a change. A run
 * with no group (all-day events only) is answered with no decisions and followed to its end.
 * Output: the run still waiting for the owner's group answers, or null.
 */
export async function refreshSuggestions(s: client.Snapshot, deps: SuggestionDeps = deviceSuggestions()): Promise<{ runId: string; waiting: Waiting } | null> {
	const open = await deps.open();
	const stored = storedSuggestions(s);
	const merged = mergeSuggestions(stored, triageKinds(open?.waiting), s);
	if (JSON.stringify(merged) !== JSON.stringify(stored)) await deps.save(JSON.stringify(merged));
	if (!open || triageOf(open.waiting)?.groups.length !== 0) return open;
	const answered = await deps.answer(open.runId, []);
	if (answered.ok) await deps.finish(open.runId);
	return null;
}
