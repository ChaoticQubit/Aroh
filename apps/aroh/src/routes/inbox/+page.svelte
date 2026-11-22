<script lang="ts">
	import { onMount } from 'svelte';
	import InboxScreen from '$lib/ui/screens/InboxScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { follow } from '$lib/app/chat.svelte';
	import { engine, type RunSummary } from '$lib/app/engine';
	import { inboxView } from '$lib/app/views';

	const s = $derived(data.snapshot);
	let runs = $state<RunSummary[]>([]);

	/** Reads the engine's newest runs; offline leaves the list empty. */
	async function loadRuns() {
		const r = await engine.runs();
		runs = r.ok ? r.value : [];
	}

	onMount(() => void loadRuns());

	/**
	 * The replan run of `goalId` waiting at `approve` for proposal `id`, if any. Such a proposal is
	 * decided through the run, so the engine applies it once and the run does not stay waiting.
	 */
	async function waitingRun(goalId: string, id: string): Promise<string | null> {
		const list = await engine.runs({ goalId, state: 'waiting_human' });
		for (const r of list.ok ? list.value : []) {
			const v = await engine.run(r.run_id);
			const w = v.ok ? v.value.waiting.find((x) => x.node === 'approve') : undefined;
			if ((w?.data.proposal_record as { proposal_id?: string } | undefined)?.proposal_id === id) return r.run_id;
		}
		return null;
	}

	/** Applies or rejects a proposal (through its waiting run when it has one), or decides a suggestion. */
	async function ondecide(group: 'approval' | 'suggestion' | 'insight', id: string, decision: unknown) {
		const yes = decision === 'apply';
		if (group === 'suggestion') return data.run(() => client.decidePreference(id, yes));
		if (group !== 'approval') return;
		const goalId = s?.proposals.find((p) => p.id === id)?.goal_id;
		const runId = goalId ? await waitingRun(goalId, id) : null;
		if (!runId) return data.run(() => client.decideProposal(id, yes));
		const answered = await engine.answer(runId, 'approve', yes);
		if (!answered.ok) return void (data.error = answered.message);
		await follow(runId);
		await loadRuns();
	}
</script>

{#if s}
	<InboxScreen {...inboxView(s, runs)} {ondecide} />
{/if}
