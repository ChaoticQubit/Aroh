<script lang="ts">
	import { onMount } from 'svelte';
	import PlacesScreen from '$lib/ui/screens/PlacesScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import * as location from '$lib/app/location';

	const SEARCH_MS = 400;
	const s = $derived(data.snapshot);
	let v = $state<location.LocationView | null>(null);
	let query = $state('');
	let hits = $state<location.PlaceHit[]>([]);
	let map = $state<string | undefined>(undefined);
	let timer: ReturnType<typeof setTimeout> | undefined;

	const center = $derived.by(() => {
		const list = v?.places ?? [];
		if (!list.length) return null;
		return { lat: list.reduce((a, p) => a + p.lat, 0) / list.length, lon: list.reduce((a, p) => a + p.lon, 0) / list.length };
	});

	onMount(() => location.follow((next) => (v = next), (e) => (data.error = String(e))));

	$effect(() => {
		const at = center;
		if (!at) return;
		location.snapshot(at.lat, at.lon, 360, 140).then(
			(url) => (map = url),
			() => (map = undefined)
		);
	});

	/** Searches MapKit a moment after the owner stops typing. */
	function onsearch(text: string) {
		query = text;
		clearTimeout(timer);
		if (!text.trim()) return void (hits = []);
		timer = setTimeout(() => {
			location.search(text).then(
				(found) => query === text && (hits = found),
				(e) => (data.error = String(e))
			);
		}, SEARCH_MS);
	}

	/** Names the picked search hit and clears the search. */
	function onadd(resultId: string) {
		const hit = hits[Number(resultId)];
		if (!hit) return;
		void data.run(async () => {
			await location.addPlace(hit);
			query = '';
			hits = [];
		});
	}
</script>

{#if v && s}
	<PlacesScreen places={location.placeRows(v, s)} {query} results={location.resultRows(hits, center)} {map} {onsearch} {onadd} />
{/if}
