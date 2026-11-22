<script lang="ts">
	import Render from '../components/Render.svelte';
	import QuickLog from '../components/QuickLog.svelte';
	import ActionList from '../components/planning/ActionList.svelte';
	import AllDayChip from '../components/planning/AllDayChip.svelte';
	import AllDayChoice from '../components/planning/AllDayChoice.svelte';
	import ActionSheet from './ActionSheet.svelte';
	import { allDayOn, applyEdits } from './edits';
	import { getClock } from '$lib/ui/clock';
	import { formatDate, formatDuration, formatTime, formatYmd } from '$lib/ui/format';
	import type { ActionView, ComponentSpec, OnChange } from '$lib/ui/contract';
	import type { ActionEdit, AllDayView, DayKind, DayPick } from './types';

	/**
	 * Today: date header, today's all-day events (a chip tap reopens its kind), the kind question
	 * for the next unanswered all-day event, next action, check-in, today's actions, one insight and
	 * the quick-log bar. Tapping an action opens the edit sheet (`inline` draws it in place for
	 * mockups). `onaction` gets `{ id, status }`, `oncheckin` the answer, `onlog` the quick-log text,
	 * `onedit`, `ondelete` and `onchoose` the calendar changes.
	 */
	let {
		actions,
		checkIn = null,
		insight = null,
		allDay = [],
		editing: openId = null,
		confirming = false,
		inline = false,
		onaction,
		oncheckin,
		onlog,
		onedit,
		ondelete,
		onchoose
	}: {
		actions: ActionView[];
		checkIn?: ComponentSpec | null;
		insight?: ComponentSpec | null;
		allDay?: AllDayView[];
		editing?: string | null;
		confirming?: boolean;
		inline?: boolean;
		onaction?: OnChange;
		oncheckin?: OnChange;
		onlog?: (text: string) => void;
		onedit?: (edit: ActionEdit) => void;
		ondelete?: (id: string) => void;
		onchoose?: (pick: DayPick) => void;
	} = $props();

	const { now, zone } = getClock();
	const todayYmd = formatYmd(now, zone);
	let edits = $state<Record<string, ActionEdit>>({});
	let deleted = $state<string[]>([]);
	let picks = $state<Record<string, DayKind>>({});
	let choosing = $state<string | null>(null);
	let editing = $derived(openId);
	const events = $derived(allDayOn(allDay, todayYmd, picks));
	const busyDay = $derived(events.find((e) => e.kind === 'busy'));
	const away = $derived(events.find((e) => e.kind === 'away'));
	const today = $derived(applyEdits(actions, edits, deleted).filter((a) => formatYmd(a.start, zone) === todayYmd));
	const reopened = $derived(choosing ? allDay.map((e) => (picks[e.id] ? { ...e, kind: picks[e.id] } : e)).find((e) => e.id === choosing) : undefined);
	const asking = $derived(reopened ?? allDay.find((e) => e.kind === null && !picks[e.id] && e.end >= todayYmd));
	const open = $derived(editing ? today.find((a) => a.id === editing) : undefined);
	const next = $derived(today.find((a) => a.status === 'planned' && Date.parse(a.start) > Date.parse(now)));
	const left = $derived(today.filter((a) => a.status === 'planned').length);
	const heading = formatDate(todayYmd, { weekday: 'long', day: 'numeric', month: 'long' });
	const minutesTo = $derived(next ? Math.round((Date.parse(next.start) - Date.parse(now)) / 60_000) : 0);

	/** Applies a move or resize locally, closes the sheet and reports it. */
	function edit(e: ActionEdit) {
		edits[e.id] = e;
		editing = null;
		onedit?.(e);
	}

	/** Removes an action locally, closes the sheet and reports it. */
	function remove(id: string) {
		deleted = [...deleted, id];
		editing = null;
		ondelete?.(id);
	}

	/** Stores the owner's kind for an all-day event, closes a reopened card and reports it. */
	function choose(pick: DayPick) {
		picks[pick.id] = pick.kind;
		choosing = null;
		onchoose?.(pick);
	}
</script>

<div class="flex min-h-full flex-col">
	<div class="flex w-full max-w-5xl flex-1 flex-col gap-4 p-4 @3xl:p-6">
		<div class="flex flex-col gap-2">
			<div>
				<h2 class="text-xl font-semibold">{heading}</h2>
				<p class="text-sm text-muted">
					{#if busyDay}{busyDay.title}: busy day. {/if}{#if away}{away.title}: away. {/if}{left} {left === 1 ? 'action' : 'actions'} left today{#if next}. Next: <span class="font-medium text-fg">{next.title}</span> at {formatTime(next.start, zone)}, in {formatDuration(minutesTo)}{/if}
				</p>
			</div>
			{#if events.length}
				<ul class="flex flex-col gap-1" aria-label="All day">
					{#each events as e (e.id)}<li><AllDayChip event={e} onpick={onchoose ? (id) => (choosing = id) : undefined} /></li>{/each}
				</ul>
			{/if}
		</div>
		{#if asking && onchoose}<AllDayChoice event={asking} onchoose={choose} onclose={reopened ? () => (choosing = null) : undefined} />{/if}
		<div class="grid gap-4 @3xl:grid-cols-[minmax(0,1fr)_minmax(0,360px)]">
			<div class="flex flex-col gap-4">
				<ActionList spec={{ kind: 'action_list', label: "Today's actions", actions: today }} onchange={onaction} onopen={onedit ? (id) => (editing = id) : undefined} />
			</div>
			<div class="flex flex-col gap-4">
				{#if checkIn}<Render spec={checkIn} onchange={oncheckin} />{/if}
				{#if insight}<Render spec={insight} />{/if}
			</div>
		</div>
	</div>
	<QuickLog {onlog} />
</div>
{#if open}
	<ActionSheet action={open} {confirming} {inline} onsave={edit} ondelete={remove} onclose={() => (editing = null)} />
{/if}
