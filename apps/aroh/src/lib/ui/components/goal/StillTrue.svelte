<script lang="ts">
	import type { Snippet } from 'svelte';
	import Check from '@lucide/svelte/icons/check';
	import { Button } from '$lib/ui/primitives/button';

	/**
	 * A profile fact the intake already knows, asked as "Still true?" instead of asked again:
	 * the question, the known value and where it came from, with Yes (`onyes`) and Edit. Edit
	 * swaps the row for `children`, the same input the question would have used.
	 */
	let { label, value, source, onyes, children }: { label: string; value: string; source: string; onyes?: () => void; children: Snippet } = $props();

	let mode = $state<'ask' | 'yes' | 'edit'>('ask');

	/** Keeps the known value as the answer. */
	function yes() {
		mode = 'yes';
		onyes?.();
	}
</script>

{#if mode === 'edit'}
	<div class="flex flex-col gap-2">
		{@render children()}
		<p class="text-xs text-muted">Saved to your profile when you submit.</p>
	</div>
{:else}
	<div class="flex flex-col gap-2 rounded-sm border border-border bg-bg p-3">
		<div class="flex items-baseline justify-between gap-3">
			<span class="text-sm font-medium">{label}</span>
			<span class="text-xs text-muted">Still true?</span>
		</div>
		<p class="text-lg font-semibold">{value}</p>
		<p class="text-xs text-muted">{source}</p>
		{#if mode === 'yes'}
			<p class="flex items-center gap-2 text-sm text-status-done" role="status"><Check class="size-4" aria-hidden="true" />Still true</p>
		{:else}
			<div class="flex gap-2">
				<Button variant="outline" size="touch" onclick={yes}>Yes</Button>
				<Button variant="ghost" size="touch" onclick={() => (mode = 'edit')}>Edit</Button>
			</div>
		{/if}
	</div>
{/if}
