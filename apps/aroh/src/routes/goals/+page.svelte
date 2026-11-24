<script lang="ts">
	import { goto } from '$app/navigation';
	import GoalsScreen from '$lib/ui/screens/GoalsScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import { categoryList, goalProgress, goalRows } from '$lib/app/views';
	import { dashboard, type GoalStats } from '$lib/app/dashboard';
	import { getClock } from '$lib/ui/clock';

	const clock = getClock();
	const s = $derived(data.snapshot);
	let stats = $state<GoalStats[] | null>(null);

	$effect(() => {
		if (!s) return;
		dashboard().then(
			(d) => (stats = d.goals),
			(e) => (data.error = String(e))
		);
	});
</script>

{#if s}
	<GoalsScreen goals={goalRows(s, clock.zone)} categories={categoryList(s)} progress={goalProgress(s, stats, clock)} onopen={(id) => goto(`/goals/${id}`)} onnew={() => goto('/goals/new')} />
{/if}
