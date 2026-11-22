<script lang="ts">
	import { onMount } from 'svelte';
	import CalendarScreen from '$lib/ui/screens/CalendarScreen.svelte';
	import { getClock } from '$lib/ui/clock';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { actionViews, categoryList } from '$lib/app/views';
	import { allDayViews, busyViews, confirmDay, openTriage, triageKinds } from '$lib/app/calendar';
	import type { DayKind } from '$lib/ui/screens/types';

	const clock = getClock();
	const s = $derived(data.snapshot);
	let suggested = $state<Record<string, DayKind>>({});

	onMount(() => void openTriage().then((t) => (suggested = triageKinds(t?.waiting))));
</script>

{#if s}
	<CalendarScreen
		actions={actionViews(s)}
		categories={categoryList(s)}
		busy={busyViews(s)}
		allDay={allDayViews(s, clock.zone, suggested)}
		onedit={(e) => data.run(() => client.editAction(e.id, e.start, e.end))}
		ondelete={(id) => data.run(() => client.deleteAction(id))}
		onchoose={(p) => data.run(() => confirmDay(p.id, p.kind))}
	/>
{/if}
