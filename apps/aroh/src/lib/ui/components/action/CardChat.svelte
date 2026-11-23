<script lang="ts" module>
	import type { DiffLine } from '$lib/ui/action/card';

	/** One entry of a card chat: a message from the owner or the coach, or an applied edit with its diff. */
	export type CardChatEntry = { kind: 'owner'; text: string } | { kind: 'coach'; text: string } | { kind: 'applied'; scope: string; lines: DiffLine[]; undone: boolean };
</script>

<script lang="ts">
	import Minus from '@lucide/svelte/icons/minus';
	import Plus from '@lucide/svelte/icons/plus';
	import ArrowRight from '@lucide/svelte/icons/arrow-right';
	import Undo2 from '@lucide/svelte/icons/undo-2';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import Persona from '../agent/Persona.svelte';
	import CardComposer from './CardComposer.svelte';

	/**
	 * Chat of one action card (Q71): owner and coach messages, the applied edit as a diff card with
	 * Undo, and the composer with the mic. Owner edits apply at once; `onundo` reverts entry `i`,
	 * `onsend` sends a new message. `coach` names the persona of the goal.
	 */
	let { coach, entries, onsend, onundo }: { coach: string; entries: CardChatEntry[]; onsend?: (text: string) => void; onundo?: (i: number) => void } = $props();

	const mark = { add: 'text-status-done', remove: 'text-status-missed', change: 'text-muted' } as const;
</script>

<div class="flex h-full min-h-0 flex-col">
	<ol class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4" aria-label="Card chat">
		{#each entries as e, i (i)}
			<li class={e.kind === 'owner' ? 'flex justify-end' : 'flex flex-col gap-1'}>
				{#if e.kind === 'owner'}
					<p class="max-w-[85%] rounded-md bg-fg px-3 py-2 text-md text-bg">{e.text}</p>
				{:else if e.kind === 'coach'}
					<Persona name={coach} />
					<p class="max-w-[92%] text-md">{e.text}</p>
				{:else}
					<Card.Root size="sm" class="gap-2 px-4">
						<div class="flex items-center justify-between gap-2">
							<Persona name={coach} note={e.undone ? 'Undone' : 'Changed the card'} />
							<span class="text-xs text-muted">{e.scope}</span>
						</div>
						<ul class="flex flex-col gap-1.5 {e.undone ? 'opacity-50' : ''}">
							{#each e.lines as l, j (j)}
								<li class="flex items-start gap-2 text-sm">
									<span class="mt-0.5 shrink-0 {mark[l.op]}" aria-label={l.op === 'add' ? 'Added' : l.op === 'remove' ? 'Removed' : 'Changed'}>
										{#if l.op === 'add'}<Plus class="size-4" />{:else if l.op === 'remove'}<Minus class="size-4" />{:else}<ArrowRight class="size-4" />{/if}
									</span>
									<span class={l.op === 'remove' ? 'text-muted line-through' : ''}>{l.text}</span>
								</li>
							{/each}
						</ul>
						{#if !e.undone}
							<div class="flex items-center justify-between gap-2 border-t border-border pt-2">
								<p class="text-xs text-muted">Applied. The card and calendar are updated.</p>
								<Button variant="outline" size="touch" onclick={() => onundo?.(i)}><Undo2 />Undo</Button>
							</div>
						{/if}
					</Card.Root>
				{/if}
			</li>
		{/each}
	</ol>
	<CardComposer placeholder="Ask {coach.toLowerCase()} to change this" {onsend} />
</div>
