import type { BusyView, InboxItem } from '$lib/ui/screens/types';
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

/** The groups and events of a waiting calendar-triage `decide` node, or null for any other node. */
export function triageOf(w: Waiting | undefined): { groups: TriageGroup[]; events: TriageShown[] } | null {
	if (w?.node !== 'decide') return null;
	const sorted = w.data.sorted as { groups?: TriageGroup[] } | undefined;
	return { groups: sorted?.groups ?? [], events: (w.data.events as TriageShown[] | undefined) ?? [] };
}

/** Joins titles as "A", "A and B" or "A, B and C". */
function list(titles: string[]): string {
	return titles.length < 2 ? (titles[0] ?? '') : `${titles.slice(0, -1).join(', ')} and ${titles[titles.length - 1]}`;
}

/** One question card per group: name, event count and up to three titles, Keep or Ignore preselected from the suggestion. */
export function triageCards(w: Waiting | undefined): InboxItem[] {
	const t = triageOf(w);
	if (!t) return [];
	return t.groups.map((g) => {
		const titles = [...new Set(g.events.map((k) => t.events.find((e) => e.key === k)?.title ?? '').filter(Boolean))].slice(0, SAMPLES);
		const count = `${g.events.length} ${g.events.length === 1 ? 'event' : 'events'}`;
		const like = titles.length ? `, like ${list(titles)}` : '';
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
 * busy blocks), and sends the unmatched timed events to triage unless a triage run is still open.
 * Returns busy blocks kept, events sent and the open or new triage run.
 */
export async function connect(deps: CalendarDeps, now = Date.now()): Promise<{ kept: number; sent: number; run: string | null }> {
	const events = await deps.read(new Date(now - PAST_DAYS * DAY_MS).toISOString(), new Date(now + AHEAD_DAYS * DAY_MS).toISOString());
	const report = await deps.importEvents(events);
	const unmatched = report.unmatched.filter((e) => !e.all_day).slice(0, MAX_TRIAGE);
	if (!unmatched.length) return { kept: report.kept, sent: 0, run: null };
	const runs = await deps.runs();
	const open = runs.ok ? runs.value.find((r) => r.workflow === WORKFLOW && !DONE.includes(r.state)) : undefined;
	if (open) return { kept: report.kept, sent: 0, run: open.run_id };
	const started = await deps.triage(unmatched.map(({ event_id, calendar_id, calendar, title, start_at, end_at, rrule }) => ({ event_id, calendar_id, calendar, title, start_at, end_at, rrule })));
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

/** The snapshot's busy blocks as neutral Calendar views, named by their triage group. */
export function busyViews(s: client.Snapshot): BusyView[] {
	return (s.busy ?? []).map((b) => ({ id: b.id, title: b.title, start: b.start_at, end: b.end_at, group: b.group_name ?? 'Busy' }));
}
