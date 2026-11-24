<script lang="ts">
	import Mic from '@lucide/svelte/icons/mic';
	import Send from '@lucide/svelte/icons/send';
	import { Button } from '$lib/ui/primitives/button';
	import { Dictation, type Speech } from '$lib/app/speech.svelte';
	import DictationNotice from './inputs/DictationNotice.svelte';

	/**
	 * Sticky quick-log bar: text field, send and, given `voice`, a mic that dictates into the field.
	 * Emits the trimmed text on send. No photo button until photo logging exists (Q59).
	 */
	let { onlog, voice }: { onlog?: (text: string) => void; voice?: Speech } = $props();

	const id = $props.id();
	let draft = $state('');
	const dictation = $derived(voice ? new Dictation(voice, () => draft, (text) => (draft = text)) : null);
</script>

<div class="sticky bottom-0">
	{#if dictation}<DictationNotice {dictation} />{/if}
	<form
		class="border-t border-border bg-bg p-2"
		onsubmit={(e) => {
			e.preventDefault();
			const text = draft.trim();
			draft = '';
			if (text) onlog?.(text);
		}}
	>
		<div class="flex max-w-5xl items-center gap-1">
			<label for={id} class="sr-only">Quick log</label>
			<input {id} bind:value={draft} placeholder="Log a meal, weight or note" class="h-11 min-w-0 flex-1 rounded-sm border border-input bg-transparent px-3 text-lg outline-none placeholder:text-muted focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50" />
			{#if dictation}
				<Button variant={dictation.listening ? 'default' : 'ghost'} size="icon-touch" type="button" aria-label={dictation.listening ? 'Stop speaking' : 'Speak a log entry'} aria-pressed={dictation.listening} onclick={() => void dictation.toggle()}><Mic /></Button>
			{/if}
			<Button size="icon-touch" type="submit" aria-label="Log" disabled={!draft.trim()}><Send /></Button>
		</div>
	</form>
</div>
