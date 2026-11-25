<script lang="ts">
	import { onMount } from 'svelte';
	import InboxScreen from '$lib/ui/screens/InboxScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { follow } from '$lib/app/chat.svelte';
	import { ACTIVE, engine, unreachable, type Insight, type RunSummary, type Waiting } from '$lib/app/engine';
	import { refreshRuns } from '$lib/app/realtime';
	import { decisionsFrom, deviceSuggestions, finishTriage, refreshSuggestions, triageCards } from '$lib/app/calendar';
	import { inboxView } from '$lib/app/views';
	import { openProposals, withOpened, type OpenedProposal } from '$lib/app/diary';
	import * as location from '$lib/app/location';
	import { getClock } from '$lib/ui/clock';

	const clock = getClock();
	const s = $derived(data.snapshot);
	let runs = $state<RunSummary[]>(engine.cachedRuns() ?? []);
	let insights = $state<Insight[]>(engine.cachedInsights() ?? []);
	let waits = $state<Record<string, string>>(engine.cachedWaits(engine.cachedRuns() ?? []));
	let triage = $state<{ runId: string; waiting: Waiting } | null>(null);
	let answers: Record<string, unknown> = {};
	let places = $state<location.LocationView | null>(null);
	let offline = $state(false);
	let opened = $state<Record<string, OpenedProposal>>({});

	$effect(() => {
		const list = s?.proposals ?? [];
		void openProposals(list).then((o) => (opened = o));
	});

	/** Reads the engine's newest runs, what the waiting ones wait for, and insights; a failed read leaves them empty and sets `offline`. */
	async function loadRuns() {
		const [r, i] = await Promise.all([engine.runs(), engine.insights()]);
		offline = unreachable(r);
		runs = r.ok ? r.value : [];
		insights = i.ok ? i.value : [];
		waits = await engine.waits(runs);
		await loadTriage();
	}

	/**
	 * Re-reads the runs and what the waiting ones wait for, after a run event or a return to the
	 * app. Triage answers and insights stay as they are; a failed read keeps the last list.
	 */
	async function refresh() {
		const r = await engine.runs();
		offline = unreachable(r);
		if (!r.ok) return;
		const w = await engine.waits(r.value);
		runs = r.value;
		waits = w;
	}

	/** Reads the calendar-triage run waiting at `decide` for the group cards and keeps its day suggestions. */
	async function loadTriage() {
		triage = data.snapshot ? await refreshSuggestions(data.snapshot, deviceSuggestions(runs)) : null;
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

	onMount(() => {
		void loadRuns();
		return refreshRuns(
			() => void refresh(),
			() => runs.some((r) => ACTIVE.includes(r.state))
		);
	});
	onMount(() => location.follow((v) => (places = v), (e) => (data.error = String(e))));

	/** Answers a place prompt; an answer the card cannot give is ignored. */
	function onplace(id: string, answer: unknown) {
		const decision = location.decisionFor(answer);
		if (decision) void data.run(() => location.decidePrompt(id, decision));
	}

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

	/** The dismissed insight ids still shown (engine insights, device notes, Busy-day and fixed-clash notes), plus `id`. */
	function dismissedWith(id: string): string {
		const ids = [...insights, ...(s?.notes ?? [])].map((i) => i.id);
		const shown = new Set([...ids, ...(s?.busy ?? []).map((b) => b.id), ...(s?.actions ?? []).filter((a) => a.locked).map((a) => a.id)]);
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
		if (id.startsWith(location.PATTERN_PREFIX)) return data.run(() => location.decidePattern(id.slice(location.PATTERN_PREFIX.length), yes));
		const goalId = s?.proposals.find((p) => p.id === id)?.goal_id;
		const runId = goalId ? await waitingRun(goalId, id) : null;
		if (!runId) return data.run(() => client.decideProposal(id, yes));
		const answered = await engine.answer(runId, 'approve', yes);
		if (!answered.ok) return void (data.error = answered.message);
		await follow(runId);
		await loadRuns();
	}
</script>

{#if offline}
	<div class="flex items-center gap-3 px-4 pt-3" role="alert">
		<p class="min-w-0 flex-1 text-sm text-status-missed">Could not reach the engine, so runs waiting for you may be missing.</p>
		<button type="button" class="min-h-11 rounded-sm border border-border bg-bg px-3 text-sm text-fg outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => void loadRuns()}>Retry</button>
	</div>
{/if}
{#if s}
	{@const inbox = inboxView({ ...s, proposals: withOpened(s.proposals, opened) }, runs, insights, clock.zone, clock.now, waits)}
	<InboxScreen
		{...inbox}
		approvals={[...(places ? location.patternItems(places) : []), ...inbox.approvals]}
		places={places ? location.placePrompts(places, s) : []}
		triage={triageCards(triage?.waiting)}
		{ondecide}
		{onplace}
	/>
{/if}
