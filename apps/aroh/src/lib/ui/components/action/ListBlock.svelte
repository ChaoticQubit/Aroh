<script lang="ts">
	import Check from '@lucide/svelte/icons/check';
	import type { Card, CardLog } from '$lib/ui/action/card';

	type List = Extract<Card['blocks'][number], { kind: 'steps' | 'checklist' }>;

	/**
	 * Steps or checklist block of an action card (Q71). Steps are numbered with their minutes;
	 * checklist rows are 44 px tick rows read from and written to the session `log` via `ontoggle`.
	 */
	let { block, index, log, ontoggle }: { block: List; index: number; log: CardLog; ontoggle?: (row: number) => void } = $props();

	const isChecked = (row: number) => log.checked.some((c) => c.block === index && c.row === row);
</script>

{#if block.kind === 'steps'}
	<ol class="flex flex-col gap-3">
		{#each block.items as item, row (row)}
			<li class="grid grid-cols-[1.75rem_1fr_auto] items-baseline gap-2 text-sm">
				<span class="flex size-6 items-center justify-center rounded-full bg-surface-2 text-xs font-semibold tabular-nums">{row + 1}</span>
				<span>{item.text}</span>
				{#if item.minutes !== null}<span class="text-xs text-muted tabular-nums">{item.minutes} min</span>{/if}
			</li>
		{/each}
	</ol>
{:else}
	<ul class="-my-1 flex flex-col">
		{#each block.items as item, row (row)}
			{@const on = isChecked(row)}
			<li>
				<button type="button" class="flex min-h-11 w-full items-center gap-3 rounded-sm text-left text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50" aria-pressed={on} onclick={() => ontoggle?.(row)}>
					<span class="flex size-5 shrink-0 items-center justify-center rounded-sm border {on ? 'border-fg bg-fg text-bg' : 'border-border-strong'}">{#if on}<Check class="size-3.5" strokeWidth={3} />{/if}</span>
					<span class={on ? 'text-muted line-through' : ''}>{item.text}</span>
				</button>
			</li>
		{/each}
	</ul>
{/if}
