<script lang="ts">
	import Check from '@lucide/svelte/icons/check';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import Render from '../Render.svelte';
	import Persona from './Persona.svelte';
	import type { ChatMessage, OnChange } from '$lib/ui/contract';

	/**
	 * Every open question of a run as one card: each with its input and a Skip, a count line, and
	 * one Send answers button enabled once every question has an answer or a skip. Emits
	 * `{ type: 'answer', id, value }`, `{ type: 'skip', id, skipped }` and `{ type: 'submit' }`.
	 * Once submitted it lists what was sent.
	 */
	let { group, onchange }: { group: Extract<ChatMessage, { type: 'questions' }>; onchange?: OnChange } = $props();
</script>

<Card.Root size="sm" class="gap-4 px-4" aria-label="Questions">
	<Persona name={group.persona} note={group.submitted ? 'Answers sent' : `${group.items.length} questions`} />
	<p class="text-sm text-fg">{group.prompt}</p>
	{#if group.submitted}
		<ul class="flex flex-col gap-2 text-sm">
			{#each group.items as item (item.id)}
				<li class="flex flex-col">
					<span class="text-muted">{item.text}</span>
					<span class="text-fg">{group.skipped.includes(item.id) ? 'Skipped' : group.values[item.id]}</span>
				</li>
			{/each}
		</ul>
		<p class="flex items-center gap-2 text-sm text-status-done" role="status"><Check class="size-4" />Answers sent</p>
	{:else}
		<ol class="flex flex-col gap-4">
			{#each group.items as item, i (item.id)}
				{@const skipped = group.skipped.includes(item.id)}
				<li class="flex flex-col gap-2 border-t border-border pt-3 first:border-t-0 first:pt-0">
					<div class="flex items-start justify-between gap-2">
						<p class="text-sm font-semibold text-fg"><span class="text-muted tabular-nums">{i + 1}.</span> {item.text}</p>
						<Button size="sm" variant="ghost" aria-pressed={skipped} aria-label="{skipped ? 'Answer' : 'Skip'} question {i + 1}" onclick={() => onchange?.({ type: 'skip', id: item.id, skipped: !skipped })}
							>{skipped ? 'Answer' : 'Skip'}</Button
						>
					</div>
					{#if skipped}
						<p class="text-sm text-muted">Skipped. The planner picks a sensible default.</p>
					{:else}
						<Render spec={item.input} onchange={(value) => onchange?.({ type: 'answer', id: item.id, value })} />
					{/if}
				</li>
			{/each}
		</ol>
		<p class="text-sm text-muted" role="status">{group.status}</p>
		{#if group.error}<p class="text-sm text-status-missed" role="alert">{group.error}</p>{/if}
		<div class="flex">
			<Button size="touch" class="@3xl:h-8" disabled={group.missing.length > 0 || group.sending} onclick={() => onchange?.({ type: 'submit' })}>
				{group.sending ? 'Sending' : 'Send answers'}
			</Button>
		</div>
	{/if}
</Card.Root>
