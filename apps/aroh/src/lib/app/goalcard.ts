import type { Tone } from '$lib/ui/components/StatusChip.svelte';
import { ACTIVE, formatCost, pauseMessage, PLANNING, progressLines, retryMessage, stopMessage, waitLabel, type RunAnswer, type RunEvent, type RunSummary, type RunView } from './engine';
import { PACE_LABEL, type Pace } from './dashboard';

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

/** A goal's status chip: tone and label, the same on the Goals list, the goal page and the Inbox. */
export type Chip = { tone: Tone; label: string };

const RUN_CHIP: Record<string, Chip> = {
	queued: { tone: 'planned', label: 'Working' },
	running: { tone: 'planned', label: 'Working' },
	checkpointed: { tone: 'planned', label: 'Working' },
	waiting_human: { tone: 'warn', label: 'Waiting for you' },
	waiting_budget: { tone: 'warn', label: 'Paused' },
	failed: { tone: 'missed', label: 'Failed' },
	stopped: { tone: 'skipped', label: 'Stopped' }
};

const SETTLED_CHIP: Record<string, Chip> = {
	done: { tone: 'done', label: 'Done' },
	paused: { tone: 'skipped', label: 'Paused' },
	archived: { tone: 'skipped', label: 'Archived' }
};

const PACE_TONE: Record<Pace, Tone> = { on_track: 'done', ahead: 'done', behind: 'missed' };

/**
 * The chip state of a run in lifecycle `state` stopped from outside a step as `stop` (null:
 * not stopped): `stopped` for a failed run its owner cancelled, else `state`. Errors and
 * time-limit stops stay `failed`.
 */
export function runState(state: string, stop: string | null | undefined): string {
	return state === 'failed' && stop === 'cancelled' ? 'stopped' : state;
}

/** The chip of a run in `state` alone (an Inbox run row); null for a finished run or an unknown state. */
export function runChip(state: string): Chip | null {
	return RUN_CHIP[state] ?? null;
}

/**
 * A goal's chip from its row `goal`, the `runState` of the run `pickRun` picks (null: no run),
 * whether a proposal of it is `proposed`, and its dashboard `pace`. Done, paused or archived goals
 * first; then the run: Working, Waiting for you (or a proposal), Paused at a budget, Failed or
 * Stopped by the owner; else Planning for a draft, else the pace, else Active.
 */
export function chipFor(goal: { status: string }, state: string | null, proposed: boolean, pace: Pace | null = null): Chip {
	const settled = SETTLED_CHIP[goal.status];
	if (settled) return settled;
	const run = state === null ? null : runChip(state);
	if (run && ACTIVE.includes(state ?? '')) return run;
	if (proposed) return RUN_CHIP.waiting_human;
	if (run) return run;
	if (goal.status === 'draft') return { tone: 'planned', label: 'Planning' };
	return pace ? { tone: PACE_TONE[pace], label: PACE_LABEL[pace] } : { tone: 'planned', label: 'Active' };
}

/** `chipFor` over the goal's `runs` (newest first), following the run `pickRun` picks. */
export function goalChip(goal: { status: string }, runs: RunSummary[], proposed: boolean, pace: Pace | null = null): Chip {
	const id = pickRun(runs);
	const run = runs.find((r) => r.run_id === id);
	return chipFor(goal, run ? runState(run.state, run.stop) : null, proposed, pace);
}

/**
 * The newest runs list joined with the waiting-run list, which reaches past the 50 newest, each
 * run once (the `newest` copy wins), newest first by creation as `GET /v1/runs` orders them.
 */
export function mergeRuns(newest: RunSummary[], waiting: RunSummary[]): RunSummary[] {
	const seen = new Set(newest.map((r) => r.run_id));
	const all = [...newest, ...waiting.filter((r) => !seen.has(r.run_id))];
	return all.sort((a, b) => b.created_at.localeCompare(a.created_at) || b.run_id.localeCompare(a.run_id));
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
 * `limitMin` in minutes and `now` in epoch ms. Its tone and chip are the shared `chip` (`chipFor`
 * over the same run). Lines: the step, time of the limit and cost while working; what it waits
 * for; the stop or failure reason with Retry when it applies; agents idle otherwise. With
 * `unreachable` and no view the engine was not read: Offline with Retry.
 */
export function goalStatus(view: RunView | null, limitMin: number, now: number, chip: Chip, unreachable = false): GoalStatus {
	if (!view && unreachable) return { tone: 'missed', chip: 'Offline', title: 'Could not reach the engine', detail: 'Tap Retry to check again.', stop: false, retry: true, limit: false };
	const shown = { tone: chip.tone, chip: chip.label };
	const idle = { ...shown, title: 'Agents are idle', stop: false, retry: false, limit: false };
	if (!view) return { ...idle, detail: 'No run yet.' };
	const cost = formatCost(view.usage?.cost_usd_micros);
	const stopped = stopMessage(view.status);
	const limit = view.status.reason?.kind === 'time_limit';
	if (stopped) return { ...shown, title: stopped, detail: `Nothing more was spent: ${cost} in total.`, stop: false, retry: true, limit };
	const transient = retryMessage(view.status);
	if (transient) return { ...shown, title: transient, detail: `${cost} in total.`, stop: false, retry: true, limit: false };
	if (ACTIVE.includes(view.status.status)) {
		const step = progressLines(view.events).findLast((l) => !l.done)?.text ?? 'Starting';
		const detail = `${clockText(activeMs(view.events, now))} of ${clockText(limitMin * MINUTE_MS)} · ${cost} so far`;
		return { ...shown, title: step, detail, stop: true, retry: false, limit: false };
	}
	const pause = pauseMessage(view.status);
	if (pause && view.status.status === 'waiting_budget') return { ...shown, title: pause, detail: `${cost} so far.`, stop: false, retry: false, limit: false };
	if (pause) return { ...shown, title: pause, detail: `${cost} in total.`, stop: false, retry: false, limit: false };
	const wait = waitLabel(view);
	if (wait) {
		const detail = view.waiting[0]?.node === 'approve' ? 'Nothing is on your calendar until you approve.' : 'The timer is paused while you answer.';
		return { ...shown, title: wait, detail, stop: false, retry: false, limit: false };
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
