<script lang="ts">
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import Droplet from '@lucide/svelte/icons/droplet';
	import Dumbbell from '@lucide/svelte/icons/dumbbell';
	import KeyRound from '@lucide/svelte/icons/key-round';
	import Lock from '@lucide/svelte/icons/lock';
	import NotebookPen from '@lucide/svelte/icons/notebook-pen';
	import Pencil from '@lucide/svelte/icons/pencil';
	import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
	import Sunrise from '@lucide/svelte/icons/sunrise';
	import Utensils from '@lucide/svelte/icons/utensils';
	import { Button } from '$lib/ui/primitives/button';
	import { formatDate, formatTime, formatYmd } from '$lib/ui/format';
	import DiaryCardEdit from './DiaryCardEdit.svelte';
	import DiaryWords from './DiaryWords.svelte';
	import { diaryDeleteEntry, diaryEditCard, diaryEditEntry, diaryMoveEntry, diaryRedo, redoNeedsConfirm, type CardDraft, type DayResult, type DiaryDayView } from '$lib/app/diary';

	/**
	 * One diary day card (Q85). `compact` is the summary row used on the calendar day and for past
	 * days (`onopen` opens the full card); `full` shows the summary, mood and energy, highlights, the
	 * done sessions (`onaction`), food, water, workout, notes for tomorrow and the verbatim entries
	 * behind a toggle. With no structure it shows `note` (the no-key line by default, `onsettings`).
	 * Given `onchange`, the full card offers Redo summary, editing every card field and editing or
	 * deleting entries (B118 to B120) or moving them to another day (B127), and hands each fresh
	 * day and note to `onchange`.
	 */
	let {
		day,
		mode = 'full',
		zone,
		title,
		note = null,
		onopen,
		onaction,
		onsettings,
		onchange
	}: {
		day: DiaryDayView;
		mode?: 'compact' | 'full';
		zone: string;
		title?: string;
		note?: string | null;
		onopen?: () => void;
		onaction?: (id: string) => void;
		onsettings?: () => void;
		onchange?: (next: { view: DiaryDayView; note: string | null }) => void;
	} = $props();
	const noKey = $derived(!!note?.startsWith('Add your Claude key'));

	const s = $derived(day.structure);
	const label = $derived(title ?? formatDate(day.day, { weekday: 'long', day: 'numeric', month: 'long' }));
	const count = $derived(`${day.entries.length} ${day.entries.length === 1 ? 'entry' : 'entries'}`);
	let editing = $state(false);
	let confirming = $state(false);
	let redoing = $state(false);
	let failure = $state<string | null>(null);

	/** Hands a successful result to `onchange`; returns the failure sentence or null. */
	function apply(r: DayResult): string | null {
		if (!r.ok) return r.message;
		onchange?.({ view: r.view, note: r.note });
		return null;
	}

	/** Redo summary (B118): asks first when the card holds the owner's edits, then restructures. */
	async function redo(confirmed = false) {
		if (!confirmed && redoNeedsConfirm(day)) return void (confirming = true);
		confirming = false;
		redoing = true;
		failure = apply(await diaryRedo(day.day, day.done));
		redoing = false;
	}

	/** Saves the edit form's draft; closes the form when it saved. */
	async function save(draft: CardDraft): Promise<string | null> {
		const error = apply(await diaryEditCard(day.day, draft, day.done));
		if (!error) editing = false;
		return error;
	}
</script>

