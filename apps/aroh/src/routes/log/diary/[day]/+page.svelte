<script lang="ts">
	import { page } from '$app/state';
	import DayRoute from '$lib/ui/diary/DayRoute.svelte';

	const day = $derived(page.params.day ?? '');
</script>

{#key day}
	<DayRoute {day} />
{/key}
