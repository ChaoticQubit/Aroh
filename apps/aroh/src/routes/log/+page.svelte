<script lang="ts">
	import LogScreen from '$lib/ui/screens/LogScreen.svelte';
	import DayRoute from '$lib/ui/diary/DayRoute.svelte';
	import { getClock } from '$lib/ui/clock';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { speech } from '$lib/app/speech.svelte';
	import { checkInData, logView } from '$lib/app/views';
	import { diaryDayOf } from '$lib/app/diary';

	const clock = getClock();
	const s = $derived(data.snapshot);
	const view = $derived(s ? logView(s, clock) : null);
	const today = diaryDayOf(clock.now, clock.zone);

	/** Writes a check-in answer for its tracker. */
	function oncheckin(trackerId: string, answer: unknown) {
		const due = view?.checkIns.find((c) => c.trackerId === trackerId);
		const entry = due && checkInData(due, answer);
		if (entry) void data.run(() => client.addEntry(trackerId, entry));
	}
</script>

<DayRoute day={today} />
{#if view}
	<LogScreen {...view} {oncheckin} onlog={(text) => data.run(() => client.logText(text))} voice={speech} />
{/if}
