<script lang="ts">
	import { Button } from '$lib/ui/primitives/button';
	import { DOT } from './StatusChip.svelte';

	/**
	 * The catch-up question after a late done (Q78): the question, what one tap removes, then
	 * Remove and Keep; or, once answered, a one-line note with a close button.
	 */
	let {
		offer,
		note = null,
		onremove,
		onkeep
	}: { offer: { text: string; detail: string } | null; note?: string | null; onremove: () => void; onkeep: () => void } = $props();
</script>

{#if offer}
	<div role="alertdialog" aria-label="Catch-up" data-testid="catch-up" class="flex flex-col gap-2 rounded-md border border-border bg-surface px-3 py-2.5">
		<div class="flex items-start gap-2">
			<span class="mt-1.5 h-2 w-2 shrink-0 rounded-full {DOT.warn}" aria-hidden="true"></span>
			<div class="flex min-w-0 flex-1 flex-col gap-0.5">
				<p class="text-sm leading-5 font-medium break-words text-fg">{offer.text}</p>
				<p class="text-xs leading-4 break-words text-muted">{offer.detail}</p>
			</div>
		</div>
		<div class="flex justify-end gap-2">
			<Button size="touch" variant="outline" onclick={onkeep}>Keep</Button>
			<Button size="touch" onclick={onremove}>Remove</Button>
		</div>
	</div>
{:else if note}
	<div role="status" aria-live="polite" data-testid="catch-up-note" class="flex min-w-0 items-start gap-2 rounded-md border border-border bg-surface px-3 py-2">
		<p class="min-w-0 flex-1 text-sm leading-5 break-words text-fg">{note}</p>
		<button type="button" class="-mr-1 shrink-0 px-1 text-sm leading-5 text-muted" aria-label="Dismiss" onclick={onkeep}>✕</button>
	</div>
{/if}
