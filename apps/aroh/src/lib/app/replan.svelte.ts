import { invoke } from '@tauri-apps/api/core';
import { deviceZone } from '$lib/ui/clock';
import * as client from './client';
import { ACTIVE, engine } from './engine';
import { phaseOf, type Observed, type Phase } from './replan';

const POLL_MS = 2_500;
const SLOW_POLL_MS = 10_000;
const GIVE_UP_MS = 15 * 60_000;
const SETTLED: Phase['kind'][] = ['clear', 'needs_you', 'failed', 'updated', 'undone'];

/**
 * The status of the owner's latest action edit (Q77), shared by Today and Calendar. `track` starts
 * after a saved edit; it asks the server whether the edit started a replan, follows that run and,
 * once done, reads what moved. `phase` is never left silent past a minute.
 */
class EditStatus {
	phase = $state<Phase | null>(null);
	private seen: Observed | null = null;
	private actionId = '';
	private token = 0;
	private ticker: ReturnType<typeof setInterval> | undefined;

	/** Starts following the replan after the owner saved an edit of `actionId`; a newer edit replaces an older one. */
	track(actionId: string) {
		this.actionId = actionId;
		this.seen = { savedAt: Date.now(), now: Date.now(), server: null, run: null, runSeenAt: null, proposals: null, zone: deviceZone() };
		this.show();
		void this.loop(++this.token);
	}

	/** Hides the status line and stops following. */
	dismiss() {
		this.token++;
		this.phase = null;
		clearInterval(this.ticker);
		this.ticker = undefined;
	}

	/** Retry: continues a stopped run, or pushes the edit again and asks the server anew. */
	async retry() {
		const p = this.phase;
		if (!this.seen || !p) return;
		if (p.kind === 'failed' && p.runId) {
			const r = await engine.retry(p.runId);
			if (!r.ok) return void (this.phase = { kind: 'failed', text: `Could not replan. ${r.message}`, tone: 'missed', runId: p.runId });
			this.seen = { ...this.seen, run: null, runSeenAt: Date.now() };
		} else {
			await client.sync().catch(() => null);
			this.seen = { ...this.seen, savedAt: Date.now(), server: null, run: null, runSeenAt: null };
		}
		this.show();
		void this.loop(++this.token);
	}

	/** Undo: turns back what the finished replan applied, on device, then syncs. */
	async undo() {
		const runId = this.phase?.runId;
		if (!runId || !this.seen) return;
		try {
			await invoke<number>('app_undo_run', { runId });
			this.phase = { kind: 'undone', text: 'Undone. The replan was turned back.', tone: 'skipped', runId };
		} catch (e) {
			this.phase = { kind: 'failed', text: `Could not undo: ${String(e)}`, tone: 'missed' };
		}
	}

	/** Recomputes `phase` from what was seen, at the current time, and keeps a 1 s ticker for the elapsed time while it moves. */
	private show() {
		if (!this.seen) return;
		this.seen.now = Date.now();
		this.phase = phaseOf(this.seen);
		const moving = !SETTLED.includes(this.phase.kind);
		if (moving && !this.ticker) this.ticker = setInterval(() => this.show(), 1_000);
		if (!moving) {
			clearInterval(this.ticker);
			this.ticker = undefined;
		}
	}

	/** One poll: server check until it answers, then the run, then what moved once it is done. */
	private async poll() {
		const seen = this.seen!;
		const state = seen.server?.ok ? seen.server.value : null;
		if (!state?.checked) {
			seen.server = await engine.editReplan(this.actionId);
			if (!seen.server.ok || !seen.server.value.run_id) return;
		}
		const runId = (seen.server as { ok: true; value: { run_id: string } }).value.run_id;
		const r = await engine.run(runId);
		if (!r.ok) return void (seen.server = r);
		seen.runSeenAt ??= Date.now();
		seen.run = r.value;
		if (!ACTIVE.includes(r.value.status.status) && r.value.status.status === 'done' && !seen.proposals) {
			await client.sync().catch(() => null);
			seen.proposals = await invoke<client.Proposal[]>('app_run_proposals', { runId });
		}
	}

	/** Polls until the edit settles, a newer edit replaces it, or 15 minutes pass. */
	private async loop(token: number) {
		while (token === this.token && this.seen && Date.now() - this.seen.savedAt < GIVE_UP_MS) {
			try {
				await this.poll();
			} catch (e) {
				this.seen.server = { ok: false, reason: 'error', message: String(e) };
			}
			if (token !== this.token) return;
			this.show();
			if (this.phase && SETTLED.includes(this.phase.kind)) return;
			await new Promise((r) => setTimeout(r, this.phase?.kind === 'not_started' ? SLOW_POLL_MS : POLL_MS));
		}
	}
}

/** The one edit status Today and Calendar show. */
export const editStatus = new EditStatus();
