import type { Proposal } from './client';
import { pauseMessage, progressLines, stopMessage, type EngineResult, type RunView } from './engine';

/** How long an edit may wait for the server's check, or a run for the engine, before the app says it has not started. */
export const NOT_STARTED_MS = 60_000;

/** `GET /v1/actions/{id}/replan`: the newest edit event, whether the server checked it, its run, and the run it waits behind. */
export type EditReplan = { event_at: string | null; checked: boolean; run_id: string | null; waiting_on: string | null };

/** Everything the app has seen of one edit so far; `now`, `savedAt` and `runSeenAt` are epoch milliseconds. */
export type Observed = {
	savedAt: number;
	now: number;
	server: EngineResult<EditReplan> | null;
	run: RunView | null;
	runSeenAt: number | null;
	proposals: Proposal[] | null;
	zone: string;
};

/** Where one owner edit stands (Q77); `tone` picks the status dot, `runId` is set when Retry or Undo act on a run. */
export type Phase = {
	kind: 'saved' | 'clear' | 'replanning' | 'not_started' | 'needs_you' | 'failed' | 'updated' | 'undone';
	text: string;
	tone: 'planned' | 'warn' | 'missed' | 'done' | 'skipped';
	runId?: string;
	undo?: boolean;
};

/** Short local weekday and time of `iso` in `zone`, "Tue 15:00". */
function when(iso: string, zone: string): string {
	const d = new Date(iso);
	const day = new Intl.DateTimeFormat('en-US', { weekday: 'short', timeZone: zone }).format(d);
	const time = new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit', hour12: false, timeZone: zone }).format(d);
	return `${day} ${time}`;
}

/** Minutes and seconds of `ms`, "1:15". */
function clock(ms: number): string {
	const s = Math.max(0, Math.floor(ms / 1000));
	return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/** Owner words for each change of a proposal diff: "moved Read to Tue 15:00", "removed Walk", "added Plank on Wed 07:00". Malformed changes are left out. */
export function movedLines(diff: unknown, zone: string): string[] {
	const changes = (diff as { changes?: unknown } | null)?.changes;
	if (!Array.isArray(changes)) return [];
	const out: string[] = [];
	for (const c of changes) {
		if (!c || typeof c !== 'object') continue;
		const { op, title, after } = c as { op?: string; title?: string; after?: { start_at?: string } | null };
		if (typeof title !== 'string' || !title) continue;
		const at = after?.start_at;
		if ((op === 'move' || op === 'resize') && at) out.push(`moved ${title} to ${when(at, zone)}`);
		else if (op === 'delete') out.push(`removed ${title}`);
		else if (op === 'create' && at) out.push(`added ${title} on ${when(at, zone)}`);
	}
	return out;
}

/** Why the server cannot be asked, in owner words. */
function unreachable(server: Extract<EngineResult<EditReplan>, { ok: false }>): string {
	if (server.reason === 'needs_engine') return 'Replanning has not started: Sign in and set the server URL in Settings so Aroh can replan.';
	return `Replanning has not started: Aroh could not reach the server (${server.message}). Check the connection and retry.`;
}

/** The phase of a run the server started for the edit, once its view is read. */
function runPhase(o: Observed, run: RunView, runId: string): Phase {
	const status = run.status.status;
	if (status === 'waiting_human') return { kind: 'needs_you', text: 'Replanning needs your OK in Inbox.', tone: 'warn', runId };
	if (status === 'failed' || status === 'waiting_budget') {
		const reason = stopMessage(run.status) ?? pauseMessage(run.status) ?? 'The run stopped.';
		return { kind: 'failed', text: `Could not replan. ${reason}`, tone: 'missed', runId };
	}
	if (status === 'done') {
		if (!o.proposals) return { kind: 'replanning', text: 'Replanning: Reading what changed', tone: 'planned', runId };
		if (o.proposals.some((p) => p.status === 'undone')) return { kind: 'undone', text: 'Undone. The replan was turned back.', tone: 'skipped', runId };
		const applied = o.proposals.filter((p) => p.status === 'applied');
		const lines = applied.flatMap((p) => movedLines(p.diff, o.zone));
		if (applied.length && lines.length) return { kind: 'updated', text: `Updated: ${lines.join(', ')}.`, tone: 'done', runId, undo: true };
		return { kind: 'updated', text: 'Updated: nothing else needed to move.', tone: 'done', runId, undo: false };
	}
	const started = run.events.some((e) => e.body.type === 'step_started');
	if (status === 'queued' && !started && o.now - (o.runSeenAt ?? o.now) >= NOT_STARTED_MS) {
		return { kind: 'not_started', text: 'Replanning has not started: the replan is queued but the engine has not picked it up in a minute. Its scheduler may be off.', tone: 'warn', runId };
	}
	const step = progressLines(run.events).filter((l) => !l.done).at(-1)?.text ?? 'Starting';
	return { kind: 'replanning', text: `Replanning: ${step} · ${clock(o.now - o.savedAt)}`, tone: 'planned', runId };
}

/** Where an owner edit stands from what the app observed: Saved, Replanning (step and elapsed), Updated, Could not replan, or why it has not started. Never silent after `NOT_STARTED_MS`. */
export function phaseOf(o: Observed): Phase {
	const waited = o.now - o.savedAt;
	if (o.server && !o.server.ok) return { kind: 'not_started', text: unreachable(o.server), tone: 'warn' };
	const state = o.server?.ok ? o.server.value : null;
	if (!state || !state.checked) {
		if (state?.waiting_on) return { kind: 'replanning', text: `Replanning: waiting for an earlier replan of this goal · ${clock(waited)}`, tone: 'planned', runId: state.waiting_on };
		if (waited < NOT_STARTED_MS) return { kind: 'saved', text: 'Saved. Checking for clashes.', tone: 'planned' };
		if (!state?.event_at) return { kind: 'not_started', text: 'Replanning has not started: this change has not reached the server yet. Aroh sends it when you are online.', tone: 'warn' };
		return { kind: 'not_started', text: 'Replanning has not started: the server has the change but has not checked it in a minute. Its scheduler may be off.', tone: 'warn' };
	}
	if (!state.run_id) return { kind: 'clear', text: 'Saved. Nothing else needed to move.', tone: 'done' };
	if (!o.run) return { kind: 'replanning', text: `Replanning: Starting · ${clock(waited)}`, tone: 'planned', runId: state.run_id };
	return runPhase(o, o.run, state.run_id);
}
