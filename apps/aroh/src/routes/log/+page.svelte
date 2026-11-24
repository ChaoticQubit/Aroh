<script lang="ts">
	import LogScreen from '$lib/ui/screens/LogScreen.svelte';
	import { getClock } from '$lib/ui/clock';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { speech } from '$lib/app/speech.svelte';
	import { checkInData, logView } from '$lib/app/views';

	const clock = getClock();
	const s = $derived(data.snapshot);
	const view = $derived(s ? logView(s, clock) : null);

	/** Writes a check-in answer for its tracker. */
	function oncheckin(trackerId: string, answer: unknown) {
		const due = view?.checkIns.find((c) => c.trackerId === trackerId);
		const entry = due && checkInData(due, answer);
		if (entry) void data.run(() => client.addEntry(trackerId, entry));
	}
</script>

{#if view}
	<LogScreen {...view} {oncheckin} onlog={(text) => data.run(() => client.logText(text))} voice={speech} />
{/if}
