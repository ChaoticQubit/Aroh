<script lang="ts">
	import Lock from '@lucide/svelte/icons/lock';
	import Mic from '@lucide/svelte/icons/mic';
	import Send from '@lucide/svelte/icons/send';
	import { Button } from '$lib/ui/primitives/button';
	import { Dictation, type Speech } from '$lib/app/speech.svelte';
	import DictationNotice from '$lib/ui/components/inputs/DictationNotice.svelte';
	import { follow } from '$lib/ui/follow';

	/**
	 * The diary composer on the Log tab (Q85): a text area, a mic that dictates into it given
	 * `voice`, and send. Emits the trimmed text and whether it was spoken; `busy` disables send
	 * while an entry is being sealed and structured. The draft lives in memory only.
	 */
	let { onsend, voice, busy = false }: { onsend: (text: string, spoken: boolean) => void; voice?: Speech; busy?: boolean } = $props();

	const id = $props.id();
	let draft = $state('');
	let spoken = $state(false);
	const dictation = $derived(
		voice
			? new Dictation(voice, () => draft, (text) => {
					draft = text;
					spoken = true;
				})
			: null
	);
	$effect(() => {
		const mic = dictation;
		return () => void mic?.release();
	});
</script>

{#if dictation}<DictationNotice {dictation} />{/if}
<form
	class="flex flex-col gap-2 rounded-sm border border-border bg-bg p-3"
	onsubmit={(e) => {
		e.preventDefault();
		const text = draft.trim();
		if (!text || busy) return;
		onsend(text, spoken);
		draft = '';
		spoken = false;
	}}
>
	<label for={id} class="text-sm font-semibold">How is your day going?</label>
	<textarea {id} bind:value={draft} use:follow={draft} rows="2" maxlength="2000" placeholder="Write or speak. Meals, water and workouts become tracker numbers." class="min-h-16 w-full resize-none rounded-sm border border-input bg-transparent px-3 py-2 text-lg outline-none placeholder:text-muted focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"></textarea>
	<div class="flex items-center gap-1">
		<p class="flex flex-1 items-center gap-1 text-xs text-muted"><Lock class="size-3" aria-hidden="true" />Encrypted on this phone before it syncs</p>
		{#if dictation}
			<Button variant={dictation.listening ? 'default' : 'ghost'} size="icon-touch" type="button" aria-label={dictation.listening ? 'Stop speaking' : 'Speak a diary entry'} aria-pressed={dictation.listening} onclick={() => void dictation.toggle()}><Mic /></Button>
		{/if}
		<Button size="icon-touch" type="submit" aria-label="Add to diary" disabled={busy || !draft.trim()}><Send /></Button>
	</div>
</form>
