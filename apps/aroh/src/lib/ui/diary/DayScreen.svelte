<script lang="ts">
	import Smartphone from '@lucide/svelte/icons/smartphone';
	import DiaryCard from './DiaryCard.svelte';
	import DiaryComposer from './DiaryComposer.svelte';
	import PageLoader from '$lib/ui/notes/PageLoader.svelte';
	import { formatDate } from '$lib/ui/format';
	import { isDate } from '$lib/app/notes';
	import type { DiaryDayView, DiaryLoad } from '$lib/app/diary';
	import type { Speech } from '$lib/app/speech.svelte';

	/**
	 * One day as a single screen (Q96, B126): the date header, the diary of `day` (composer, card
	 * and Past days when `current`, the full card otherwise, or the "available on iPhone" note),
	 * then the day page editor with Subpages and Backlinks. `load` is the diary state (null while
	 * loading); `title` names the card; the callbacks are the diary ones of DiaryCard and DiaryComposer.
	 */
	let {
		day,
		load,
		current,
		title,
		zone,
		busy = false,
		voice,
		onsend,
		onopen,
		onaction,
		onsettings,
		onchange
	}: {
		day: string;
		load: DiaryLoad | null;
		current: boolean;
		title?: string;
		zone: string;
		busy?: boolean;
		voice?: Speech;
		onsend: (text: string, spoken: boolean) => void;
		onopen: (day: string) => void;
		onaction: (id: string) => void;
		onsettings: () => void;
		onchange: (next: { view: DiaryDayView; note: string | null }) => void;
	} = $props();

	const heading = $derived(isDate(day) ? formatDate(day, { weekday: 'long', day: 'numeric', month: 'long' }) : day);
	const count = $derived(load?.kind === 'ready' ? load.today.entries.length : 0);
</script>

<div class="flex flex-col">
	<div class="mx-auto flex w-full max-w-2xl flex-col gap-4 px-4 pt-4">
		<header class="flex flex-col gap-1">
			<h2 class="text-2xl leading-tight font-semibold">{heading}</h2>
			{#if load?.kind === 'ready' && (current || count)}<p class="text-sm text-muted">{current && title ? `${title}: ` : ''}{count} diary {count === 1 ? 'entry' : 'entries'}.</p>{/if}
		</header>
		{#if load?.kind === 'unavailable'}
			<section class="flex max-w-lg flex-col items-start gap-3 rounded-sm border border-border bg-bg p-5" aria-label="Diary">
				<Smartphone class="size-6 text-muted" aria-hidden="true" />
				<h3 class="text-lg font-semibold">Your diary is available on iPhone</h3>
				<p class="text-sm text-muted">The diary is end-to-end encrypted with a key that lives in your iPhone's iCloud Keychain. Write and read it in Aroh on your iPhone. Trackers and check-ins still work here.</p>
			</section>
		{:else}
			{#if current}
				<DiaryComposer {onsend} {voice} {busy} />
				{#if busy}<p class="text-sm text-muted" role="status">Saving and structuring…</p>{/if}
			{/if}
			{#if load?.kind === 'error'}
				<p class="text-sm text-status-missed" role="alert">{load.message}</p>
			{:else if load?.kind === 'ready'}
				{#if load.today.entries.length}
					<DiaryCard day={load.today} {zone} title={current ? title : undefined} note={load.note} {onaction} {onsettings} {onchange} />
				{:else if !current}
					<p class="text-sm text-muted">No diary for this day on this phone.</p>
				{/if}
			{/if}
		{/if}
	</div>

	<PageLoader kind="day" target={day} embedded />

	{#if current && load?.kind === 'ready' && load.past.length}
		<section class="mx-auto flex w-full max-w-2xl flex-col gap-2 px-4 pb-4" aria-label="Past days">
			<h3 class="px-1 text-sm font-semibold">Past days</h3>
			{#each load.past as p (p.day)}<DiaryCard day={p} mode="compact" {zone} onopen={() => onopen(p.day)} />{/each}
		</section>
	{/if}
</div>
