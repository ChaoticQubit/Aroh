<script lang="ts">
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import PhoneShell from '$lib/ui/shell/PhoneShell.svelte';
	import { Button } from '$lib/ui/primitives/button';
	import Segmented from '$lib/ui/components/Segmented.svelte';
	import CalendarGrid from '$lib/ui/screens/CalendarGrid.svelte';
	import ActionSheet from '$lib/ui/screens/ActionSheet.svelte';
	import TodayScreen from '$lib/ui/screens/TodayScreen.svelte';
	import CatchUpBanner from '$lib/ui/components/CatchUpBanner.svelte';
	import { sixpackAllDay } from '$lib/ui/mockdata/sixpack';
	import { missedId, weekActions, weekBusy, weekDays } from './data';

	/**
	 * The "Week view" mock (Q78) on the real components: the phone week as seven narrow day columns
	 * with an hour grid, a block selected for dragging, the edit sheet of a missed session with its
	 * status control, and Today with Not logged yet and the catch-up question. `solo` renders one
	 * frame full screen for screenshots.
	 */
	let { inboxCount, solo = null }: { inboxCount: number; solo?: number | null } = $props();

	type Frame = { name: string; body: 'week' | 'selected' | 'sheet' | 'today' };
	const FRAMES: Frame[] = [
		{ name: '1. Week, 7 days with an hour grid', body: 'week' },
		{ name: '2. Strength picked up to drag', body: 'selected' },
		{ name: '3. Tap: missed session, log it done', body: 'sheet' },
		{ name: '4. Today: not logged yet, catch-up', body: 'today' }
	];
	const list = $derived(solo === null ? FRAMES : FRAMES.slice(solo, solo + 1));
	const keep = () => {};
	const missed = weekActions.find((a) => a.id === missedId)!;
	const offer = { text: 'A catch-up was added because this was missed. Remove it?', detail: 'Removes Easy run (catch-up), Sat 28 Nov 09:00.' };
</script>

{#snippet calendar(selected: string | null)}
	<div class="flex flex-col gap-3 p-4">
		<div class="flex flex-wrap items-center gap-2">
			<Button variant="outline" size="touch">Today</Button>
			<Button variant="ghost" size="icon-touch" aria-label="Previous"><ChevronLeft /></Button>
			<Button variant="ghost" size="icon-touch" aria-label="Next"><ChevronRight /></Button>
			<p class="mr-auto text-sm font-semibold">23 Nov - 29 Nov 2026</p>
			<Segmented label="Calendar view" value="week" options={[{ value: 'day', label: 'Day' }, { value: 'week', label: 'Week' }, { value: 'month', label: 'Month' }]} />
		</div>
		<CalendarGrid compact days={weekDays} actions={weekActions} busy={weekBusy} allDay={sixpackAllDay} {selected} onopen={keep} onedit={keep} onpick={keep} />
	</div>
{/snippet}

{#snippet frame(f: Frame)}
	<div class="relative h-full">
		<PhoneShell title={f.body === 'today' ? 'Today' : 'Calendar'} tab={f.body === 'today' ? 'today' : 'calendar'} {inboxCount}>
			{#if f.body === 'week'}
				{@render calendar(null)}
			{:else if f.body === 'selected'}
				{@render calendar(missedId)}
			{:else if f.body === 'sheet'}
				{@render calendar(null)}
			{:else}
				<div class="px-4 pt-3"><CatchUpBanner {offer} onremove={keep} onkeep={keep} /></div>
				<TodayScreen actions={weekActions} onaction={keep} onstatus={keep} onedit={keep} onlock={keep} ondelete={keep} />
			{/if}
		</PhoneShell>
		{#if f.body === 'sheet'}
			<ActionSheet action={missed} inline onstatus={keep} onsave={keep} onlock={keep} ondelete={keep} onclose={keep} />
		{/if}
	</div>
{/snippet}

{#if solo !== null}
	<div class="h-dvh w-full overflow-hidden bg-bg">{@render frame(list[0])}</div>
{:else}
	<ul class="flex gap-6 overflow-x-auto pb-4">
		{#each list as f (f.name)}
			<li class="flex shrink-0 flex-col items-center gap-2">
				<div class="box-content h-[844px] w-[393px] [transform:translateZ(0)] overflow-hidden rounded-md border border-border bg-bg">{@render frame(f)}</div>
				<p class="text-sm font-medium">{f.name}</p>
			</li>
		{/each}
	</ul>
{/if}
