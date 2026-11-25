import type { Tone } from '$lib/ui/components/StatusChip.svelte';
import { ACTIVE, formatCost, pauseMessage, PLANNING, progressLines, retryMessage, stopMessage, waitLabel, type RunAnswer, type RunEvent, type RunSummary, type RunView } from './engine';

/**
 * What the goal card's status header shows: chip tone and text, a title, a detail line and which
 * run buttons apply: Stop, Retry, and Change time limit (`limit`) for a run its time limit stopped.
 */
export type GoalStatus = { tone: Tone; chip: string; title: string; detail: string; stop: boolean; retry: boolean; limit: boolean };

const MINUTE_MS = 60_000;

/**
 * The run the goal card follows: the newest one that waits for the owner, else the newest run.
 * `runs` is newest first, as `GET /v1/runs` lists them. Null when the goal has no run.
 */
export function pickRun(runs: RunSummary[]): string | null {
	return (runs.find((r) => r.state === 'waiting_human') ?? runs[0])?.run_id ?? null;
}

/** The goal's newest new-goal or Run again run, whose research and answers the card shows; null when none. `runs` is newest first. */
export function originRun(runs: RunSummary[]): string | null {
	return runs.find((r) => PLANNING.includes(r.workflow))?.run_id ?? null;
}

/**
 * Active work time of a run from its events: each step from its start to its finish, an
 * unfinished step until `now`. Waiting between steps never counts. Output in milliseconds.
 */
export function activeMs(events: RunEvent[], now: number): number {
	const open = new Map<string, number>();
	let total = 0;
	for (const e of events) {
		const step = e.body.step;
		if (!step) continue;
		if (e.body.type === 'step_started') open.set(step, e.at_ms);
		if (e.body.type === 'step_finished' && open.has(step)) {
			total += e.at_ms - (open.get(step) ?? e.at_ms);
			open.delete(step);
		}
	}
	for (const start of open.values()) total += Math.max(0, now - start);
	console.assert(total >= 0, 'active time is never negative');
	return total;
}

/** Milliseconds as minutes and seconds, "1:05". */
function clockText(ms: number): string {
	const s = Math.floor(ms / 1000);
	return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/**
 * The goal card status for the followed run `view` (null: none yet), with the run time limit
 * `limitMin` in minutes and `now` in epoch ms: Working with the step, time of the limit and
 * cost; Waiting for you with what it waits for; Stopped with Retry; Failed with Retry after a
 * transient error; Paused or Failed with the reason; On track when agents are idle. With
 * `unreachable` and no view the engine was not read: Offline with Retry, never On track.
 */
export function goalStatus(view: RunView | null, limitMin: number, now: number, unreachable = false): GoalStatus {
	const idle = { tone: 'done' as Tone, chip: 'On track', title: 'Agents are idle', stop: false, retry: false, limit: false };
	if (!view && unreachable) return { tone: 'missed', chip: 'Offline', title: 'Could not reach the engine', detail: 'Tap Retry to check again.', stop: false, retry: true, limit: false };
	if (!view) return { ...idle, detail: 'No run yet.' };
	const cost = formatCost(view.usage?.cost_usd_micros);
	const stopped = stopMessage(view.status);
	const limit = view.status.reason?.kind === 'time_limit';
	if (stopped) return { tone: 'skipped', chip: 'Stopped', title: stopped, detail: `Nothing more was spent: ${cost} in total.`, stop: false, retry: true, limit };
	const transient = retryMessage(view.status);
	if (transient) return { tone: 'missed', chip: 'Failed', title: transient, detail: `${cost} in total.`, stop: false, retry: true, limit: false };
	if (ACTIVE.includes(view.status.status)) {
		const step = progressLines(view.events).findLast((l) => !l.done)?.text ?? 'Starting';
		const detail = `${clockText(activeMs(view.events, now))} of ${clockText(limitMin * MINUTE_MS)} · ${cost} so far`;
		return { tone: 'planned', chip: 'Working', title: step, detail, stop: true, retry: false, limit: false };
	}
	const pause = pauseMessage(view.status);
	if (pause && view.status.status === 'waiting_budget') return { tone: 'warn', chip: 'Paused', title: pause, detail: `${cost} so far.`, stop: false, retry: false, limit: false };
	if (pause) return { tone: 'missed', chip: 'Failed', title: pause, detail: `${cost} in total.`, stop: false, retry: false, limit: false };
	const wait = waitLabel(view);
	if (wait) {
		const detail = view.waiting[0]?.node === 'approve' ? 'Nothing is on your calendar until you approve.' : 'The timer is paused while you answer.';
		return { tone: 'warn', chip: 'Waiting for you', title: wait, detail, stop: false, retry: false, limit: false };
	}
	return { ...idle, detail: `Last run ${cost}.` };
}

/**
 * Whether the settled run mark `mark` (`run_id:status`, see `settledMark`) is already in `runs`,
 * the goal's runs as read at open, so coaches need no second read for it (B87). Null runs: none read.
 */
export function markRead(mark: string, runs: { run_id?: string; state?: string }[] | null): boolean {
	return runs?.some((r) => `${r.run_id}:${r.state}` === mark) ?? false;
}

/** True while the followed run `view` waits for the owner to approve or decline a plan; Run again is blocked then. */
export function planWaits(view: RunView | null): boolean {
	return view?.status.status === 'waiting_human' && view.waiting.some((w) => w.node === 'approve' && !!w.data.plan);
}

/** The replan reason for answers the owner changed on the goal card: each question with its new answer. */
export function answersReason(changed: RunAnswer[]): string {
	console.assert(changed.length > 0, 'only changed answers replan');
	const lines = changed.map((a) => `${a.text}: ${a.answer ?? 'skipped'}.`);
	return `I changed my answers. ${lines.join(' ')} Update the plan to fit.`;
}

/**
 * The key the goal page refetches its coaches and plugins on: `run_id:status` once a run has
 * settled (waiting, done, failed or stopped), null while it is queued or running or when there
 * is no run. Keying on the snapshot instead refetched them on every sync during a run (B73).
 */
export function settledMark(view: RunView | null): string | null {
	if (!view || ACTIVE.includes(view.status.status)) return null;
	return `${view.run_id}:${view.status.status}`;
}
