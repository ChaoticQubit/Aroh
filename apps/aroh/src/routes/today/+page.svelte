<script lang="ts">
	import TodayScreen from '$lib/ui/screens/TodayScreen.svelte';
	import { getClock } from '$lib/ui/clock';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { actionViews, checkInData, dueCheckIns } from '$lib/app/views';

	const clock = getClock();
	const s = $derived(data.snapshot);
	const due = $derived(s ? dueCheckIns(s, clock)[0] : undefined);

	/** Saves a ticked or unticked action. */
	function onaction(value: unknown) {
		const { id, status } = value as { id: string; status: 'planned' | 'done' };
		void data.run(() => client.setActionStatus(id, status));
	}

	/** Writes the check-in answer as a tracker entry. */
	function oncheckin(answer: unknown) {
		const entry = due && checkInData(due, answer);
		if (due && entry) void data.run(() => client.addEntry(due.trackerId, entry));
	}
</script>

{#if s}
	<TodayScreen actions={actionViews(s)} checkIn={due?.spec ?? null} {onaction} {oncheckin} onlog={(text) => data.run(() => client.logText(text))} />
{/if}
