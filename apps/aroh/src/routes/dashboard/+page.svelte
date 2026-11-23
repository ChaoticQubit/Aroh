<script lang="ts">
	import { goto } from '$app/navigation';
	import DashboardScreen from '$lib/ui/components/dashboard/DashboardScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import { dashboard, dashGoals, type Dashboard } from '$lib/app/dashboard';

	const s = $derived(data.snapshot);
	let figures = $state<Dashboard | null>(null);

	$effect(() => {
		if (!s) return;
		dashboard().then(
			(d) => (figures = d),
			(e) => (data.error = String(e))
		);
	});
</script>

{#if s && figures}
	<DashboardScreen goals={dashGoals(s, figures)} weeks={figures.weeks} trendWeeks={figures.trend_weeks.length} onopen={(id) => goto(`/goals/${id}`)} onnew={() => goto('/goals/new')} />
{/if}
