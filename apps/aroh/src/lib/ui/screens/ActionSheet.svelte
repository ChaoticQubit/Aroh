<script lang="ts">
	import * as Sheet from '$lib/ui/primitives/sheet';
	import ActionEditor from '../components/planning/ActionEditor.svelte';
	import type { ActionView } from '$lib/ui/contract';
	import type { ActionEdit } from './types';

	/**
	 * Bottom sheet with the action editor on the sheet primitive: scrim, hairline top edge, grab
	 * bar, focus trap. Tapping the scrim or pressing Escape calls `onclose`. `inline` draws the same
	 * sheet in place, without portal, scroll lock or focus trap, for the `/mockups` frames.
	 */
	let {
		action,
		confirming = false,
		inline = false,
		onsave,
		ondelete,
		onclose
	}: { action: ActionView; confirming?: boolean; inline?: boolean; onsave?: (edit: ActionEdit) => void; ondelete?: (id: string) => void; onclose?: () => void } = $props();

	const PANEL = 'flex flex-col gap-3 rounded-t-[var(--radius-dialog)] border-t border-border bg-bg px-4 pt-2 pb-[max(1rem,env(safe-area-inset-bottom))]';
</script>

{#snippet body()}
	<span class="mx-auto h-1 w-9 rounded-full bg-border-strong" aria-hidden="true"></span>
	<ActionEditor {action} {confirming} {onsave} {ondelete} {onclose} />
{/snippet}

{#if inline}
	<div class="fixed inset-0 z-40 flex flex-col justify-end">
		<button type="button" class="flex-1 bg-scrim outline-none" aria-label="Close" tabindex="-1" onclick={() => onclose?.()}></button>
		<div role="dialog" aria-label="Edit {action.title}" class={PANEL}>{@render body()}</div>
	</div>
{:else}
	<Sheet.Root open={true} onOpenChange={(open) => !open && onclose?.()}>
		<Sheet.Content side="bottom" showCloseButton={false} aria-describedby={undefined} class="{PANEL} text-fg shadow-none">
			<Sheet.Title class="sr-only">Edit {action.title}</Sheet.Title>
			{@render body()}
		</Sheet.Content>
	</Sheet.Root>
{/if}
