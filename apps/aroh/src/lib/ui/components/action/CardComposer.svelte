<script lang="ts">
	import Mic from '@lucide/svelte/icons/mic';
	import Send from '@lucide/svelte/icons/send';
	import { Button } from '$lib/ui/primitives/button';
	import { untrack } from 'svelte';
	import { deviceKeep, readDraft, writeDraft } from '$lib/app/chatkeep';
	import { grow } from '$lib/ui/grow';

	/**
	 * Composer of the card chat: one text field, the mic (shown only when `mic` is true, Q61) and Send.
	 * `listening` draws the mic pressed. Calls `onsend` with the trimmed text and `onmic` on the
	 * mic; the caller empties `draft` once the text is used. With `keep` the unsent draft is kept under that key until sent (B60).
	 */
	let {
		draft = $bindable(''),
		placeholder = 'Change this card',
		mic = true,
		listening = false,
		keep,
		onsend,
		onmic
	}: { draft?: string; placeholder?: string; mic?: boolean; listening?: boolean; keep?: string; onsend?: (text: string) => void; onmic?: () => void } = $props();

	const id = $props.id();
	const store = deviceKeep();
	untrack(() => {
		if (keep && !draft) draft = readDraft(store, keep);
	});
	$effect(() => {
		if (keep) writeDraft(store, keep, draft);
	});
</script>

<form
	class="flex items-end gap-1 border-t border-border bg-bg p-2"
	onsubmit={(e) => {
		e.preventDefault();
		if (draft.trim()) onsend?.(draft.trim());
	}}
>
	<label for={id} class="sr-only">Message to the coach</label>
	<textarea {id} rows="1" {placeholder} use:grow={draft} class="min-h-11 min-w-0 flex-1 resize-none rounded-sm border border-input bg-transparent px-3 py-2 text-lg outline-none placeholder:truncate placeholder:text-muted focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50" bind:value={draft}></textarea>
	{#if mic}
		<Button variant={listening ? 'default' : 'ghost'} size="icon-touch" type="button" aria-label={listening ? 'Stop speaking' : 'Speak to the coach'} aria-pressed={listening} onclick={() => onmic?.()}><Mic /></Button>
	{/if}
	<Button size="icon-touch" type="submit" aria-label="Send" disabled={!draft.trim()}><Send /></Button>
</form>
