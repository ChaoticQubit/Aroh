<script lang="ts">
	import { onMount } from 'svelte';
	import TodayScreen from '$lib/ui/screens/TodayScreen.svelte';
	import { getClock } from '$lib/ui/clock';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { actionViews, checkInData, dueCheckIns } from '$lib/app/views';
	import { allDayViews, confirmDay, openTriage, triageKinds } from '$lib/app/calendar';
	import type { DayKind } from '$lib/ui/screens/types';

	const clock = getClock();
	const s = $derived(data.snapshot);
	const due = $derived(s ? dueCheckIns(s, clock)[0] : undefined);
	let suggested = $state<Record<string, DayKind>>({});

	onMount(() => void openTriage().then((t) => (suggested = triageKinds(t?.waiting))));

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
	<TodayScreen
		actions={actionViews(s)}
		checkIn={due?.spec ?? null}
		allDay={allDayViews(s, clock.zone, suggested)}
		{onaction}
		{oncheckin}
		onlog={(text) => data.run(() => client.logText(text))}
		onedit={(e) => data.run(() => client.editAction(e.id, e.start, e.end))}
		ondelete={(id) => data.run(() => client.deleteAction(id))}
		onchoose={(p) => data.run(() => confirmDay(p.id, p.kind))}
	/>
{/if}
