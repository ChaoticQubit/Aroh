<script lang="ts">
	import { onMount } from 'svelte';
	import InboxScreen from '$lib/ui/screens/InboxScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { follow } from '$lib/app/chat.svelte';
	import { ACTIVE, engine, unreachable, type Insight, type RunSummary, type Waiting } from '$lib/app/engine';
	import { refreshRuns } from '$lib/app/realtime';
	import { decisionsFrom, deviceSuggestions, finishTriage, IGNORE, refreshSuggestions, triageCards } from '$lib/app/calendar';
	import { clearAll, deleteEntry, dismissedList, type InboxDeps } from '$lib/app/inbox';
	import type { DeleteGroup } from '$lib/ui/screens/types';
	import { inboxView } from '$lib/app/views';
	import { openProposals, withOpened, type OpenedProposal } from '$lib/app/diary';
	import { decideProposal } from '$lib/app/proposals';
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
	let insightsRead = $state(false);
	let hiddenTriage = $state<string[]>([]);

	const inbox = $derived.by(() => {
		if (!s) return null;
		const view = inboxView({ ...s, proposals: withOpened(s.proposals, opened) }, runs, insights, clock.zone, clock.now, waits);
		return {
			...view,
			approvals: [...(places ? location.patternItems(places) : []), ...view.approvals],
			places: places ? location.placePrompts(places, s) : [],
			triage: triageCards(triage?.waiting).filter((t) => !hiddenTriage.includes(t.id))
		};
	});

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
		insightsRead = i.ok;
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

	/**
	 * Reads the calendar-triage run waiting at `decide` for the group cards and keeps its day
	 * suggestions. Answers and deleted cards held for that run stay until the run changes.
	 */
	async function loadTriage() {
		const before = triage?.runId ?? null;
		triage = data.snapshot ? await refreshSuggestions(data.snapshot, deviceSuggestions(runs)) : null;
		if ((triage?.runId ?? null) === before) return;
		answers = {};
		hiddenTriage = [];
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
	 * The ids an insight card can have now (engine insights, device notes, Busy-day and fixed-clash
	 * notes), or null while the engine insights are unread, so no dismissal is dropped as gone.
	 */
	function liveInsights(): Set<string> | null {
		if (!insightsRead) return null;
		const ids = [...insights, ...(s?.notes ?? [])].map((i) => i.id);
		return new Set([...ids, ...(s?.busy ?? []).map((b) => b.id), ...(s?.actions ?? []).filter((a) => a.locked).map((a) => a.id)]);
	}

	/** Stores insight `ids` as dismissed on this device. */
	function dismissInsights(ids: string[]) {
		return data.run(() => client.setSetting('dismissed_insights', dismissedList(s?.settings.dismissed_insights, ids, liveInsights())));
	}

	/** What the shared Inbox delete logic (`inbox.ts`) needs from this page (B115). */
	const deps: InboxDeps = {
		decide: (group, id, decision) => ondecide(group, id, decision),
		ignoreGroup: async (name) => {
			hiddenTriage = [...hiddenTriage, name];
			await decideGroup(name, [IGNORE]);
		},
		ignorePlace: (id) => data.run(() => location.decidePrompt(id, { kind: 'ignore' })),
		dismissInsights,
		storeRuns: async (ids) => {
			const live = offline ? null : new Set(runs.map((r) => r.run_id));
			await data.run(() => client.setSetting('dismissed_runs', dismissedList(s?.settings.dismissed_runs, ids, live)));
			await refresh();
		},
		cancelRun: (id) => engine.cancel(id),
		runState: (id) => runs.find((r) => r.run_id === id)?.state
	};

	/** Shows the refusals of runs that would not stop. */
	function refusedRuns(messages: string[]) {
		if (messages.length) data.error = `The run was not stopped: ${messages.join('; ')}`;
	}

	/** Deletes one Inbox entry (B115). */
	async function ondelete(group: DeleteGroup, id: string) {
		refusedRuns(await deleteEntry(group, id, deps));
	}

	/** Clears the Inbox (B115), every unanswered calendar group of the waiting triage included. */
	async function onclear() {
		if (!inbox) return;
		const v = inbox;
		const groups = triageCards(triage?.waiting).map((t) => t.id).filter((name) => !(name in answers));
		const shown = { places: v.places.map((p) => p.id), approvals: v.approvals.map((a) => a.id), suggestions: v.suggestions.map((f) => f.id), insights: v.insights.map((i) => i.id), groups, runs: v.runs.flatMap((r) => (r.id ? [r.id] : [])) };
		refusedRuns(await clearAll(shown, deps));
	}

	/**
	 * Routes a card decision: a calendar group answer, a suggestion, an insight dismissal, or a
	 * proposal applied or rejected (through its waiting run when it has one).
	 */
	async function ondecide(group: 'triage' | 'approval' | 'suggestion' | 'insight', id: string, decision: unknown) {
		if (group === 'triage') return decideGroup(id, decision);
		const yes = decision === 'apply';
		if (group === 'suggestion') return data.run(() => client.decidePreference(id, yes));
		if (group === 'insight') return dismissInsights([id]);
		if (group !== 'approval') return;
		if (id.startsWith(location.PATTERN_PREFIX)) return data.run(() => location.decidePattern(id.slice(location.PATTERN_PREFIX.length), yes));
		const decided = await decideProposal(id, s?.proposals.find((p) => p.id === id)?.goal_id ?? null, yes);
		if (!decided.ok) return void (data.error = decided.message);
		data.error = null;
		if (!decided.value) return;
		await follow(decided.value);
		await loadRuns();
	}
</script>

{#if offline}
	<div class="flex items-center gap-3 px-4 pt-3" role="alert">
		<p class="min-w-0 flex-1 text-sm text-status-missed">Could not reach the engine, so runs waiting for you may be missing.</p>
		<button type="button" class="min-h-11 rounded-sm border border-border bg-bg px-3 text-sm text-fg outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => void loadRuns()}>Retry</button>
	</div>
{/if}
{#if inbox}
	<InboxScreen {...inbox} {ondecide} {onplace} {ondelete} {onclear} />
{/if}
