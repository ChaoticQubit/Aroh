<script lang="ts">
	import { Button } from '$lib/ui/primitives/button';
	import { DOT } from './StatusChip.svelte';
	import type { Phase } from '$lib/app/replan';

	/** One-line status of the owner's latest edit (Q77): dot, text, then Retry, Undo or Inbox, and close. */
	let { phase, onretry, onundo, ondismiss }: { phase: Phase; onretry: () => void; onundo: () => void; ondismiss: () => void } = $props();
	const retry = $derived(phase.kind === 'not_started' || phase.kind === 'failed');
</script>

<div role="status" aria-live="polite" data-testid="replan-status" data-phase={phase.kind} class="flex min-w-0 items-start gap-2 rounded-md border border-border bg-surface px-3 py-2">
	<span class="mt-1.5 h-2 w-2 shrink-0 rounded-full {DOT[phase.tone]} {phase.kind === 'replanning' || phase.kind === 'saved' ? 'animate-pulse' : ''}" aria-hidden="true"></span>
	<p class="min-w-0 flex-1 text-sm leading-5 break-words text-fg tabular-nums">{phase.text}</p>
	{#if retry}<Button size="xs" variant="outline" onclick={onretry}>Retry</Button>{/if}
	{#if phase.kind === 'updated' && phase.undo}<Button size="xs" variant="outline" onclick={onundo}>Undo</Button>{/if}
	{#if phase.kind === 'needs_you'}<Button size="xs" variant="outline" href="/inbox">Inbox</Button>{/if}
	<button type="button" class="-mr-1 shrink-0 px-1 text-sm leading-5 text-muted" aria-label="Dismiss status" onclick={ondismiss}>✕</button>
</div>
