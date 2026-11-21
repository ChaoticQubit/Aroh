<script lang="ts">
	import CalendarScreen from '$lib/ui/screens/CalendarScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import { actionViews, categoryList } from '$lib/app/views';

	const s = $derived(data.snapshot);
</script>

{#if s}
	<CalendarScreen actions={actionViews(s)} categories={categoryList(s)} />
{/if}
