<script lang="ts" generics="T">
	import type { Snippet } from 'svelte';
	import type { MenuState } from './extensions';

	/**
	 * A suggestion list under the caret (above it when the keyboard leaves no room): `menu` is the
	 * open state or null, `row` draws one item. Taps never blur the editor, so the keyboard stays; a
	 * tap picks on pointer up because WebKit sends no click after a held pointer down.
	 */
	let { menu, label, row }: { menu: MenuState<T> | null; label: string; row: Snippet<[T]> } = $props();

	const GAP = 6;
	const WANT = 260;
	const place = $derived.by(() => {
		const r = menu?.rect;
		if (!r || typeof window === 'undefined') return 'left: 1rem; right: 1rem; bottom: 1rem';
		const height = window.visualViewport?.height ?? window.innerHeight;
		const width = window.innerWidth;
		const left = Math.max(8, Math.min(r.left, width - 296));
		const below = height - r.bottom - GAP;
		return below >= Math.min(WANT, r.top) ? `left: ${left}px; top: ${r.bottom + GAP}px; max-height: ${Math.max(below - 8, 120)}px` : `left: ${left}px; bottom: ${height - r.top + GAP}px; max-height: ${Math.max(r.top - GAP - 8, 120)}px`;
	});
</script>

{#if menu && menu.items.length}
	<div role="listbox" aria-label={label} class="fixed z-50 flex w-72 max-w-[calc(100vw-1rem)] flex-col overflow-y-auto rounded-md border border-border bg-bg py-1 shadow-md" style={place}>
		{#each menu.items as item, i (i)}
			<button
				type="button"
				role="option"
				aria-selected={i === menu.index}
				class="flex min-h-11 w-full items-center gap-3 px-3 text-left text-md outline-none {i === menu.index ? 'bg-surface-2' : ''} hover:bg-surface-2"
				onpointerdown={(e) => e.preventDefault()}
				onpointerup={() => menu?.pick(item)}
				onclick={(e) => e.detail === 0 && menu?.pick(item)}
			>
				{@render row(item)}
			</button>
		{/each}
	</div>
{/if}
