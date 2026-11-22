<script lang="ts">
	import type { CategoryView } from '../screens/types';

	/**
	 * Category chips: one toggle per category with its colour dot.
	 * A pressed chip shows that category's items; an unpressed one hides them.
	 * `hidden` holds the colour slots switched off and is bindable.
	 */
	let { categories, hidden = $bindable([]) }: { categories: CategoryView[]; hidden?: number[] } = $props();
</script>

<div class="-mx-4 flex gap-2 overflow-x-auto px-4 @3xl:mx-0 @3xl:px-0" role="group" aria-label="Filter by category">
	{#each categories as c (c.slot)}
		{@const on = !hidden.includes(c.slot)}
		<button
			type="button"
			aria-pressed={on}
			class="flex h-11 shrink-0 items-center gap-2 rounded-sm border px-2.5 text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50 @3xl:h-8 {on ? 'border-border bg-bg text-fg' : 'border-border bg-surface text-muted'}"
			onclick={() => (hidden = on ? [...hidden, c.slot] : hidden.filter((h) => h !== c.slot))}
		>
			<span class="size-2 rounded-full {on ? '' : 'border'}" style={on ? `background: var(--cat-${c.slot})` : `border-color: var(--cat-${c.slot})`}></span>{c.name}
		</button>
	{/each}
</div>
