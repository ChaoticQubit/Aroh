<script lang="ts">
	import { onMount } from 'svelte';
	import CalendarScreen from '$lib/ui/screens/CalendarScreen.svelte';
	import { getClock } from '$lib/ui/clock';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { actionViews, categoryList } from '$lib/app/views';
	import { allDayViews, busyViews, confirmDay, refreshSuggestions } from '$lib/app/calendar';

	const clock = getClock();
	const s = $derived(data.snapshot);

	onMount(() => void (data.snapshot && refreshSuggestions(data.snapshot)));
</script>

{#if s}
	<CalendarScreen
		actions={actionViews(s)}
		categories={categoryList(s)}
		busy={busyViews(s)}
		allDay={allDayViews(s, clock.zone)}
		onedit={(e) => data.run(() => client.editAction(e.id, e.start, e.end))}
		onlock={(id, locked) => data.run(() => client.setLocked(id, locked))}
		ondelete={(id) => data.run(() => client.deleteAction(id))}
		onchoose={(p) => data.run(() => confirmDay(p.id, p.kind))}
	/>
{/if}
