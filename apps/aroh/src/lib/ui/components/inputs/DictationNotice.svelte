<script lang="ts">
	import { Button } from '$lib/ui/primitives/button';
	import { openSettings, type Dictation } from '$lib/app/speech.svelte';

	/**
	 * Why the last mic tap failed, shown above a sticky composer so the tab bar never hides it.
	 * Input: the mic's `dictation`. With microphone or speech access off it adds Open Settings.
	 * Renders nothing while there is no error.
	 */
	let { dictation }: { dictation: Dictation } = $props();
</script>

{#if dictation.error}
	<div role="alert" class="flex items-center gap-2 border-t border-border bg-bg px-3 py-2 text-sm text-status-missed">
		<p class="min-w-0 flex-1">{dictation.error}</p>
		{#if dictation.denied}<Button variant="outline" size="touch" type="button" onclick={() => void openSettings().catch(() => undefined)}>Open Settings</Button>{/if}
	</div>
{/if}
