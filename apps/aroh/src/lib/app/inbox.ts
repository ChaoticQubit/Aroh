import type { DeleteGroup } from '$lib/ui/screens/types';
import { ACTIVE, type EngineResult } from './engine';

/**
 * What deleting Inbox entries needs from the app: `decide` routes a card decision as the Inbox
 * does (a proposal through the shared `decideProposal`), `ignoreGroup` answers a calendar group
 * Ignore, `ignorePlace` ignores a place prompt, `dismissInsights` and `storeRuns` store dismissed
 * ids, `cancelRun` stops a run on the engine, `runState` is a listed run's state.
 */
export type InboxDeps = {
	decide: (group: 'approval' | 'suggestion', id: string, decision: 'reject') => Promise<unknown>;
	ignoreGroup: (name: string) => Promise<unknown>;
	ignorePlace: (id: string) => Promise<unknown>;
	dismissInsights: (ids: string[]) => Promise<unknown>;
	storeRuns: (ids: string[]) => Promise<unknown>;
	cancelRun: (id: string) => Promise<EngineResult<unknown>>;
	runState: (id: string) => string | undefined;
};

/**
 * Deletes runs `ids` (B115): one still waiting, paused or working is cancelled first (the screen
 * asked), then all that are gone are stored as dismissed in one write. Resolves to the refusal
 * messages of runs that would not stop, which stay listed.
 */
export async function deleteRuns(ids: string[], deps: InboxDeps): Promise<string[]> {
	const gone: string[] = [];
	const refused: string[] = [];
	for (const id of ids) {
		const state = deps.runState(id);
		if (state !== undefined && runDelete(state) === 'cancel') {
			const stopped = await deps.cancelRun(id);
			if (!stopped.ok) {
				refused.push(stopped.message);
				continue;
			}
		}
		gone.push(id);
	}
	if (gone.length) await deps.storeRuns(gone);
	return refused;
}

/** Deletes one Inbox entry (B115): a proposal or suggestion is dismissed, a calendar group ignored, a place prompt ignored, an insight or run stored as dismissed. */
export async function deleteEntry(group: DeleteGroup, id: string, deps: InboxDeps): Promise<string[]> {
	if (group === 'approval' || group === 'suggestion') await deps.decide(group, id, 'reject');
	else if (group === 'insight') await deps.dismissInsights([id]);
	else if (group === 'place') await deps.ignorePlace(id);
	else if (group === 'triage') await deps.ignoreGroup(id);
	else return deleteRuns([id], deps);
	return [];
}

/** What Clear all deletes: the listed entries by kind, and every calendar group of the waiting triage not answered yet. */
export type Shown = { places: string[]; approvals: string[]; suggestions: string[]; insights: string[]; groups: string[]; runs: string[] };

/**
 * Clears the Inbox (B115): each entry deleted as `deleteEntry` would, insights and runs in one
 * write each. Resolves to the refusal messages of runs that would not stop.
 */
export async function clearAll(shown: Shown, deps: InboxDeps): Promise<string[]> {
	for (const id of shown.places) await deps.ignorePlace(id);
	for (const id of shown.approvals) await deps.decide('approval', id, 'reject');
	for (const id of shown.suggestions) await deps.decide('suggestion', id, 'reject');
	if (shown.insights.length) await deps.dismissInsights(shown.insights);
	for (const name of shown.groups) await deps.ignoreGroup(name);
	return shown.runs.length ? deleteRuns(shown.runs, deps) : [];
}

/** Most ids a dismissed list (`dismissed_insights`, `dismissed_runs`) stores, as `aroh_app::local::MAX_DISMISSED`. */
export const MAX_DISMISSED = 300;

/** The ids of a dismissed list setting value. */
export function dismissedSet(value: string | undefined): Set<string> {
	return new Set((value ?? '').split(',').filter(Boolean));
}

/**
 * The dismissed list `current` with `add` appended once each (B115). Ids whose item is no longer
 * in `live` are dropped, so the list stays near the items still listed and a 31st dismissal never
 * pushes out the first; with `live` null (the items could not be read) every id stays. Past
 * `MAX_DISMISSED` the oldest go first.
 */
export function dismissedList(current: string | undefined, add: string[], live: Set<string> | null): string {
	const kept = [...dismissedSet(current)].filter((id) => !live || live.has(id));
	const out = [...new Set([...kept, ...add])].slice(-MAX_DISMISSED);
	console.assert(out.length <= MAX_DISMISSED && add.every((id) => out.includes(id)), 'a dismissal is always stored');
	return out.join(',');
}

/**
 * What deleting a run in `state` from the Inbox does (B115): a run that waits for the owner, is
 * paused or still works is cancelled on the engine after a confirm, so it never vanishes silently;
 * a run that already stopped is only hidden.
 */
export function runDelete(state: string): 'cancel' | 'hide' {
	return state === 'waiting_human' || state === 'waiting_budget' || ACTIVE.includes(state) ? 'cancel' : 'hide';
}
