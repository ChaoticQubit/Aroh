<script lang="ts">
	import * as Sheet from '$lib/ui/primitives/sheet';

	/**
	 * Native-style action sheet that starts Run again for goal `title` (Q74): keep the saved
	 * answers (`onkeep`) or edit them first (`onedit`, offered when `answers` is true), and a
	 * separate Cancel. Cancel, the scrim and Escape call `onclose`.
	 */
	let {
		title,
		answers,
		busy = false,
		onkeep,
		onedit,
		onclose
	}: { title: string; answers: boolean; busy?: boolean; onkeep: () => void; onedit: () => void; onclose: () => void } = $props();

	const ROW = 'flex min-h-14 w-full items-center justify-center px-4 text-base outline-none focus-visible:bg-surface disabled:opacity-50';
</script>

<Sheet.Root open={true} onOpenChange={(open) => !open && onclose()}>
	<Sheet.Content side="bottom" showCloseButton={false} class="flex flex-col gap-2 border-none bg-transparent px-2 pb-[max(0.5rem,env(safe-area-inset-bottom))] text-fg shadow-none">
		<div class="flex flex-col divide-y divide-border overflow-hidden rounded-[var(--radius-dialog)] bg-bg">
			<div class="flex flex-col gap-1 px-4 py-3 text-center">
				<Sheet.Title class="text-sm font-medium text-fg">Run "{title}" again?</Sheet.Title>
				<Sheet.Description class="text-xs text-muted">Fresh research and a new plan. Your calendar changes only when you approve it; done sessions stay.</Sheet.Description>
			</div>
			<button type="button" class={ROW} disabled={busy} onclick={onkeep}>{answers ? 'Keep my answers' : 'Run again'}</button>
			{#if answers}<button type="button" class={ROW} disabled={busy} onclick={onedit}>Edit answers first</button>{/if}
		</div>
		<button type="button" class="{ROW} rounded-[var(--radius-dialog)] bg-bg font-medium" disabled={busy} onclick={onclose}>Cancel</button>
	</Sheet.Content>
</Sheet.Root>
