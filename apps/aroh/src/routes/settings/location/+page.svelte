<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import LocationExplainer from '$lib/ui/components/location/LocationExplainer.svelte';
	import LocationSettingsScreen from '$lib/ui/screens/LocationSettingsScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import * as location from '$lib/app/location';
	import { openSettings } from '$lib/app/speech.svelte';

	let v = $state<location.LocationView | null>(null);
	let status = $state<string | null>(null);

	onMount(() => location.follow((next) => (v = next), (e) => (data.error = String(e))));

	/** Runs the two iOS permission steps and shows the resulting status line. */
	async function oncontinue() {
		await data.run(async () => (status = await location.enable()));
	}

	/** Reports a switch: pausing tracking or turning nudges off. */
	function onsetting(key: 'paused' | 'nudges', on: boolean) {
		void data.run(() => location.setSetting(key === 'paused' ? 'location_paused' : 'location_nudges', on));
	}
</script>

{#if v && !v.settings.enabled}
	<LocationExplainer {oncontinue} onlater={() => void goto('/settings')} />
{:else if v}
	{#if status}<p class="px-4 pt-4 text-sm text-muted">{status}</p>{/if}
	<LocationSettingsScreen
		settings={location.locationSettingsView(v)}
		{onsetting}
		onunignore={(id) => void data.run(() => location.unignore(id))}
		onclear={() => void data.run(location.clear)}
		onplaces={() => void goto('/settings/places')}
		onlog={() => void goto('/settings/location/log')}
		onopensettings={() => void openSettings().catch((e) => (data.error = String(e)))}
	/>
{/if}
