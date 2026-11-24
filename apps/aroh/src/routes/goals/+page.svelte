<script lang="ts">
	import { goto } from '$app/navigation';
	import GoalsScreen from '$lib/ui/screens/GoalsScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import { categoryList, goalProgress, goalRows } from '$lib/app/views';
	import { dashboard, type GoalStats } from '$lib/app/dashboard';
	import { getClock } from '$lib/ui/clock';
	import { engine, unreachable } from '$lib/app/engine';
	import { onMount } from 'svelte';

	const clock = getClock();
	const s = $derived(data.snapshot);
	let stats = $state<GoalStats[] | null>(null);
	let waitingRuns = $state<string[]>([]);
	let offline = $state(false);

	/** Reads which goals have a run waiting for the owner; a failed read sets `offline`. */
	async function loadWaiting() {
		const r = await engine.runs({ state: 'waiting_human' });
		offline = unreachable(r);
		if (r.ok) waitingRuns = r.value.flatMap((run) => (run.goal_id ? [run.goal_id] : []));
	}

	onMount(() => void loadWaiting());

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
		<p class="min-w-0 flex-1 text-sm text-status-missed">Could not reach the engine, so a goal waiting for you may not say so.</p>
		<button type="button" class="min-h-11 rounded-sm border border-border bg-bg px-3 text-sm text-fg outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => void loadWaiting()}>Retry</button>
	</div>
{/if}
{#if s}
	<GoalsScreen goals={goalRows(s, clock.zone)} categories={categoryList(s)} progress={goalProgress(s, stats, clock, waitingRuns)} onopen={(id) => goto(`/goals/${id}`)} onnew={() => goto('/goals/new')} />
{/if}
