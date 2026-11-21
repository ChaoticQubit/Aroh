<script lang="ts">
	import Check from '@lucide/svelte/icons/check';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import Render from '../Render.svelte';
	import Persona from './Persona.svelte';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/** A persona's question with its answer input rendered from the nested spec. Emits the answer on send. */
	let { spec, onchange }: { spec: SpecOf<'question_card'>; onchange?: OnChange } = $props();

	let answer = $state<unknown>(undefined);
	let sent = $state(false);
</script>

<Card.Root size="sm" class="gap-3 px-4">
	<Persona name={spec.persona} note="Question" />
	<p class="text-sm font-semibold text-fg">{spec.question}</p>
	{#if sent}
		<p class="flex items-center gap-2 text-sm text-status-done" role="status"><Check class="size-4" />Answer sent</p>
	{:else}
		<Render spec={spec.input} onchange={(v) => (answer = v)} />
		<div class="flex">
			<Button
				size="touch"
				class="@3xl:h-8"
				onclick={() => {
					sent = true;
					onchange?.(answer);
				}}>Send answer</Button
			>
		</div>
	{/if}
</Card.Root>
