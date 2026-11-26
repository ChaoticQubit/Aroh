<script lang="ts">
	import * as Sheet from '$lib/ui/primitives/sheet';
	import ActionEditor from '../components/planning/ActionEditor.svelte';
	import type { ActionView, DeleteScope } from '$lib/ui/contract';
	import type { ActionEdit, GoalChoice, NewAction } from './types';

	/**
	 * Bottom sheet with the action editor on the sheet primitive: scrim, hairline top edge, grab
	 * bar, focus trap. Tapping the scrim or pressing Escape calls `onclose`. `inline` draws the same
	 * sheet in place, without portal, scroll lock or focus trap, for the `/mockups` frames. With
	 * `oncreate` the same sheet adds a new item from a Calendar slot (B121).
	 */
	let {
		action,
		confirming = false,
		inline = false,
		onstatus,
		onsave,
		onlock,
		ondelete,
		goals = [],
		oncreate,
		onclose
	}: {
		action: ActionView;
		confirming?: boolean;
		inline?: boolean;
		onstatus?: (id: string, status: 'planned' | 'done' | 'skipped') => void;
		onsave?: (edit: ActionEdit) => void;
		onlock?: (id: string, locked: boolean) => void;
		ondelete?: (id: string, scope: DeleteScope) => void;
		goals?: GoalChoice[];
		oncreate?: (item: NewAction) => void;
		onclose?: () => void;
	} = $props();

	const PANEL = 'flex flex-col gap-3 rounded-t-[var(--radius-dialog)] border-t border-border bg-bg pt-2 pr-[max(1rem,env(safe-area-inset-right))] pb-[max(1rem,env(safe-area-inset-bottom))] pl-[max(1rem,env(safe-area-inset-left))]';
</script>

{#snippet body()}
	<span class="mx-auto h-1 w-9 rounded-full bg-border-strong" aria-hidden="true"></span>
	<ActionEditor {action} {confirming} {goals} {onstatus} {onsave} {onlock} {ondelete} {oncreate} {onclose} />
{/snippet}

{#if inline}
	<div class="fixed inset-0 z-40 flex flex-col justify-end">
		<button type="button" class="flex-1 bg-scrim outline-none" aria-label="Close" tabindex="-1" onclick={() => onclose?.()}></button>
		<div role="dialog" aria-label={oncreate ? 'New item' : `Edit ${action.title}`} class={PANEL}>{@render body()}</div>
	</div>
{:else}
	<Sheet.Root open={true} onOpenChange={(open) => !open && onclose?.()}>
		<Sheet.Content side="bottom" showCloseButton={false} aria-describedby={undefined} class="{PANEL} text-fg shadow-none">
			<Sheet.Title class="sr-only">{oncreate ? 'New item' : `Edit ${action.title}`}</Sheet.Title>
			{@render body()}
		</Sheet.Content>
	</Sheet.Root>
{/if}
