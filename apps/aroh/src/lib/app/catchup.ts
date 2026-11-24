import { formatDay, formatTime } from '$lib/ui/format';
import type { Action } from './client';

/** What turning back a replan run would change, as `app_catch_up` answers. */
export type CatchUp = { added: Pick<Action, 'title' | 'start_at'>[]; moved: number };

/** The one-tap offer to remove the catch-up a miss replan added (Q78). */
export type CatchUpOffer = { runId: string; text: string; detail: string };

/**
 * Whether marking an action `next` (from `before`) may have a catch-up to offer: only when it is
 * newly done and already ended at `now`, so the server is asked only for late logs.
 */
export function askCatchUp(before: Action['status'], next: Action['status'], end: string, now: string): boolean {
	return next === 'done' && before !== 'done' && Date.parse(end) < Date.parse(now);
}

/**
 * The offer for run `runId` from what it changed, with each added action named in `zone`; null
 * when the run added nothing, so no question is asked.
 */
export function offerOf(runId: string, change: CatchUp, zone: string): CatchUpOffer | null {
	if (!change.added.length) return null;
	const names = change.added.map((a) => `${a.title}, ${formatDay(a.start_at, zone)} ${formatTime(a.start_at, zone)}`).join(' and ');
	const back = change.moved ? `, and puts back ${change.moved} moved ${change.moved === 1 ? 'action' : 'actions'}` : '';
	return { runId, text: 'A catch-up was added because this was missed. Remove it?', detail: `Removes ${names}${back}.` };
}
