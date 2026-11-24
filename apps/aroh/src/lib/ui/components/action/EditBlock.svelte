<script lang="ts">
	import GripVertical from '@lucide/svelte/icons/grip-vertical';
	import Plus from '@lucide/svelte/icons/plus';
	import { Button } from '$lib/ui/primitives/button';
	import type { Card } from '$lib/ui/action/card';
	import { addRow, moveRow, removeRow, setListCell, setText, setWorkoutCell, type ListField, type WorkoutField } from '$lib/ui/action/edit';
	import { grip, swipeRemove } from '$lib/ui/action/rows';

	/**
	 * One block of an action card edited in place like a note (Q71): tap a cell to type, + Add a row,
	 * drag a row by its grip (or ArrowUp and ArrowDown on it), swipe a row left to delete it.
	 * Every change calls `onchange` with the whole new card; the page saves as you type.
	 */
	let { card, index, onchange }: { card: Card; index: number; onchange: (card: Card) => void } = $props();

	const block = $derived(card.blocks[index]);
	const id = $props.id();
	const field = 'h-11 min-w-0 rounded-sm border border-transparent bg-transparent px-2 text-lg outline-none hover:border-border focus-visible:border-ring focus-visible:bg-bg focus-visible:ring-3 focus-visible:ring-ring/50';
	const numberField = `${field} w-full text-right tabular-nums`;
	const textField = 'block min-h-11 w-full resize-none rounded-sm border border-transparent bg-transparent px-2 py-2.5 text-lg leading-6 break-words outline-none hover:border-border focus-visible:border-ring focus-visible:bg-bg focus-visible:ring-3 focus-visible:ring-ring/50';
	const gripClass = 'flex size-11 shrink-0 touch-none items-center justify-center rounded-sm text-muted outline-none focus-visible:ring-3 focus-visible:ring-ring/50';

	const cell = (row: number, f: WorkoutField) => (e: Event) => onchange(setWorkoutCell(card, index, row, f, (e.currentTarget as HTMLInputElement).value));
	const listCell = (row: number, f: ListField) => (e: Event) => onchange(setListCell(card, index, row, f, (e.currentTarget as HTMLInputElement | HTMLTextAreaElement).value));
	const move = (from: number, to: number) => onchange(moveRow(card, index, from, to));
	const remove = (row: number) => () => onchange(removeRow(card, index, row));
	const blurOnEnter = (e: KeyboardEvent) => {
		if (e.key !== 'Enter') return;
		e.preventDefault();
		(e.currentTarget as HTMLElement).blur();
	};
	/** Grows textarea `node` to its wrapped text, on mount, on each new `text` and when its width changes. */
	function grow(node: HTMLTextAreaElement, text: string) {
		const fit = () => {
			node.style.height = 'auto';
			node.style.height = `${node.scrollHeight + node.offsetHeight - node.clientHeight}px`;
		};
		fit();
		let width = node.clientWidth;
		const seen = new ResizeObserver(() => {
			if (node.clientWidth === width) return;
			width = node.clientWidth;
			fit();
		});
		seen.observe(node);
		return {
			update(next: string) {
				if (next === text) return;
				text = next;
				fit();
			},
			destroy: () => seen.disconnect()
		};
	}
	const label = $derived(block.kind === 'workout' ? 'exercise' : block.kind === 'steps' ? 'step' : 'item');
</script>

{#if block.kind !== 'workout'}
	<label class="sr-only" for="{id}-title">Block title</label>
	<input id="{id}-title" class="{field} -mx-2 mb-1 font-semibold" value={block.title} placeholder="Title" oninput={(e) => onchange(setText(card, { title: index }, e.currentTarget.value))} />
{/if}
<ol class="flex flex-col divide-y divide-border">
	{#if block.kind === 'workout'}
		{#each block.exercises as e, row (row)}
			{@const reps = e.sets[0].reps}
			<li data-row class="flex flex-col gap-1 bg-surface py-2" use:swipeRemove={remove(row)}>
				<div class="flex items-center gap-1">
					<button type="button" data-grip class={gripClass} aria-label="Move {e.name}, drag or use the arrow keys" use:grip={{ index: row, onmove: move }}><GripVertical class="size-4" /></button>
					<label class="sr-only" for="{id}-name-{row}">Exercise name</label>
					<input id="{id}-name-{row}" class="{field} flex-1 font-medium" value={e.name} oninput={cell(row, 'name')} />
				</div>
				<div class="grid grid-cols-4 gap-1 pl-12">
					{#each [['sets', 'Sets', e.sets.length], ['reps', 'Reps', reps], ['weight', e.unit ?? 'Load', e.sets[0].weight ?? ''], ['rest', 'Rest s', e.rest_s]] as const as [f, text, value] (f)}
						<label class="flex min-w-0 flex-col text-xs text-muted">
							<span class="px-2">{text}</span>
							<input class={numberField} inputmode="decimal" enterkeyhint="done" {value} placeholder={f === 'weight' ? 'BW' : ''} oninput={cell(row, f)} />
						</label>
					{/each}
				</div>
			</li>
		{/each}
	{:else}
		{#each block.items as item, row (row)}
			<li data-row class="flex items-start gap-1 bg-surface py-1" use:swipeRemove={remove(row)}>
				<button type="button" data-grip class={gripClass} aria-label="Move {label} {row + 1}, drag or use the arrow keys" use:grip={{ index: row, onmove: move }}><GripVertical class="size-4" /></button>
				<label class="sr-only" for="{id}-text-{row}">{label} {row + 1}</label>
				<div class="min-w-0 flex-1"><textarea id="{id}-text-{row}" class={textField} rows="1" enterkeyhint="done" use:grow={item.text} value={item.text} placeholder="Type a {label}" onkeydown={blurOnEnter} oninput={listCell(row, 'text')}></textarea></div>
				{#if block.kind === 'steps'}
					<label class="flex w-16 shrink-0 items-center text-xs text-muted">
						<input class={numberField} inputmode="numeric" enterkeyhint="done" value={'minutes' in item ? (item.minutes ?? '') : ''} aria-label="Minutes of step {row + 1}" oninput={listCell(row, 'minutes')} />min
					</label>
				{/if}
			</li>
		{/each}
	{/if}
</ol>
<Button variant="ghost" size="touch" class="mt-1 self-start" onclick={() => onchange(addRow(card, index))}><Plus />Add {label}</Button>
<p class="sr-only">Swipe a row left to delete it.</p>
