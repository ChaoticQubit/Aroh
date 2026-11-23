<script lang="ts">
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import { Input } from '$lib/ui/primitives/input';
	import * as Select from '$lib/ui/primitives/select';
	import Field from '../Field.svelte';
	import Persona from '../agent/Persona.svelte';
	import PlaceMap from './PlaceMap.svelte';
	import type { PlacePromptView } from './types';

	/**
	 * Asks whether a place seen on repeated visits is what Aroh guesses, and links it to a goal or action.
	 * `ondecide` gets "link" with the target, "rename" with the owner's name, or "ignore".
	 * Nothing is named or watched until the owner answers.
	 */
	let {
		prompt,
		ondecide
	}: {
		prompt: PlacePromptView;
		ondecide?: (decision: { kind: 'link'; target: string } | { kind: 'rename'; name: string } | { kind: 'ignore' }) => void;
	} = $props();

	const id = $props.id();
	let target = $derived(prompt.targets[0]?.value ?? '');
	const picked = $derived(prompt.targets.find((t) => t.value === target));
	let step = $state<'ask' | 'rename' | 'linked' | 'renamed' | 'ignored'>('ask');
	let name = $state('');

	/** Records the answer, moves the card to its closing line and reports it. */
	function decide(kind: 'link' | 'rename' | 'ignore') {
		if (kind === 'link') {
			step = 'linked';
			ondecide?.({ kind, target });
		} else if (kind === 'rename') {
			if (!name.trim()) return;
			step = 'renamed';
			ondecide?.({ kind, name: name.trim() });
		} else {
			step = 'ignored';
			ondecide?.({ kind });
		}
	}
</script>

<Card.Root size="sm" class="gap-3 px-4">
	<Persona name="Aroh" note="New place" />
	<p class="text-lg font-semibold">{prompt.question ?? `Is this your ${prompt.guess}?`}</p>
	<PlaceMap label="Map of {prompt.name}" pins={[{ x: 190, y: 100, name: prompt.name }]} ring height={120} />
	<div class="flex flex-col gap-0.5">
		<p class="text-sm font-medium">{prompt.name}</p>
		<p class="text-xs text-muted">{prompt.address}</p>
		<p class="text-sm text-muted">You were here {prompt.visits} times in {prompt.weeks} weeks, {prompt.usual}.</p>
	</div>
	{#if step === 'ask'}
		<Field label="Link to" labelId="{id}-target">
			<Select.Root type="single" bind:value={target}>
				<Select.Trigger class="h-11! w-full text-sm" aria-labelledby="{id}-target">{picked?.label ?? 'Pick a goal or action'}</Select.Trigger>
				<Select.Content>
					{#each prompt.targets as t (t.value)}<Select.Item value={t.value} label={t.label} class="min-h-11">{t.label}</Select.Item>{/each}
				</Select.Content>
			</Select.Root>
		</Field>
		<div class="flex flex-wrap gap-2">
			<Button size="touch" onclick={() => decide('link')}>Yes, link it</Button>
			<Button size="touch" variant="outline" onclick={() => (step = 'rename')}>Not this</Button>
			<Button size="touch" variant="ghost" class="text-muted" onclick={() => decide('ignore')}>Ignore place</Button>
		</div>
	{:else if step === 'rename'}
		<Field label="What is this place?" id="{id}-name">
			<Input id="{id}-name" placeholder="For example Office or Pool" class="h-11" bind:value={name} />
		</Field>
		<div class="flex flex-wrap gap-2">
			<Button size="touch" onclick={() => decide('rename')}>Save name</Button>
			<Button size="touch" variant="ghost" class="text-muted" onclick={() => decide('ignore')}>Ignore place</Button>
		</div>
	{:else}
		<p class="text-sm text-muted" role="status">
			{#if step === 'linked'}Linked to {picked?.label}. Aroh can nudge you here when it is due or missed.{:else if step === 'renamed'}Saved as {name.trim()}. Link it to a goal from Places.{:else}Ignored. Aroh will not use or ask about this place. Undo in Settings, Location.{/if}
		</p>
	{/if}
</Card.Root>
