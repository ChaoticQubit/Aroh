<script lang="ts">
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import { Button } from '$lib/ui/primitives/button';
	import Segmented from '../components/Segmented.svelte';
	import CategoryFilter from '../components/CategoryFilter.svelte';
	import CalendarGrid from './CalendarGrid.svelte';
	import ActionSheet from './ActionSheet.svelte';
	import AllDayChoice from '../components/planning/AllDayChoice.svelte';
	import { allDayOn, applyEdits } from './edits';
	import { getClock } from '$lib/ui/clock';
	import { daysBetween, formatDate, formatYmd, mondayOf, shiftDay, shiftMonth } from '$lib/ui/format';
	import { stepAnchor, swipe } from '$lib/ui/swipe';
	import type { Snippet } from 'svelte';
	import type { ActionView, DeleteScope } from '$lib/ui/contract';
	import type { ActionEdit, AllDayView, BusyView, CategoryView, DayKind, DayPick, GoalChoice, NewAction } from './types';

	/**
	 * Calendar: month title with the range and its session count, Today, previous, next and the Day,
	 * Week, Month switch, category chips, then a week hour grid on wide containers or, on phones, the
	 * the same week as a compact hour grid of seven narrow day columns (Q78). Kept device
	 * events (`busy`) show in the hour grids as neutral blocks, device all-day events (`allDay`) in
	 * an all-day row and as bars in month cells and the week strip, with the kind question for the
	 * first unanswered one in view; tapping a chip reopens it. A sideways swipe pages a day, week or
	 * month like the arrows. Tapping an action opens the edit
	 * sheet; tapping an empty slot opens the same sheet to add an item there (B121), filed under one
	 * of `goals` or none, reported by `oncreate`; tapping a week date or month cell opens that day.
	 * `onstatus`, `onedit`, `onlock`, `ondelete` and `onchoose` report the changes. `initialView`, `day`, `editing`,
	 * `confirming`, `selected` and `inline` (sheet drawn in place for mockups) set the opening state.
	 * `dayTop` renders above the day grid with the shown day, for the diary card.
	 */
	let {
		actions,
		categories,
		busy = [],
		allDay = [],
		initialView = 'week',
		day = undefined,
		editing: openId = null,
		confirming = false,
		selected: selectedId = null,
		inline = false,
		onstatus,
		onedit,
		onlock,
		ondelete,
		onchoose,
		goals = [],
		oncreate,
		dayTop
	}: {
		actions: ActionView[];
		categories: CategoryView[];
		busy?: BusyView[];
		allDay?: AllDayView[];
		initialView?: 'day' | 'week' | 'month';
		day?: string;
		editing?: string | null;
		confirming?: boolean;
		selected?: string | null;
		inline?: boolean;
		onstatus?: (id: string, status: 'planned' | 'done' | 'skipped') => void;
		onedit?: (edit: ActionEdit) => void;
		onlock?: (id: string, locked: boolean) => void;
		ondelete?: (id: string, scope: DeleteScope) => void;
		onchoose?: (pick: DayPick) => void;
		goals?: GoalChoice[];
		oncreate?: (item: NewAction) => void;
		dayTop?: Snippet<[string]>;
	} = $props();

	const NEW_MINUTES = 30;
	let adding = $state<ActionView | null>(null);

	const { now, zone } = getClock();
	const today = formatYmd(now, zone);
	const monday = mondayOf(today);
	let view = $derived(initialView);
	let anchor = $derived(day ?? today);
	let hidden = $state<number[]>([]);
	let edits = $state<Record<string, ActionEdit>>({});
	let deleted = $state<string[]>([]);
	let picks = $state<Record<string, DayKind>>({});
	let choosing = $state<string | null>(null);
	let editing = $derived(openId);
	let selected = $derived(selectedId);
	const events = $derived(allDay.map((e) => (picks[e.id] ? { ...e, kind: picks[e.id] } : e)));
	const current = $derived(applyEdits(actions, edits, deleted));
	const used = $derived(categories.filter((c) => current.some((a) => a.category === c.slot)));
	const visible = $derived(current.filter((a) => !hidden.includes(a.category)));
	const open = $derived(editing ? current.find((a) => a.id === editing) : undefined);
	const weekStart = $derived(shiftDay(monday, Math.floor(daysBetween(monday, anchor) / 7) * 7));
	const week = $derived(Array.from({ length: 7 }, (_, i) => shiftDay(weekStart, i)));
	const shown = $derived(view === 'month' ? [] : view === 'day' ? [anchor] : week);
	const reopened = $derived(choosing ? events.find((e) => e.id === choosing) : undefined);
	const asking = $derived(reopened ?? events.find((e) => e.kind === null && shown.some((d) => allDayOn([e], d).length > 0)));
	const monthStart = $derived(shiftMonth(anchor, 0));
	const monthGridStart = $derived(mondayOf(monthStart));
	const monthDays = $derived(Array.from({ length: 42 }, (_, i) => shiftDay(monthGridStart, i)));
	const range = $derived(
		view === 'day'
			? formatDate(anchor, { weekday: 'long', day: 'numeric', month: 'long' })
			: view === 'week'
				? `${formatDate(week[0], { day: 'numeric', month: 'short' })} to ${formatDate(week[6], { day: 'numeric', month: 'short' })}`
				: formatDate(monthStart, { month: 'long' })
	);
	const inRange = $derived(
		visible.filter((a) => {
			const d = formatYmd(a.start, zone);
			return view === 'month' ? d.slice(0, 7) === monthStart.slice(0, 7) : shown.includes(d);
		})
	);
	const goalCount = $derived(new Set(inRange.map((a) => a.goalTitle)).size);
	/** Moves the anchor one day, week or month in `dir`. */
	function step(dir: 1 | -1) {
		anchor = stepAnchor(view, anchor, dir);
	}

	/** Opens the edit sheet on an action and keeps it selected for dragging after the sheet closes. */
	function openAction(id: string) {
		adding = null;
		editing = id;
		selected = id;
	}

	/** Opens the add sheet on an empty slot starting at ISO `start`, 30 minutes long. */
	function addAt(start: string) {
		editing = null;
		adding = { id: '', title: '', start, end: new Date(Date.parse(start) + NEW_MINUTES * 60_000).toISOString(), status: 'planned', locked: true, category: 1, goalTitle: '' };
	}

	/** Reports a new item from the add sheet and closes it. */
	function create(item: NewAction) {
		adding = null;
		oncreate?.(item);
	}

	/** Shows day `d` in the day view, from a week date or a month cell. */
	function openDay(d: string) {
		anchor = d;
		view = 'day';
	}

	/** Applies a move or resize locally and reports it. */
	function edit(e: ActionEdit) {
		edits[e.id] = e;
		editing = null;
		onedit?.(e);
	}

	/** Removes an action locally and reports it. */
	function remove(id: string, scope: DeleteScope) {
		deleted = [...deleted, id];
		editing = null;
		selected = null;
		ondelete?.(id, scope);
	}

	/** Stores the owner's kind for an all-day event, closes a reopened card and reports it. */
	function choose(pick: DayPick) {
		picks[pick.id] = pick.kind;
		choosing = null;
		onchoose?.(pick);
	}
