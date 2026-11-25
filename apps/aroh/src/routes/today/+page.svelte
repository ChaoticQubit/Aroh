<script lang="ts">
	import { onMount } from 'svelte';
	import TodayScreen from '$lib/ui/screens/TodayScreen.svelte';
	import { getClock } from '$lib/ui/clock';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { editStatus } from '$lib/app/replan.svelte';
	import ReplanStatus from '$lib/ui/components/ReplanStatus.svelte';
	import CatchUpBanner from '$lib/ui/components/CatchUpBanner.svelte';
	import { catchUp } from '$lib/app/catchup.svelte';
	import { speech } from '$lib/app/speech.svelte';
	import { actionViews, checkInData, dueCheckIns } from '$lib/app/views';
	import { allDayViews, confirmDay, refreshSuggestions } from '$lib/app/calendar';

	const clock = getClock();
	const s = $derived(data.snapshot);
	const due = $derived(s ? dueCheckIns(s, clock)[0] : undefined);

	onMount(() => void (data.snapshot && refreshSuggestions(data.snapshot)));

	/** Saves a status picked on a row tick or in the sheet; a late done may offer the catch-up (Q78). */
	function setStatus(id: string, status: 'planned' | 'done' | 'skipped') {
		const action = s?.actions.find((a) => a.id === id);
		if (action) void catchUp.setStatus(action, status);
	}

	/** Saves a ticked or unticked action. */
	function onaction(value: unknown) {
		const { id, status } = value as { id: string; status: 'planned' | 'done' };
		setStatus(id, status);
	}

	/** Writes the check-in answer as a tracker entry. */
	function oncheckin(answer: unknown) {
		const entry = due && checkInData(due, answer);
		if (due && entry) void data.run(() => client.addEntry(due.trackerId, entry));
	}
</script>

{#if editStatus.phase}
	<div class="px-4 pt-3"><ReplanStatus phase={editStatus.phase} onretry={() => editStatus.retry()} onundo={() => editStatus.undo()} ondismiss={() => editStatus.dismiss()} /></div>
{/if}
{#if catchUp.offer || catchUp.note}
	<div class="px-4 pt-3"><CatchUpBanner offer={catchUp.offer} note={catchUp.note} onremove={() => catchUp.remove()} onkeep={() => catchUp.keep()} /></div>
{/if}
{#if s}
	<TodayScreen
		actions={actionViews(s)}
		checkIn={due?.spec ?? null}
		allDay={allDayViews(s, clock.zone)}
		{onaction}
		onstatus={setStatus}
		{oncheckin}
		onlog={(text) => data.run(() => client.logText(text))}
		voice={speech}
		onedit={(e) => data.run(() => client.editAction(e.id, e.start, e.end).then(() => editStatus.track(e.id)))}
		onlock={(id, locked) => data.run(() => client.setLocked(id, locked))}
		ondelete={(id, scope) => data.run(() => client.deleteAction(id, scope).then(() => editStatus.track(id)))}
		onchoose={(p) => data.run(() => confirmDay(p.id, p.kind))}
	/>
{/if}
