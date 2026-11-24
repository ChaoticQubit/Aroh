<script lang="ts">
	import { goto } from '$app/navigation';
	import GoalsScreen from '$lib/ui/screens/GoalsScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import { categoryList, goalRows } from '$lib/app/views';
	import { getClock } from '$lib/ui/clock';

	const clock = getClock();
	const s = $derived(data.snapshot);
</script>

{#if s}
	<GoalsScreen goals={goalRows(s, clock.zone)} categories={categoryList(s)} onopen={(id) => goto(`/goals/${id}`)} onnew={() => goto('/goals/new')} />
{/if}
