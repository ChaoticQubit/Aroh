<script lang="ts">
	import { untrack } from 'svelte';
	import Plus from '@lucide/svelte/icons/plus';
	import X from '@lucide/svelte/icons/x';
	import { Button } from '$lib/ui/primitives/button';
	import { Input } from '$lib/ui/primitives/input';
	import { Textarea } from '$lib/ui/primitives/textarea';
	import { draftError, draftOf, type CardDraft, type DiaryStructure } from '$lib/app/diary';

	/**
	 * The edit form of a diary card (B120): summary, mood and energy 1 to 5, highlights (add, edit,
	 * remove), food, water, workout and notes for tomorrow, starting from `structure`. `onsave` gets
	 * the draft once it passes the card bounds and resolves with an error sentence or null; `oncancel`
	 * closes the form. The draft lives in memory only.
	 */
	let { structure, onsave, oncancel }: { structure: DiaryStructure; onsave: (draft: CardDraft) => Promise<string | null>; oncancel: () => void } = $props();

	const id = $props.id();
	let draft = $state<CardDraft>(untrack(() => draftOf(structure)));
	let error = $state<string | null>(null);
	let saving = $state(false);

	/** Checks the draft, saves it, and shows the bound or save error when there is one. */
	async function save(e: SubmitEvent) {
		e.preventDefault();
		error = draftError(draft);
		if (error || saving) return;
		saving = true;
		error = await onsave($state.snapshot(draft));
		saving = false;
	}
</script>

{#snippet scale(name: 'Mood' | 'Energy', value: number, set: (n: number) => void)}
	<fieldset class="flex flex-col gap-1">
		<legend class="text-sm font-semibold">{name}</legend>
		<div class="flex gap-1" role="radiogroup" aria-label={name}>
			{#each [1, 2, 3, 4, 5] as n (n)}
				<button type="button" role="radio" aria-checked={value === n} aria-label="{name} {n} of 5" onclick={() => set(n)} class="flex size-11 items-center justify-center rounded-sm border text-base tabular-nums outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {value === n ? 'border-fg bg-fg text-bg' : 'border-border'}">{n}</button>
			{/each}
		</div>
	</fieldset>
{/snippet}

<form class="flex flex-col gap-4" aria-label="Edit card" onsubmit={save}>
	<div class="flex flex-col gap-1">
		<label for="{id}-summary" class="text-sm font-semibold">Summary</label>
		<Textarea id="{id}-summary" bind:value={draft.summary} rows={3} maxlength={1000} class="text-lg" />
	</div>
	{@render scale('Mood', draft.mood, (n) => (draft.mood = n))}
	{@render scale('Energy', draft.energy, (n) => (draft.energy = n))}

	<fieldset class="flex flex-col gap-2">
		<legend class="text-sm font-semibold">Highlights</legend>
		{#each draft.highlights as _, i (i)}
			<div class="flex items-center gap-1">
				<Input bind:value={draft.highlights[i]} maxlength={300} aria-label="Highlight {i + 1}" class="h-11 text-lg" />
				<Button variant="ghost" size="icon-touch" type="button" aria-label="Remove highlight {i + 1}" onclick={() => draft.highlights.splice(i, 1)}><X /></Button>
			</div>
		{/each}
		{#if draft.highlights.length < 10}
			<Button variant="outline" size="touch" type="button" class="self-start" onclick={() => draft.highlights.push('')}><Plus />Add highlight</Button>
		{/if}
	</fieldset>

	{#each [['food', 'Food'], ['water', 'Water'], ['workout', 'Workout']] as const as [key, name] (key)}
		<div class="flex flex-col gap-1">
			<label for="{id}-{key}" class="text-sm font-semibold">{name}</label>
			<Input id="{id}-{key}" bind:value={draft[key]} maxlength={300} placeholder="Nothing logged" class="h-11 text-lg" />
		</div>
	{/each}
	<div class="flex flex-col gap-1">
		<label for="{id}-notes" class="text-sm font-semibold">Notes for tomorrow</label>
		<Textarea id="{id}-notes" bind:value={draft.notesForTomorrow} rows={2} maxlength={1000} class="text-lg" />
	</div>

	{#if error}<p class="text-sm text-status-missed" role="alert">{error}</p>{/if}
	<div class="flex justify-end gap-2">
		<Button variant="outline" size="touch" type="button" onclick={oncancel} disabled={saving}>Cancel</Button>
		<Button size="touch" type="submit" disabled={saving}>{saving ? 'Saving…' : 'Save card'}</Button>
	</div>
</form>
