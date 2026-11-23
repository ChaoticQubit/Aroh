<script lang="ts">
	import { onMount } from 'svelte';
	import LocationLogScreen from '$lib/ui/screens/LocationLogScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import * as location from '$lib/app/location';

	let v = $state<location.LocationView | null>(null);

	onMount(() => location.follow((next) => (v = next), (e) => (data.error = String(e))));
</script>

{#if v}
	<LocationLogScreen visits={location.visitRows(v)} />
{/if}
