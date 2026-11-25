<script lang="ts">
	import Smartphone from '@lucide/svelte/icons/smartphone';
	import DiaryCard from './DiaryCard.svelte';
	import DiaryComposer from './DiaryComposer.svelte';
	import type { DiaryLoad } from '$lib/app/diary';
	import type { Speech } from '$lib/app/speech.svelte';

	/**
	 * The diary part of the Log tab (Q85): composer, today's card and the past days, or on the Mac
	 * the "available on iPhone" note. `load` is the current state (null while loading); `onsend`
	 * seals a new entry; `onopen` opens a day's full card; the card links go to `onaction` and
	 * `onsettings`.
	 */
	let {
		load,
		zone,
		busy = false,
		voice,
		onsend,
		onopen,
		onaction,
		onsettings
	}: {
		load: DiaryLoad | null;
		zone: string;
		busy?: boolean;
		voice?: Speech;
		onsend: (text: string, spoken: boolean) => void;
		onopen: (day: string) => void;
		onaction: (id: string) => void;
		onsettings: () => void;
	} = $props();
</script>

{#if load?.kind === 'unavailable'}
	<section class="m-4 flex max-w-lg flex-col items-start gap-3 rounded-sm border border-border bg-bg p-5" aria-label="Diary">
		<Smartphone class="size-6 text-muted" aria-hidden="true" />
		<h3 class="text-lg font-semibold">Your diary is available on iPhone</h3>
		<p class="text-sm text-muted">The diary is end-to-end encrypted with a key that lives in your iPhone's iCloud Keychain. Write and read it in Aroh on your iPhone. Trackers and check-ins still work here.</p>
	</section>
{:else}
	<div class="flex flex-col gap-4 p-4">
		<header class="flex flex-col gap-1">
			<h2 class="text-2xl leading-tight font-semibold">Log</h2>
			{#if load?.kind === 'ready'}<p class="text-sm text-muted">Today: {load.today.entries.length} diary {load.today.entries.length === 1 ? 'entry' : 'entries'}.</p>{/if}
		</header>
		<DiaryComposer {onsend} {voice} {busy} />
		{#if busy}<p class="text-sm text-muted" role="status">Saving and structuring…</p>{/if}
		{#if load?.kind === 'error'}
			<p class="text-sm text-status-missed" role="alert">{load.message}</p>
		{:else if load?.kind === 'ready'}
			{#if load.today.entries.length}
				<DiaryCard day={load.today} {zone} title="Today" note={load.note} onaction={(id) => onaction(id)} {onsettings} />
			{/if}
			{#if load.past.length}
				<section class="flex flex-col gap-2" aria-label="Past days">
					<h3 class="px-1 text-sm font-semibold">Past days</h3>
					{#each load.past as p (p.day)}<DiaryCard day={p} mode="compact" {zone} onopen={() => onopen(p.day)} />{/each}
				</section>
			{/if}
		{/if}
	</div>
{/if}
