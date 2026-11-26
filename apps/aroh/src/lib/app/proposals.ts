import type { ChatMessage } from '$lib/ui/contract';
import * as client from './client';
import { engine, type EngineResult } from './engine';

/**
 * The replan run of `goalId` waiting at `approve` for proposal `id`, if any. Such a proposal is
 * decided through the run, so the engine applies it once and the run does not stay waiting.
 */
export async function waitingRun(goalId: string, id: string): Promise<string | null> {
	const list = await engine.runs({ goalId, state: 'waiting_human' });
	for (const r of list.ok ? list.value : []) {
		const v = await engine.run(r.run_id);
		const w = v.ok ? v.value.waiting.find((x) => x.node === 'approve') : undefined;
		if ((w?.data.proposal_record as { proposal_id?: string } | undefined)?.proposal_id === id) return r.run_id;
	}
	return null;
}

/**
 * Applies (`yes`) or dismisses proposal `id` of goal `goalId`: the one action the Inbox and the
 * goal chat share (B114). A proposal a replan run waits on is answered through that run (`runId`
 * when the caller knows it, else looked up), any other is decided on this device. Resolves to the
 * run answered, which the caller follows, or null.
 */
export async function decideProposal(id: string, goalId: string | null, yes: boolean, runId: string | null = null): Promise<EngineResult<string | null>> {
	console.assert(id.length > 0, 'a proposal decision names its proposal');
	const run = runId ?? (goalId ? await waitingRun(goalId, id) : null);
	if (!run) {
		try {
			await client.decideProposal(id, yes);
			return { ok: true, value: null };
		} catch (e) {
			return { ok: false, reason: 'error', message: String(e) };
		}
	}
	const answered = await engine.answer(run, 'approve', yes);
	return answered.ok ? { ok: true, value: run } : answered;
}

/**
 * The thread without the undecided proposal cards whose proposal was open in the snapshot and has
 * left it (decided in the Inbox or on another device), so it leaves the goal chat too. `seen`
 * collects every id seen open in `proposals` across calls, so a card whose row has not synced yet
 * still shows. A card decided here stays to say what happened.
 */
export function withOpenProposals(messages: ChatMessage[], proposals: { id: string }[] | undefined, seen: Set<string>): ChatMessage[] {
	const open = new Set((proposals ?? []).map((p) => p.id));
	for (const id of open) seen.add(id);
	return messages.filter((m) => m.type !== 'proposal' || m.decided !== undefined || open.has(m.id) || !seen.has(m.id));
}
