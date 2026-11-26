<script lang="ts">
	import CalendarArrowUp from '@lucide/svelte/icons/calendar-arrow-up';
	import ChevronDown from '@lucide/svelte/icons/chevron-down';
	import Mic from '@lucide/svelte/icons/mic';
	import Pencil from '@lucide/svelte/icons/pencil';
	import Trash2 from '@lucide/svelte/icons/trash-2';
	import { Button } from '$lib/ui/primitives/button';
	import { Input } from '$lib/ui/primitives/input';
	import { Textarea } from '$lib/ui/primitives/textarea';
	import { formatTime } from '$lib/ui/format';
	import { wordsLabel, type DiaryEntryView } from '$lib/app/diary';

	/**
	 * "Your words" on a diary day (B119, B120, B127): collapsed behind a toggle naming the entry
	 * count; expanded, each entry shows its time and words with Edit, Move to another day and Delete
	 * when `onedit`, `onmove` and `ondelete` are given. Each callback resolves with an error sentence or
	 * null. `day` is the entries' day and `today` the latest day an entry may move to.
	 */
	let {
		entries,
		zone,
		day,
		today,
		onedit,
		onmove,
		ondelete
	}: {
		entries: DiaryEntryView[];
		zone: string;
		day: string;
		today?: string;
		onedit?: (id: string, text: string) => Promise<string | null>;
		onmove?: (id: string, to: string) => Promise<string | null>;
		ondelete?: (id: string) => Promise<string | null>;
	} = $props();

	const uid = $props.id();
	let open = $state(false);
	let mode = $state<{ id: string; kind: 'edit' | 'move' | 'delete' } | null>(null);
	let text = $state('');
	let target = $state('');
	let error = $state<string | null>(null);
	let busy = $state(false);

	/** Opens `kind` for entry `e`, or closes it when it is already open. */
	function start(e: DiaryEntryView, kind: 'edit' | 'move' | 'delete') {
		error = null;
		if (mode?.id === e.id && mode.kind === kind) return void (mode = null);
		mode = { id: e.id, kind };
		text = e.text;
		target = day;
	}

	/** Runs `job` with the buttons disabled; closes the entry's panel when it succeeds. */
	async function run(job: () => Promise<string | null> | undefined) {
		busy = true;
		error = (await job()) ?? null;
		busy = false;
		if (!error) mode = null;
	}
</script>

<section class="flex flex-col gap-1" aria-label="Your words">
	<button type="button" aria-expanded={open} aria-controls="{uid}-words" onclick={() => (open = !open)} class="flex min-h-11 items-center gap-2 rounded-sm text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50">
		<span class="flex-1 text-sm font-semibold">{wordsLabel(entries.length)}</span>
		<ChevronDown class="size-4 text-muted transition-transform {open ? 'rotate-180' : ''}" aria-hidden="true" />
	</button>
	{#if open}
		<ol id="{uid}-words" class="flex flex-col divide-y divide-border">
			{#each entries as e (e.id)}
				<li class="flex flex-col gap-2 py-2">
					<div class="flex gap-3">
						<time datetime={e.at} class="flex w-12 shrink-0 items-start gap-1 text-sm text-muted tabular-nums">{formatTime(e.at, zone)}{#if e.source === 'voice'}<Mic class="mt-0.5 size-3" aria-label="Spoken" />{/if}</time>
						<p class="min-w-0 flex-1 text-sm wrap-anywhere">{e.text}</p>
					</div>
					{#if onedit || onmove || ondelete}
						<div class="flex justify-end gap-1">
							{#if onedit}<Button variant="ghost" size="icon-touch" aria-label="Edit entry" aria-pressed={mode?.id === e.id && mode.kind === 'edit'} onclick={() => start(e, 'edit')}><Pencil /></Button>{/if}
							{#if onmove}<Button variant="ghost" size="icon-touch" aria-label="Move to another day" aria-pressed={mode?.id === e.id && mode.kind === 'move'} onclick={() => start(e, 'move')}><CalendarArrowUp /></Button>{/if}
							{#if ondelete}<Button variant="ghost" size="icon-touch" aria-label="Delete entry" aria-pressed={mode?.id === e.id && mode.kind === 'delete'} onclick={() => start(e, 'delete')}><Trash2 /></Button>{/if}
						</div>
					{/if}
					{#if mode?.id === e.id && mode.kind === 'edit'}
						<form class="flex flex-col gap-2" onsubmit={(ev) => (ev.preventDefault(), void run(() => onedit?.(e.id, text)))}>
							<label for="{uid}-edit" class="sr-only">Your words</label>
							<Textarea id="{uid}-edit" bind:value={text} rows={3} maxlength={2000} class="text-lg" />
							<div class="flex justify-end gap-2">
								<Button variant="outline" size="touch" type="button" disabled={busy} onclick={() => (mode = null)}>Cancel</Button>
								<Button size="touch" type="submit" disabled={busy || !text.trim()}>{busy ? 'Saving…' : 'Save'}</Button>
							</div>
						</form>
					{:else if mode?.id === e.id && mode.kind === 'move'}
						<form class="flex flex-col gap-2" onsubmit={(ev) => (ev.preventDefault(), void run(() => onmove?.(e.id, target)))}>
							<label for="{uid}-move" class="text-sm font-semibold">Move to</label>
							<Input id="{uid}-move" type="date" bind:value={target} max={today} class="h-11 text-lg" />
							<div class="flex justify-end gap-2">
								<Button variant="outline" size="touch" type="button" disabled={busy} onclick={() => (mode = null)}>Cancel</Button>
								<Button size="touch" type="submit" disabled={busy || !target || target === day}>{busy ? 'Moving…' : 'Move'}</Button>
							</div>
						</form>
					{:else if mode?.id === e.id && mode.kind === 'delete'}
						<div role="alertdialog" aria-label="Delete entry" class="flex flex-col gap-2 rounded-sm border border-border bg-surface-2 p-3">
							<p class="text-sm">Delete these words? They are removed from this phone and every synced copy.</p>
							<div class="flex justify-end gap-2">
								<Button variant="outline" size="touch" disabled={busy} onclick={() => (mode = null)}>Keep</Button>
								<Button variant="destructive" size="touch" disabled={busy} onclick={() => void run(() => ondelete?.(e.id))}>{busy ? 'Deleting…' : 'Delete'}</Button>
							</div>
						</div>
					{/if}
					{#if error && mode?.id === e.id}<p class="text-sm text-status-missed" role="alert">{error}</p>{/if}
				</li>
			{/each}
		</ol>
	{/if}
</section>
