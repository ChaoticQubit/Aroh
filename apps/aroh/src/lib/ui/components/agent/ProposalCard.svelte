<script lang="ts">
	import ArrowRight from '@lucide/svelte/icons/arrow-right';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import Persona from './Persona.svelte';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/**
	 * A persona's proposed plan change, each change shown as before -> after. Emits "apply" or
	 * "reject" (Dismiss). Given `decided`, the caller keeps the decision (null: none yet, so a
	 * refused one makes the card decidable again); without it the card keeps its own.
	 */
	let { spec, onchange, decided: given }: { spec: SpecOf<'proposal_card'>; onchange?: OnChange; decided?: 'apply' | 'reject' | null } = $props();

	const OP = { move: 'Move', resize: 'Resize', create: 'Add', delete: 'Remove' } as const;
	const EDGE = { move: 'border-l-status-planned', resize: 'border-l-status-planned', create: 'border-l-status-done', delete: 'border-l-status-missed' } as const;
	const INK = { move: 'text-status-planned', resize: 'text-status-planned', create: 'text-status-done', delete: 'text-status-missed' } as const;
	let picked = $state<'apply' | 'reject' | null>(null);
	const decided = $derived(given === undefined ? picked : given);

	/** Records the decision and reports it. */
	function decide(choice: 'apply' | 'reject') {
		picked = choice;
		onchange?.(choice);
	}
</script>

<Card.Root size="sm" class="gap-3 px-4">
	<Persona name={spec.persona} note={spec.changes.length ? 'Proposal' : 'Suggestion'} />
	<p class="text-sm text-fg">{spec.summary}</p>
	{#if spec.changes.length}
		<ul class="flex flex-col divide-y divide-border border-y border-border">
			{#each spec.changes as c (c.op + c.title + c.before)}
				<li class="flex flex-col gap-0.5 border-l-3 py-2 pl-3 {EDGE[c.op]}">
					<span class="text-xs text-muted"><span class="font-medium {INK[c.op]}">{OP[c.op]}</span> · {c.title}</span>
					<span class="flex flex-wrap items-center gap-1.5 text-sm tabular-nums">
						{#if c.before}<span class="line-through {c.op === 'delete' ? INK.delete : 'text-muted'}">{c.before}</span>{/if}
						{#if c.before && c.after}<ArrowRight class="size-3.5 text-muted" aria-label="to" />{/if}
						{#if c.after}<span class="font-medium {c.op === 'create' ? INK.create : 'text-fg'}">{c.after}</span>{/if}
					</span>
				</li>
			{/each}
		</ul>
	{/if}
	{#if decided}
		<p class="text-sm text-muted" role="status">{decided === 'apply' ? 'Applied. You can undo this from the calendar.' : 'Dismissed. The plan stays as it was.'}</p>
	{:else}
		<div class="flex gap-2">
			<Button size="touch" class="@3xl:h-8" onclick={() => decide('apply')}>Apply</Button>
			<Button size="touch" variant="outline" class="@3xl:h-8" onclick={() => decide('reject')}>Dismiss</Button>
		</div>
	{/if}
</Card.Root>
