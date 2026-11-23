<script lang="ts">
	import Clock from '@lucide/svelte/icons/clock';
	import NotebookPen from '@lucide/svelte/icons/notebook-pen';
	import Sparkles from '@lucide/svelte/icons/sparkles';
	import * as Card from '$lib/ui/primitives/card';
	import { Button } from '$lib/ui/primitives/button';
	import Segmented from '../Segmented.svelte';
	import { progress, type Card as ActionCardData, type CardLog, type CardMeta } from '$lib/ui/action/card';
	import { formatDate } from '$lib/ui/format';
	import WorkoutBlock from './WorkoutBlock.svelte';
	import ListBlock from './ListBlock.svelte';
	import CardComposer from './CardComposer.svelte';
	import EditBlock from './EditBlock.svelte';
	import { setText, stepSet, toggleItem, toggleSet } from '$lib/ui/action/edit';

	/**
	 * Full page of one action, like a note (Q71): header, why, the structured blocks, notes, the session
	 * log with easy / ok / hard, and the card chat composer with mic. `mode` `log` turns the workout into
	 * per-set logging; an `outline` card shows its one line, the later weeks and when it is filled.
	 * With `oncard` the plan is edited in place (why, rows, notes via `onnotes`); log taps call `onlog`.
	 */
	let {
		meta,
		card,
		log,
		notes = null,
		mode = 'plan',
		editing = null,
		weeks = [],
		fillOn = null,
		mic = true,
		draft = $bindable(''),
		listening = false,
		hint = null,
		onchat,
		oncard,
		onnotes,
		onlog,
		onedit,
		onstart,
		onfinish,
		onmic
	}: {
		meta: CardMeta;
		card: ActionCardData;
		log: CardLog;
		notes?: string | null;
		mode?: 'plan' | 'log';
		editing?: { row: number; set: number } | { block: number; row: number; set: number } | null;
		weeks?: { week: string; dates: string; line: string }[];
		fillOn?: string | null;
		mic?: boolean;
		draft?: string;
		listening?: boolean;
		hint?: string | null;
		onchat?: (text: string) => void;
		oncard?: (card: ActionCardData) => void;
		onnotes?: (notes: string) => void;
		onlog?: (log: CardLog) => void;
		onedit?: (at: { block: number; row: number; set: number } | null) => void;
		onstart?: () => void;
		onfinish?: () => void;
		onmic?: () => void;
	} = $props();

	let effort = $derived<string>(log.effort ?? '');
	const fieldId = $props.id();
	const textarea = 'field-sizing-content min-h-11 w-full resize-none rounded-sm border border-transparent bg-transparent px-2 py-2 text-lg outline-none hover:border-border focus-visible:border-ring focus-visible:bg-bg focus-visible:ring-3 focus-visible:ring-ring/50';
	const done = $derived(progress(card, log));
	const end = $derived.by(() => {
		const [h, m] = meta.start.split(':').map(Number);
		const t = h * 60 + m + meta.minutes;
		return `${String(Math.floor(t / 60)).padStart(2, '0')}:${String(t % 60).padStart(2, '0')}`;
	});
	const heading = (b: ActionCardData['blocks'][number]) => (b.kind === 'workout' ? 'Workout' : b.title);
</script>

