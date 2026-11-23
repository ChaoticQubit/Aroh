<script lang="ts">
	import * as Sheet from '$lib/ui/primitives/sheet';

	/**
	 * Native-style action sheet that confirms deleting goal `title` (Q68): two red choices,
	 * delete with its actions or keep the actions on the calendar, and a separate Cancel.
	 * `ondelete(keepActions)` runs the chosen delete; Cancel, the scrim and Escape call `onclose`.
	 */
	let {
		title,
		busy = false,
		ondelete,
		onclose
	}: { title: string; busy?: boolean; ondelete: (keepActions: boolean) => void; onclose: () => void } = $props();

	const ROW = 'flex min-h-14 w-full items-center justify-center px-4 text-base outline-none focus-visible:bg-surface disabled:opacity-50';
</script>

<Sheet.Root open={true} onOpenChange={(open) => !open && onclose()}>
	<Sheet.Content side="bottom" showCloseButton={false} class="flex flex-col gap-2 border-none bg-transparent px-2 pb-[max(0.5rem,env(safe-area-inset-bottom))] text-fg shadow-none">
		<div class="flex flex-col divide-y divide-border overflow-hidden rounded-[var(--radius-dialog)] bg-bg">
			<div class="flex flex-col gap-1 px-4 py-3 text-center">
				<Sheet.Title class="text-sm font-medium text-fg">Delete "{title}"?</Sheet.Title>
				<Sheet.Description class="text-xs text-muted">Profile facts Aroh learned from this goal stay. This cannot be undone.</Sheet.Description>
			</div>
			<button type="button" class="{ROW} text-danger" disabled={busy} onclick={() => ondelete(false)}>Delete goal and its actions</button>
			<button type="button" class="{ROW} text-danger" disabled={busy} onclick={() => ondelete(true)}>Delete goal, keep actions</button>
		</div>
		<button type="button" class="{ROW} rounded-[var(--radius-dialog)] bg-bg font-medium" disabled={busy} onclick={onclose}>Cancel</button>
	</Sheet.Content>
</Sheet.Root>