{#snippet scale(name: string, value: number)}
	<div class="flex items-center gap-2" role="img" aria-label="{name} {value} of 5">
		<span class="w-14 text-sm text-muted">{name}</span>
		<span class="flex gap-1" aria-hidden="true">
			{#each [1, 2, 3, 4, 5] as n (n)}<span class="h-2 w-5 rounded-full {n <= value ? 'bg-fg' : 'bg-surface-2 ring-1 ring-border ring-inset'}"></span>{/each}
		</span>
		<span class="text-sm tabular-nums">{value}/5</span>
	</div>
{/snippet}

{#snippet line(Icon: typeof Droplet, name: string, value: string | null)}
	<li class="flex items-start gap-3 py-2">
		<Icon class="mt-0.5 size-4 shrink-0 text-muted" aria-hidden="true" />
		<span class="w-16 shrink-0 text-sm text-muted">{name}</span>
		<span class="min-w-0 flex-1 text-sm {value ? '' : 'text-muted'}">{value ?? 'Nothing logged'}</span>
	</li>
{/snippet}

{#if mode === 'compact'}
	<button type="button" onclick={() => onopen?.()} class="flex w-full flex-col gap-2 rounded-sm border border-border bg-bg p-3 text-left outline-none hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50">
		<span class="flex items-center gap-2">
			<NotebookPen class="size-4 shrink-0 text-muted" aria-hidden="true" />
			<span class="flex-1 text-sm font-semibold">Diary, {label}</span>
			<Lock class="size-3.5 text-muted" aria-label="Encrypted" />
			<ChevronRight class="size-4 text-muted" aria-hidden="true" />
		</span>
		{#if s}
			<span class="line-clamp-2 text-sm">{s.summary}</span>
			<span class="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted tabular-nums">
				<span>Mood {s.mood}/5</span><span>Energy {s.energy}/5</span><span>{count}</span>
			</span>
		{:else}
			<span class="text-sm text-muted">{count}, not structured yet</span>
		{/if}
	</button>
{:else}
	<article class="flex flex-col gap-4 rounded-sm border border-border bg-bg p-4" aria-label="Diary for {label}">
		<header class="flex items-start gap-2">
			<div class="flex min-w-0 flex-1 flex-col">
				<h3 class="text-lg font-semibold">{label}</h3>
				<p class="flex items-center gap-1 text-xs text-muted"><Lock class="size-3" aria-hidden="true" />End-to-end encrypted · {count}</p>
			</div>
			{#if s && onchange && !editing}
				<Button variant="ghost" size="touch" onclick={() => ((editing = true), (confirming = false))}><Pencil />Edit</Button>
			{/if}
		</header>

		{#if s && editing}
			<DiaryCardEdit structure={s} onsave={save} oncancel={() => (editing = false)} />
		{:else if s}
			<p class="text-base">{s.summary}</p>
			<div class="flex flex-col gap-1.5">{@render scale('Mood', s.mood)}{@render scale('Energy', s.energy)}</div>

			{#if s.highlights.length}
				<section class="flex flex-col gap-1" aria-label="Highlights">
					<h4 class="text-sm font-semibold">Highlights</h4>
					<ul class="flex list-disc flex-col gap-0.5 pl-5 text-sm">
						{#each s.highlights as h, i (i)}<li>{h}</li>{/each}
					</ul>
				</section>
			{/if}
			{#if day.stale}<p class="text-sm text-muted">Your newest words are not in this edited card. Redo summary to include them.</p>{/if}
		{/if}

		{#if day.done.length}
			<section class="flex flex-col gap-1" aria-label="What was done">
				<h4 class="text-sm font-semibold">What was done</h4>
				<ul class="flex flex-col gap-1">
					{#each day.done as a (a.id)}
						<li>
							<button type="button" onclick={() => onaction?.(a.id)} class="flex min-h-11 w-full items-center gap-2 rounded-sm border-l-3 px-3 text-left text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50" style="border-left-color: var(--cat-{a.category}); background: color-mix(in oklch, var(--cat-{a.category}) 8%, var(--bg))">
								<span class="flex-1">{a.title}</span>
								<span class="text-xs text-muted tabular-nums">{formatTime(a.start, zone)}</span>
								<span class="text-xs text-status-done">Done</span>
								<ChevronRight class="size-4 text-muted" aria-hidden="true" />
							</button>
						</li>
					{/each}
				</ul>
			</section>
		{/if}

		{#if s && !editing}
			<ul class="flex flex-col divide-y divide-border border-y border-border">
				{@render line(Utensils, 'Food', s.food)}
				{@render line(Droplet, 'Water', s.water)}
				{@render line(Dumbbell, 'Workout', s.workout)}
			</ul>
			{#if s.notesForTomorrow}
				<section class="flex gap-3 rounded-sm bg-surface-2 p-3" aria-label="Notes for tomorrow">
					<Sunrise class="mt-0.5 size-4 shrink-0 text-muted" aria-hidden="true" />
					<div class="flex flex-col gap-0.5">
						<h4 class="text-sm font-semibold">Notes for tomorrow</h4>
						<p class="text-sm">{s.notesForTomorrow}</p>
					</div>
				</section>
			{/if}
			{#if note}<p class="text-sm font-medium" role="status">{note}</p>{/if}
		{:else if !s}
			<section class="flex flex-col gap-2 rounded-sm border border-dashed border-border-strong p-3" aria-label="Not structured">
				{#if noKey}
					<p class="flex items-start gap-2 text-sm font-medium"><KeyRound class="mt-0.5 size-4 shrink-0" aria-hidden="true" />Add your Claude key in Settings to structure this day</p>
					<p class="text-sm text-muted">Your words are saved and encrypted. Once your key is set, the Aroh engine writes the summary, mood and energy, food, water and workout from them.</p>
					<Button variant="outline" class="min-h-11 self-start" onclick={() => onsettings?.()}>Open Settings</Button>
				{:else}
					<p class="text-sm font-medium">{note ?? 'Not structured yet. Your words are saved.'}</p>
				{/if}
			</section>
		{/if}

		{#if onchange && !editing && !noKey && day.entries.length}
			<div class="flex flex-col gap-2">
				{#if confirming}
					<div role="alertdialog" aria-label="Replace your edits" class="flex flex-col gap-2 rounded-sm border border-border bg-surface-2 p-3">
						<p class="text-sm">You edited this card. Redo summary replaces your edits with a new summary from your words.</p>
						<div class="flex justify-end gap-2">
							<Button variant="outline" size="touch" onclick={() => (confirming = false)}>Keep my edits</Button>
							<Button size="touch" onclick={() => void redo(true)}>Redo anyway</Button>
						</div>
					</div>
				{:else}
					<Button variant="outline" size="touch" class="self-start" disabled={redoing} onclick={() => void redo()}><RotateCcw />{s ? 'Redo summary' : 'Structure now'}</Button>
				{/if}
				{#if redoing}<p class="text-sm text-muted" role="status">Redoing the summary on the Aroh engine…</p>{/if}
				{#if failure}<p class="text-sm text-status-missed" role="alert">{failure}</p>{/if}
			</div>
		{/if}

		<DiaryWords
			entries={day.entries}
			{zone}
			day={day.day}
			onedit={onchange ? async (id, text) => apply(await diaryEditEntry(day.day, id, text, day.done)) : undefined}
			today={formatYmd(new Date().toISOString(), zone)}
			onmove={onchange ? async (id, to) => apply(await diaryMoveEntry(day.day, id, to, day.done)) : undefined}
			ondelete={onchange ? async (id) => apply(await diaryDeleteEntry(day.day, id, day.done)) : undefined}
		/>

		{#if s}
			<p class="text-xs text-muted">
				Structured at {formatTime(s.structuredAt, zone)} by the Aroh engine.{#if s.editedAt}{' '}Edited by you at {formatTime(s.editedAt, zone)}.{/if}
			</p>
		{/if}
	</article>
{/if}
