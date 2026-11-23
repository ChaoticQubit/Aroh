<script lang="ts">
	import Search from '@lucide/svelte/icons/search';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import { Input } from '$lib/ui/primitives/input';
	import PlaceMap from '../components/location/PlaceMap.svelte';
	import type { PlaceResult, PlaceRow } from '../components/location/types';

	/**
	 * Places: a map search to add a place, the named places with their linked goals and watched state,
	 * and the ignored places. At most `limit` places are watched, chosen by relevance.
	 * `query` and `results` show the search state, `map` is a MapKit snapshot of your places;
	 * `onadd` gets a result id, `onopen` a place id.
	 */
	let {
		places,
		limit = 20,
		query = '',
		results = [],
		map,
		onsearch,
		onadd,
		onopen
	}: {
		places: PlaceRow[];
		limit?: number;
		query?: string;
		results?: PlaceResult[];
		map?: string;
		onsearch?: (text: string) => void;
		onadd?: (resultId: string) => void;
		onopen?: (placeId: string) => void;
	} = $props();

	const id = $props.id();
	let text = $derived(query);
	const named = $derived(places.filter((p) => p.state !== 'ignored'));
	const ignored = $derived(places.filter((p) => p.state === 'ignored'));
	const watched = $derived(places.filter((p) => p.state === 'watched').length);
	const searching = $derived(text.trim().length > 0);
	const pins = $derived(searching ? results.map((r) => ({ x: r.x, y: r.y, name: r.name })) : named.map((p) => ({ x: p.x, y: p.y, name: p.name })));
</script>

<div class="flex w-full max-w-3xl flex-col gap-6 p-4 @3xl:p-6">
	<div class="flex flex-col gap-3">
		<label for="{id}-search" class="sr-only">Search a place or address</label>
		<div class="relative">
			<Search class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted" aria-hidden="true" />
			<Input id="{id}-search" type="search" placeholder="Search a place or address" class="h-11 pl-9" bind:value={text} oninput={() => onsearch?.(text)} />
		</div>
		<PlaceMap label={searching ? 'Search results on the map' : 'Your places on the map'} {pins} height={searching ? 180 : 140} image={searching ? undefined : map} />
	</div>

	{#if searching}
		<section class="flex flex-col gap-2" aria-labelledby="{id}-results">
			<h2 id="{id}-results" class="text-sm font-semibold text-fg">Results</h2>
			<Card.Root size="sm" class="gap-0 divide-y divide-border px-4 py-0">
				{#each results as r (r.id)}
					<div class="flex min-h-14 items-center gap-3 py-2">
						<span class="flex flex-1 flex-col"><span class="text-sm font-medium">{r.name}</span><span class="text-xs text-muted">{r.address} · {r.distance}</span></span>
						<Button variant="outline" size="touch" onclick={() => onadd?.(r.id)}>Add</Button>
					</div>
				{:else}
					<p class="py-3 text-sm text-muted">No places match.</p>
				{/each}
			</Card.Root>
			<p class="text-xs text-muted">Adding a place names it. Link it to a goal or action next.</p>
		</section>
	{:else}
		<section class="flex flex-col gap-2" aria-labelledby="{id}-named">
			<div class="flex items-baseline justify-between">
				<h2 id="{id}-named" class="text-sm font-semibold text-fg">Your places</h2>
				<span class="text-xs text-muted tabular-nums">{watched} of {limit} watched</span>
			</div>
			<Card.Root size="sm" class="gap-0 divide-y divide-border px-0 py-0">
				{#each named as p (p.id)}
					<button type="button" class="flex min-h-14 w-full items-start gap-3 px-4 py-2.5 text-left outline-none hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset" onclick={() => onopen?.(p.id)}>
						<span class="flex min-w-0 flex-1 flex-col gap-1">
							<span class="text-sm font-medium">{p.name}</span>
							<span class="truncate text-xs text-muted">{p.address}</span>
							{#if p.links.length}
								<span class="flex flex-wrap gap-x-3 gap-y-1">
									{#each p.links as l (l.goal + (l.action ?? ''))}
										<span class="flex items-center gap-1.5 text-xs text-fg"><span class="size-2 shrink-0 rounded-full" style="background: var(--cat-{l.slot})"></span>{l.action ? `${l.action}, ${l.goal}` : l.goal}</span>
									{/each}
								</span>
							{:else}
								<span class="text-xs text-muted">Not linked</span>
							{/if}
						</span>
						<span class="shrink-0 text-xs {p.state === 'watched' ? 'font-medium text-fg' : 'text-muted'}">{p.state === 'watched' ? 'Watched' : 'Not watched'}</span>
					</button>
				{/each}
			</Card.Root>
			<p class="text-xs text-muted">iOS watches up to {limit} places. Aroh picks the most relevant ones.</p>
		</section>

		{#if ignored.length}
			<section class="flex flex-col gap-2" aria-labelledby="{id}-ignored">
				<h2 id="{id}-ignored" class="text-sm font-semibold text-fg">Ignored <span class="text-xs font-normal text-muted tabular-nums">{ignored.length}</span></h2>
				<Card.Root size="sm" class="gap-0 divide-y divide-border px-4 py-0">
					{#each ignored as p (p.id)}
						<div class="flex min-h-12 items-center gap-3 py-2">
							<span class="flex flex-1 flex-col"><span class="text-sm text-muted">{p.name}</span><span class="text-xs text-muted">{p.address}</span></span>
						</div>
					{/each}
				</Card.Root>
			</section>
		{/if}
	{/if}
</div>
