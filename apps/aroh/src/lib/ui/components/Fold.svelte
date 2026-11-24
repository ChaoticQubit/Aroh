<script lang="ts">
	import ChevronDown from '@lucide/svelte/icons/chevron-down';
	import type { Snippet } from 'svelte';

	/**
	 * A collapsible goal-page card: a head with `title`, a one-line `summary` shown while closed,
	 * an optional `badge` snippet and a chevron; `children` render below when open. `open` is the
	 * default state and stays bindable so the page can expand every card for the long-data frame.
	 */
	let { title, summary = '', open = $bindable(false), badge, children }: { title: string; summary?: string; open?: boolean; badge?: Snippet; children: Snippet } = $props();
	const id = $props.id();
</script>

<section class="rounded-sm border border-border bg-bg" aria-labelledby="{id}-head">
	<h3 class="m-0">
		<button
			type="button"
			id="{id}-head"
			aria-expanded={open}
			aria-controls="{id}-body"
			class="flex min-h-13 w-full items-center gap-3 px-4 py-2.5 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset"
			onclick={() => (open = !open)}
		>
			<span class="flex min-w-0 flex-1 flex-col">
				<span class="flex items-center gap-2 text-base font-semibold">{title}{@render badge?.()}</span>
				{#if summary && !open}<span class="truncate text-sm text-muted">{summary}</span>{/if}
			</span>
			<ChevronDown class="size-4 shrink-0 text-muted transition-transform {open ? 'rotate-180' : ''}" aria-hidden="true" />
		</button>
	</h3>
	{#if open}
		<div id="{id}-body" class="flex flex-col gap-3 border-t border-border px-4 pt-3 pb-4">{@render children()}</div>
	{/if}
</section>
