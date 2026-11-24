import { invoke } from '@tauri-apps/api/core';
import { deviceZone } from '$lib/ui/clock';
import { data } from './data.svelte';
import * as client from './client';
import { engine } from './engine';
import { askCatchUp, offerOf, type CatchUp, type CatchUpOffer } from './catchup';

/**
 * The catch-up question after a late done (Q78), shared by Today, Calendar and the action page.
 * `setStatus` writes the status, then, for a late done, asks the server whether a miss replan
 * added a catch-up and offers to remove it. Nothing changes until the owner taps Remove.
 */
class CatchUpStatus {
	offer = $state<CatchUpOffer | null>(null);
	note = $state<string | null>(null);

	/** Writes `next` on `action` and, for a late done after a miss, prepares the offer. */
	async setStatus(action: Pick<client.Action, 'id' | 'status' | 'end_at'>, next: 'planned' | 'done' | 'skipped') {
		await data.run(() => client.setActionStatus(action.id, next));
		if (data.error || !askCatchUp(action.status, next, action.end_at, new Date().toISOString())) return;
		const r = await engine.missReplan(action.id);
		if (!r.ok || !r.value.run_id) return;
		const change = await invoke<CatchUp>('app_catch_up', { runId: r.value.run_id }).catch(() => null);
		const offer = change && offerOf(r.value.run_id, change, deviceZone());
		if (offer) {
			this.note = null;
			this.offer = offer;
		}
	}

	/** The one tap: turns back the miss replan, which removes the catch-up. */
	async remove() {
		const offer = this.offer;
		if (!offer) return;
		this.offer = null;
		await data.run(() => invoke<number>('app_undo_run', { runId: offer.runId }));
		this.note = data.error ? `Could not remove the catch-up: ${data.error}` : 'Catch-up removed.';
	}

	/** Keeps the catch-up and closes the question. */
	keep() {
		this.offer = null;
		this.note = null;
	}
}

/** The one catch-up question the screens show. */
export const catchUp = new CatchUpStatus();
