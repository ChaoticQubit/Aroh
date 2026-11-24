<script lang="ts">
	import Lightbulb from '@lucide/svelte/icons/lightbulb';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import Persona from './Persona.svelte';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/** An observation from a persona, read-only, with a dismiss action. Emits "dismiss". */
	let { spec, onchange }: { spec: SpecOf<'insight_card'>; onchange?: OnChange } = $props();

	let dismissed = $state(false);
</script>

{#if !dismissed}
	<Card.Root size="sm" class="gap-2 px-4">
		<div class="flex items-center justify-between gap-2">
			<Persona name={spec.persona} note="Insight" />
			<Lightbulb class="size-4 text-muted" aria-hidden="true" />
		</div>
		<p class="text-sm font-semibold text-fg">{spec.title}</p>
		<p class="text-sm text-muted">{spec.body}</p>
		<div class="flex">
			<Button
				variant="outline"
				size="touch"
				class="@3xl:h-8"
				onclick={() => {
					dismissed = true;
					onchange?.('dismiss');
				}}>Dismiss</Button
			>
		</div>
	</Card.Root>
{/if}
