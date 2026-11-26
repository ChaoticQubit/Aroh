<script lang="ts">
	import { onMount } from 'svelte';
	import CalendarScreen from '$lib/ui/screens/CalendarScreen.svelte';
	import { getClock } from '$lib/ui/clock';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { editStatus } from '$lib/app/replan.svelte';
	import ReplanStatus from '$lib/ui/components/ReplanStatus.svelte';
	import CatchUpBanner from '$lib/ui/components/CatchUpBanner.svelte';
	import { catchUp } from '$lib/app/catchup.svelte';
	import { actionViews, categoryList } from '$lib/app/views';
	import { allDayViews, busyViews, confirmDay, refreshSuggestions } from '$lib/app/calendar';
	import DiaryDayTop from '$lib/ui/diary/DiaryDayTop.svelte';

	const clock = getClock();
	const s = $derived(data.snapshot);

	onMount(() => void (data.snapshot && refreshSuggestions(data.snapshot)));
</script>

{#if editStatus.phase}
	<div class="px-4 pt-3"><ReplanStatus phase={editStatus.phase} onretry={() => editStatus.retry()} onundo={() => editStatus.undo()} ondismiss={() => editStatus.dismiss()} /></div>
{/if}
{#if catchUp.offer || catchUp.note}
	<div class="px-4 pt-3"><CatchUpBanner offer={catchUp.offer} note={catchUp.note} onremove={() => catchUp.remove()} onkeep={() => catchUp.keep()} /></div>
{/if}
{#if s}
	<CalendarScreen
		actions={actionViews(s)}
		categories={categoryList(s)}
		busy={busyViews(s)}
		allDay={allDayViews(s, clock.zone)}
		onstatus={(id, status) => {
			const action = s.actions.find((a) => a.id === id);
			if (action) void catchUp.setStatus(action, status);
		}}
		onedit={(e) => data.run(() => client.editAction(e.id, e.start, e.end).then(() => editStatus.track(e.id)))}
		onlock={(id, locked) => data.run(() => client.setLocked(id, locked))}
		ondelete={(id, scope) => data.run(() => client.deleteAction(id, scope).then(() => editStatus.track(id)))}
		onchoose={(p) => data.run(() => confirmDay(p.id, p.kind))}
		goals={s.goals.filter((g) => g.status === 'active').map((g) => ({ id: g.id, title: g.title }))}
		oncreate={(n) => data.run(() => client.createAction(n.title, n.start, n.minutes, n.goalId))}
	>
		{#snippet dayTop(day: string)}<DiaryDayTop {day} actions={actionViews(s)} zone={clock.zone} />{/snippet}
	</CalendarScreen>
{/if}