<div class="flex min-h-full flex-col">
	<div class="flex flex-1 flex-col gap-4 p-4">
		<header class="flex flex-col gap-1.5">
			<h2 class="text-xl font-semibold break-words">{meta.title}</h2>
			<p class="flex items-center gap-2 text-sm text-muted">
				<span class="size-2 shrink-0 rounded-full" style="background: var(--cat-{meta.hue})" aria-hidden="true"></span>
				<span class="min-w-0 truncate">{meta.goal}</span>
			</p>
			<p class="flex flex-wrap items-center gap-x-2 text-sm tabular-nums">
				<Clock class="size-3.5 text-muted" aria-hidden="true" />{formatDate(meta.date, { weekday: 'short', day: 'numeric', month: 'short' })} · {meta.start} to {end} · {meta.minutes} min
			</p>
			<p class="text-xs text-muted">{meta.week} · {meta.coach}</p>
		</header>

		{#if card.depth === 'outline' && !card.blocks.length}
			<Card.Root size="sm" class="gap-2 px-4">
				<p class="flex items-center gap-1.5 text-xs font-medium text-muted"><Sparkles class="size-3.5" aria-hidden="true" />Outline</p>
				<p class="text-md">{card.outline}</p>
				{#if fillOn}<p class="text-sm text-muted">The full card with sets, reps and weights is written on {formatDate(fillOn, { weekday: 'long', day: 'numeric', month: 'short' })} from what you log until then.</p>{/if}
			</Card.Root>
			{#if weeks.length}
				<section class="flex flex-col gap-2" aria-labelledby="weeks-ahead">
					<h3 id="weeks-ahead" class="text-sm font-semibold">Weeks ahead</h3>
					<ol class="flex flex-col divide-y divide-border rounded-md border border-border bg-bg">
						{#each weeks as w (w.week)}
							<li class="flex flex-col gap-0.5 px-4 py-3">
								<p class="flex items-baseline justify-between gap-2 text-sm"><span class="font-medium">{w.week}</span><span class="text-xs text-muted tabular-nums">{w.dates}</span></p>
								<p class="text-sm text-muted">{w.line}</p>
							</li>
						{/each}
					</ol>
				</section>
			{/if}
		{:else}
			{#if mode === 'log'}
				<div class="flex flex-col gap-1.5" role="status" aria-label="Session progress">
					<p class="flex justify-between text-sm"><span class="font-medium">Session in progress</span><span class="text-muted tabular-nums">{done.done} of {done.total} done</span></p>
					<div class="h-1.5 overflow-hidden rounded-full bg-surface-2"><div class="h-full rounded-full bg-status-done" style="width: {(done.done / Math.max(done.total, 1)) * 100}%"></div></div>
				</div>
			{/if}

			{#if mode === 'plan'}
				<section class="flex flex-col gap-2" aria-labelledby="why">
					<h3 id="why" class="text-sm font-semibold">{#if oncard}<label for="{fieldId}-why">Why</label>{:else}Why{/if}</h3>
					{#if oncard}
						<textarea id="{fieldId}-why" class="{textarea} -mx-2" value={card.why} oninput={(e) => oncard(setText(card, 'why', e.currentTarget.value))}></textarea>
					{:else}
						<p class="text-md">{card.why}</p>
					{/if}
				</section>
			{/if}

			{#each card.blocks as b, i (i)}
				<section class="flex flex-col gap-2" aria-labelledby="block-{i}">
					<h3 id="block-{i}" class="flex items-baseline justify-between text-sm font-semibold {oncard && mode === 'plan' && b.kind !== 'workout' ? 'sr-only' : ''}">
						{heading(b)}
						{#if b.kind === 'workout' && mode === 'log'}<span class="text-xs font-normal text-muted">Tick a set, tap a number to change it</span>{/if}
					</h3>
					<Card.Root size="sm" class="overflow-hidden px-4">
						{#if oncard && mode === 'plan'}
							<EditBlock {card} index={i} onchange={oncard} />
						{:else if b.kind === 'workout'}
							<WorkoutBlock
								block={b}
								index={i}
								{log}
								{mode}
								editing={editing && 'block' in editing ? (editing.block === i ? editing : null) : editing}
								ontick={(row, set) => onlog?.(toggleSet(card, log, i, row, set))}
								onedit={(row, set) => onedit?.(editing && 'block' in editing && editing.block === i && editing.row === row && editing.set === set ? null : { block: i, row, set })}
								onstep={(row, set, reps, weight) => onlog?.(stepSet(card, log, i, row, set, reps, weight))}
							/>
						{:else}
							<ListBlock block={b} index={i} {log} ontoggle={mode === 'log' ? (row) => onlog?.(toggleItem(log, i, row)) : undefined} />
						{/if}
					</Card.Root>
				</section>
			{/each}

			{#if onnotes && mode === 'plan'}
				<section class="flex flex-col gap-2" aria-labelledby="notes">
					<h3 id="notes" class="flex items-center gap-1.5 text-sm font-semibold"><NotebookPen class="size-3.5 text-muted" aria-hidden="true" /><label for="{fieldId}-notes">Notes</label></h3>
					<textarea id="{fieldId}-notes" class="{textarea} rounded-md border-border bg-bg px-4 py-3" placeholder="Tap to add notes" value={notes ?? ''} oninput={(e) => onnotes(e.currentTarget.value)}></textarea>
				</section>
			{:else if notes}
				<section class="flex flex-col gap-2" aria-labelledby="notes">
					<h3 id="notes" class="flex items-center gap-1.5 text-sm font-semibold"><NotebookPen class="size-3.5 text-muted" aria-hidden="true" />Notes</h3>
					<p class="rounded-md border border-border bg-bg px-4 py-3 text-md whitespace-pre-line">{notes}</p>
				</section>
			{/if}
			{#if hint}<p class="text-sm text-status-warn" role="status">{hint}</p>{/if}

			{#if mode === 'log'}
				<section class="flex flex-col gap-2" aria-labelledby="how">
					<h3 id="how" class="text-sm font-semibold">How did it feel?</h3>
					<div><Segmented label="How did it feel" bind:value={() => effort, (v) => onlog?.({ ...log, effort: (v || null) as CardLog['effort'] })} options={[{ value: 'easy', label: 'Easy' }, { value: 'ok', label: 'OK' }, { value: 'hard', label: 'Hard' }]} /></div>
					<Button size="touch" class="mt-1" onclick={() => onfinish?.()}>Finish session</Button>
				</section>
			{:else}
				<Button size="touch" onclick={() => onstart?.()}>Start session</Button>
			{/if}
		{/if}
	</div>
	<div data-composer class="sticky bottom-0 z-10">
		<CardComposer placeholder={mode === 'log' ? "Say 'last set 7 reps'" : 'Change this card'} {mic} {listening} bind:draft onsend={onchat} {onmic} />
	</div>
</div>
