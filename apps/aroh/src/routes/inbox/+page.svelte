<script lang="ts">
	import { onMount } from 'svelte';
	import InboxScreen from '$lib/ui/screens/InboxScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { follow } from '$lib/app/chat.svelte';
	import { engine, type Insight, type RunSummary, type Waiting } from '$lib/app/engine';
	import { decisionsFrom, finishTriage, openTriage, triageCards } from '$lib/app/calendar';
	import { inboxView } from '$lib/app/views';

	const s = $derived(data.snapshot);
	let runs = $state<RunSummary[]>([]);
	let insights = $state<Insight[]>([]);
	let triage = $state<{ runId: string; waiting: Waiting } | null>(null);
	let answers: Record<string, unknown> = {};

	/** Reads the engine's newest runs and insights; offline leaves both lists empty. */
	async function loadRuns() {
		const [r, i] = await Promise.all([engine.runs(), engine.insights()]);
		runs = r.ok ? r.value : [];
		insights = i.ok ? i.value : [];
		await loadTriage();
	}

	/** Reads the calendar-triage run waiting at `decide`, if any, for the Calendar groups cards. */
	async function loadTriage() {
		triage = await openTriage(runs);
		answers = {};
	}

	/** Holds one group's answer; once every group is answered, posts them together and finishes the run. */
	async function decideGroup(name: string, decision: unknown) {
		if (!triage) return;
		answers[name] = decision;
		const decisions = decisionsFrom(triage.waiting, answers);
		if (!decisions) return;
		const { runId } = triage;
		const answered = await engine.answer(runId, 'decide', decisions);
		if (!answered.ok) return void (data.error = answered.message);
		await finishTriage(runId);
		await loadRuns();
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

	/** The dismissed insight ids still shown, plus `id`: the list stays as short as the Inbox. */
	function dismissedWith(id: string): string {
		const shown = new Set([...insights, ...(s?.notes ?? [])].map((i) => i.id));
		const kept = (s?.settings.dismissed_insights ?? '').split(',').filter((x) => shown.has(x) && x !== id);
		return [...kept, id].slice(-30).join(',');
	}

	/**
	 * Routes a card decision: a calendar group answer, a suggestion, an insight dismissal, or a
	 * proposal applied or rejected (through its waiting run when it has one).
	 */
	async function ondecide(group: 'triage' | 'approval' | 'suggestion' | 'insight', id: string, decision: unknown) {
		if (group === 'triage') return decideGroup(id, decision);
		const yes = decision === 'apply';
		if (group === 'suggestion') return data.run(() => client.decidePreference(id, yes));
		if (group === 'insight') return data.run(() => client.setSetting('dismissed_insights', dismissedWith(id)));
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
	<InboxScreen {...inboxView(s, runs, insights)} triage={triageCards(triage?.waiting)} {ondecide} />
{/if}