</script>

<div class="flex flex-col gap-4 p-4 @3xl:p-6">
	<div class="flex items-start justify-between gap-3">
		<header class="flex min-w-0 flex-col gap-1">
			<h2 class="text-2xl leading-tight font-semibold">{formatDate(view === 'month' ? monthStart : anchor, { month: 'long', year: 'numeric' })}</h2>
			<p class="text-sm text-muted" aria-live="polite">{range} · {inRange.length} {inRange.length === 1 ? 'session' : 'sessions'} across {goalCount} {goalCount === 1 ? 'goal' : 'goals'}</p>
		</header>
		<Button variant="outline" size="touch" class="shrink-0 @3xl:h-9" onclick={() => (anchor = today)}>Today</Button>
	</div>
	<div class="flex items-center justify-between gap-2">
		<Button variant="ghost" size="icon-touch" class="@3xl:size-9" aria-label="Previous" onclick={() => step(-1)}><ChevronLeft /></Button>
		<Segmented label="Calendar view" bind:value={view} options={[{ value: 'day', label: 'Day' }, { value: 'week', label: 'Week' }, { value: 'month', label: 'Month' }]} />
		<Button variant="ghost" size="icon-touch" class="@3xl:size-9" aria-label="Next" onclick={() => step(1)}><ChevronRight /></Button>
	</div>
	<CategoryFilter categories={used} bind:hidden />
	{#if asking && onchoose}<AllDayChoice event={asking} onchoose={choose} onclose={reopened ? () => (choosing = null) : undefined} />{/if}

	<div class="flex flex-col gap-4" use:swipe={step}>
	{#if view === 'month'}
		<div class="overflow-hidden rounded-sm border border-border bg-bg">
			<div class="grid grid-cols-7 border-b border-border">
				{#each week as d (d)}<p class="py-2 text-center text-xs text-muted">{formatDate(d, { weekday: 'short' })}</p>{/each}
			</div>
			<div class="grid grid-cols-7">
				{#each monthDays as d (d)}
					{@const items = visible.filter((a) => formatYmd(a.start, zone) === d)}
					{@const marks = allDayOn(events, d)}
					<button
						type="button"
						class="flex min-h-16 flex-col items-stretch gap-0.5 border-t border-l border-border p-1 text-left outline-none first:border-l-0 focus-visible:ring-3 focus-visible:ring-ring/50 @3xl:min-h-24 {d.slice(0, 7) === monthStart.slice(0, 7) ? '' : 'text-muted/60'}"
						aria-label="{d}, {marks.length ? `${marks.map((e) => e.title).join(', ')}, ` : ''}{items.length} actions"
						onclick={() => openDay(d)}
					>
						<span class="w-fit min-w-5 rounded-sm px-1 text-center text-xs leading-5 font-medium tabular-nums {d === today ? 'bg-accent text-accent-fg' : ''}">{Number(d.slice(8))}</span>
						{#each marks.slice(0, 2) as e (e.id)}
							<span class="truncate rounded-sm border px-1 text-[10px] leading-3.5 {e.kind === 'busy' || e.kind === 'away' ? 'border-border-strong bg-surface-2 text-fg' : e.kind === 'reminder' ? 'border-border text-muted' : 'border-dashed border-border-strong text-fg'}" title={e.title}>{e.title}</span>
						{/each}
						<span class="flex flex-wrap gap-0.5 @3xl:hidden">
							{#each items.slice(0, 4) as a (a.id)}<span class="size-1.5 rounded-full" style="background: var(--cat-{a.category})"></span>{/each}
						</span>
						<span class="hidden flex-col gap-0.5 @3xl:flex">
							{#each items.slice(0, 3) as a (a.id)}
								<span class="truncate border-l-3 px-1 text-xs leading-4" style="border-left-color: var(--cat-{a.category}); background: color-mix(in oklch, var(--cat-{a.category}) 8%, var(--bg))">{a.title}</span>
							{/each}
							{#if items.length > 3}<span class="px-1 text-xs text-muted">+{items.length - 3} more</span>{/if}
						</span>
					</button>
				{/each}
			</div>
		</div>
	{:else if view === 'week'}
		<div class="hidden @3xl:block"><CalendarGrid days={week} actions={visible} {busy} allDay={events} {selected} onopen={onedit ? openAction : undefined} onedit={onedit ? edit : undefined} onpick={onchoose ? (id) => (choosing = id) : undefined} oncreate={oncreate ? addAt : undefined} onday={openDay} /></div>
		<div class="@3xl:hidden"><CalendarGrid compact days={week} actions={visible} {busy} allDay={events} {selected} onopen={onedit ? openAction : undefined} onedit={onedit ? edit : undefined} onpick={onchoose ? (id) => (choosing = id) : undefined} oncreate={oncreate ? addAt : undefined} onday={openDay} /></div>
	{:else}
		{#if dayTop}{@render dayTop(anchor)}{/if}
		<CalendarGrid days={[anchor]} actions={visible} {busy} allDay={events} {selected} onopen={onedit ? openAction : undefined} onedit={onedit ? edit : undefined} onpick={onchoose ? (id) => (choosing = id) : undefined} oncreate={oncreate ? addAt : undefined} />
	{/if}
	</div>
</div>
{#if adding}
	<ActionSheet action={adding} {inline} {goals} oncreate={create} onclose={() => (adding = null)} />
{:else if open}
	<ActionSheet action={open} {confirming} {inline} {onstatus} onsave={edit} {onlock} ondelete={remove} onclose={() => (editing = null)} />
{/if}
