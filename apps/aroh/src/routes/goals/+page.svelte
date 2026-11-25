<script lang="ts">
	import { goto } from '$app/navigation';
	import GoalsScreen from '$lib/ui/screens/GoalsScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import { categoryList, goalProgress, goalRows } from '$lib/app/views';
	import { dashboard, type GoalStats } from '$lib/app/dashboard';
	import { getClock } from '$lib/ui/clock';
	import { engine, unreachable, type RunSummary } from '$lib/app/engine';
	import { mergeRuns } from '$lib/app/goalcard';
	import { refreshRuns } from '$lib/app/realtime';
	import { onMount } from 'svelte';

	const WAITING = { state: 'waiting_human' };

	const clock = getClock();
	const s = $derived(data.snapshot);
	let stats = $state<GoalStats[] | null>(null);
	let runs = $state<RunSummary[]>(mergeRuns(engine.cachedRuns() ?? [], engine.cachedRuns(WAITING) ?? []));
	let offline = $state(false);
	const progress = $derived(s ? goalProgress(s, stats, clock, runs) : {});

	const older: Record<string, RunSummary[]> = {};

	/**
	 * Reads the newest runs and every waiting run (which can be older than the newest 50), and once
	 * per goal the runs of a draft or active goal with none in those lists, so each goal's chip
	 * follows its own newest run. A failed read sets `offline` and keeps the last list.
	 */
	async function loadRuns() {
		const [all, waiting] = await Promise.all([engine.runs(), engine.runs(WAITING)]);
		offline = unreachable(all) || unreachable(waiting);
		if (!all.ok || !waiting.ok) return;
		const merged = mergeRuns(all.value, waiting.value);
		const listed = new Set(merged.map((r) => r.goal_id));
		const missing = (data.snapshot?.goals ?? []).filter((g) => (g.status === 'draft' || g.status === 'active') && !listed.has(g.id) && !(g.id in older));
		const reads = await Promise.all(missing.map((g) => engine.runs({ goalId: g.id })));
		reads.forEach((r, i) => r.ok && (older[missing[i].id] = r.value));
		runs = mergeRuns(merged, Object.values(older).flat());
	}

	onMount(() => {
		void loadRuns();
		return refreshRuns(
			() => void loadRuns(),
			() => Object.values(progress).some((p) => p.label === 'Working')
		);
	});

	$effect(() => {
		if (!s) return;
		dashboard().then(
			(d) => (stats = d.goals),
			(e) => (data.error = String(e))
		);
	});
</script>

{#if offline}
	<div class="flex items-center gap-3 px-4 pt-3" role="alert">
		<p class="min-w-0 flex-1 text-sm text-status-missed">Could not reach the engine, so a goal's status may be out of date.</p>
		<button type="button" class="min-h-11 rounded-sm border border-border bg-bg px-3 text-sm text-fg outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => void loadRuns()}>Retry</button>
	</div>
{/if}
{#if s}
	<GoalsScreen goals={goalRows(s, clock.zone)} categories={categoryList(s)} {progress} onopen={(id) => goto(`/goals/${id}`)} onnew={() => goto('/goals/new')} />
{/if}
