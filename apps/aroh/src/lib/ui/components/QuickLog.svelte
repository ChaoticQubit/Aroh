<script lang="ts">
	import Camera from '@lucide/svelte/icons/camera';
	import Mic from '@lucide/svelte/icons/mic';
	import Send from '@lucide/svelte/icons/send';
	import { Button } from '$lib/ui/primitives/button';

	/** Sticky quick-log bar: text field, mic, photo and send. Emits the trimmed text on send. */
	let { onlog }: { onlog?: (text: string) => void } = $props();

	const id = $props.id();
	let draft = $state('');
</script>

<form
	class="sticky bottom-0 border-t border-border bg-bg p-2"
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
		<Button variant="ghost" size="icon-touch" type="button" aria-label="Speak a log entry"><Mic /></Button>
		<Button variant="ghost" size="icon-touch" type="button" aria-label="Log with a photo"><Camera /></Button>
		<Button size="icon-touch" type="submit" aria-label="Log" disabled={!draft.trim()}><Send /></Button>
	</div>
</form>
