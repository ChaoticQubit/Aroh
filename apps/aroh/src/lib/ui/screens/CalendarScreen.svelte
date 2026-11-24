<script lang="ts">
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import Lock from '@lucide/svelte/icons/lock';
	import { Button } from '$lib/ui/primitives/button';
	import Segmented from '../components/Segmented.svelte';
	import CategoryFilter from '../components/CategoryFilter.svelte';
	import StatusChip from '../components/StatusChip.svelte';
	import CalendarGrid from './CalendarGrid.svelte';
	import ActionSheet from './ActionSheet.svelte';
	import AllDayChip from '../components/planning/AllDayChip.svelte';
	import AllDayChoice from '../components/planning/AllDayChoice.svelte';
	import { allDayOn, applyEdits } from './edits';
	import { getClock } from '$lib/ui/clock';
	import { daysBetween, formatDate, formatTime, formatYmd, minuteOfDay, mondayOf, shiftDay, shiftMonth } from '$lib/ui/format';
	import { actionStyle } from '$lib/ui/status';
	import { stepAnchor, swipe } from '$lib/ui/swipe';
	import type { ActionView } from '$lib/ui/contract';
	import type { ActionEdit, AllDayView, BusyView, CategoryView, DayKind, DayPick } from './types';

	/**
	 * Calendar: month title with the range and its session count, Today, previous, next and the Day,
	 * Week, Month switch, category chips, then a week hour grid on wide containers or, on phones, the
	 * week as a plan strip (tap a day to pick it) over the picked day's list. Kept device
	 * events (`busy`) show in the hour grids as neutral blocks, device all-day events (`allDay`) in
	 * an all-day row and as bars in month cells and the week strip, with the kind question for the
	 * first unanswered one in view; tapping a chip reopens it. A sideways swipe pages a day, week or
	 * month like the arrows. Tapping an action opens the edit
	 * sheet; `onedit`, `onlock`, `ondelete` and `onchoose` report the changes. `initialView`, `day`, `editing`,
	 * `confirming`, `selected` and `inline` (sheet drawn in place for mockups) set the opening state.
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
		onedit,
		onlock,
		ondelete,
		onchoose
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
		onedit?: (edit: ActionEdit) => void;
		onlock?: (id: string, locked: boolean) => void;
		ondelete?: (id: string) => void;
		onchoose?: (pick: DayPick) => void;
	} = $props();

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
	const START = 6;
	const END = 22;
	const pct = (minute: number) => (Math.min(END * 60, Math.max(START * 60, minute)) - START * 60) / ((END - START) * 0.6);
	const dayItems = $derived(
		[
			...visible.filter((a) => formatYmd(a.start, zone) === anchor).map((a) => ({ kind: 'action' as const, a, start: a.start })),
			...busy.filter((b) => formatYmd(b.start, zone) === anchor).map((b) => ({ kind: 'busy' as const, b, start: b.start }))
		].sort((x, y) => Date.parse(x.start) - Date.parse(y.start))
	);

	/** Top and height in percent of the strip for one block, at least 2 percent so short blocks stay visible. */
	function span(start: string, end: string): string {
		const top = pct(minuteOfDay(start, zone));
		const bottom = formatYmd(end, zone) === formatYmd(start, zone) ? pct(minuteOfDay(end, zone)) : 100;
		return `top: ${top}%; height: max(6px, ${Math.max(2, bottom - top)}%)`;
	}

	/** Moves the anchor one day, week or month in `dir`. */
	function step(dir: 1 | -1) {
		anchor = stepAnchor(view, anchor, dir);
	}

	/** Opens the edit sheet on an action and keeps it selected for dragging after the sheet closes. */
	function openAction(id: string) {
		editing = id;
		selected = id;
	}

	/** Applies a move or resize locally and reports it. */
	function edit(e: ActionEdit) {
		edits[e.id] = e;
		editing = null;
		onedit?.(e);
	}

	/** Removes an action locally and reports it. */
	function remove(id: string) {
		deleted = [...deleted, id];
		editing = null;
		selected = null;
		ondelete?.(id);
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
						onclick={() => {
							anchor = d;
							view = 'day';
						}}
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
		<div class="hidden @3xl:block"><CalendarGrid days={week} actions={visible} {busy} allDay={events} {selected} onopen={onedit ? openAction : undefined} onedit={onedit ? edit : undefined} onpick={onchoose ? (id) => (choosing = id) : undefined} /></div>
		<div class="flex flex-col gap-4 @3xl:hidden">
			<section class="flex flex-col gap-3 rounded-sm border border-border bg-bg p-3" aria-label="Week">
				<div class="grid grid-cols-[24px_repeat(7,minmax(0,1fr))] gap-x-1">
					<span></span>
					{#each week as d (d)}
						{@const marks = allDayOn(events, d)}
						<button type="button" class="flex min-h-11 flex-col items-center gap-0.5 rounded-sm text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {d < today ? 'text-muted' : ''}" aria-pressed={d === anchor} aria-label="{formatDate(d, { weekday: 'long', day: 'numeric' })}{marks.length ? `, ${marks.map((e) => e.title).join(', ')}` : ''}" onclick={() => (anchor = d)}>
							<span class="text-xs text-muted">{formatDate(d, { weekday: 'narrow' })}</span>
							<span class="flex size-7 items-center justify-center rounded-sm font-medium tabular-nums {d === anchor ? 'bg-fg text-bg' : d === today ? 'ring-1 ring-border-strong' : ''}">{Number(d.slice(8))}</span>
							<span class="h-1 w-4 rounded-full {marks.length ? 'bg-border-strong' : ''}" aria-hidden="true"></span>
						</button>
					{/each}
				</div>
				<div class="grid h-[300px] grid-cols-[24px_repeat(7,minmax(0,1fr))] gap-x-1">
					<div class="relative">{#each [6, 12, 18] as h (h)}<span class="absolute text-xs text-muted tabular-nums" style="top: {pct(h * 60)}%">{h}</span>{/each}</div>
					{#each week as d (d)}
						<button type="button" class="relative border-l border-border outline-none focus-visible:ring-3 focus-visible:ring-ring/50" aria-label="Show {formatDate(d, { weekday: 'long', day: 'numeric' })}" onclick={() => (anchor = d)}>
							{#each busy.filter((b) => formatYmd(b.start, zone) === d) as b (b.id)}
								<span class="absolute inset-x-0.5 rounded-sm border border-l-3 border-border border-l-border-strong bg-surface-2" style={span(b.start, b.end)}></span>
							{/each}
							{#each visible.filter((a) => formatYmd(a.start, zone) === d) as a (a.id)}
								<span class="absolute inset-x-0.5 flex items-start justify-center overflow-hidden rounded-sm border-l-3 {a.status === 'planned' ? '' : 'opacity-50'}" style="{span(a.start, a.end)}; border-left-color: var(--cat-{a.category}); background: color-mix(in oklab, var(--cat-{a.category}) 55%, var(--bg))">{#if a.locked}<Lock class="mt-0.5 size-3 text-fg" aria-hidden="true" />{/if}</span>
							{/each}
						</button>
					{/each}
				</div>
				<p class="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted">
					<span class="flex items-center gap-1.5"><span class="h-2.5 w-1.5 rounded-sm" style="background: var(--cat-1)"></span>Goal sessions</span>
					<span class="flex items-center gap-1.5"><span class="h-2.5 w-1.5 rounded-sm border border-border-strong bg-surface-2"></span>Busy</span>
					<span class="flex items-center gap-1.5"><Lock class="size-3" aria-hidden="true" />Fixed</span>
				</p>
			</section>
			<section class="flex flex-col gap-2" aria-labelledby="calendar-day">
				<div class="flex items-baseline justify-between px-1">
					<h3 id="calendar-day" class="text-sm font-semibold">{formatDate(anchor, { weekday: 'long', day: 'numeric' })}</h3>
					<p class="text-sm text-muted tabular-nums">{dayItems.filter((i) => i.kind === 'action').length} actions</p>
				</div>
				{#if allDayOn(events, anchor).length}
					<ul class="flex flex-col gap-1" aria-label="All day">
						{#each allDayOn(events, anchor) as e (e.id)}<li><AllDayChip event={e} onpick={onchoose ? (id) => (choosing = id) : undefined} /></li>{/each}
					</ul>
				{/if}
				{#if dayItems.length}
					<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
						{#each dayItems as item (item.kind === 'action' ? item.a.id : item.b.id)}
							{#if item.kind === 'action'}
								{@const a = item.a}
								{@const look = actionStyle(a.status)}
								<li>
									<svelte:element
										this={onedit ? 'button' : 'div'}
										type={onedit ? 'button' : undefined}
										role={onedit ? 'button' : undefined}
										aria-label={onedit ? `Edit ${a.title}` : undefined}
										class="flex min-h-13 w-full items-center gap-3 px-4 py-2 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset"
										onclick={onedit ? () => openAction(a.id) : undefined}
									>
										<span class="w-11 shrink-0 text-sm tabular-nums">{formatTime(a.start, zone)}</span>
										<span class="flex min-w-0 flex-1 flex-col border-l-2 pl-3" style="border-color: var(--cat-{a.category})">
											<span class="truncate text-base {look.title}">{a.title}</span>
											<span class="truncate text-xs text-muted">{a.goalTitle}</span>
										</span>
										{#if a.locked}<Lock class="size-4 shrink-0 text-muted" aria-label="Locked" />{/if}
										{#if look.label && a.status !== 'planned'}<StatusChip tone={a.status} label={look.label} />{/if}
									</svelte:element>
								</li>
							{:else}
								<li class="flex min-h-13 items-center gap-3 px-4 py-2 text-muted">
									<span class="w-11 shrink-0 text-sm tabular-nums">{formatTime(item.b.start, zone)}</span>
									<span class="flex min-w-0 flex-1 flex-col border-l-2 border-border-strong pl-3">
										<span class="truncate text-base">{item.b.title}</span>
										<span class="truncate text-xs">Busy · {formatTime(item.b.start, zone)} to {formatTime(item.b.end, zone)}</span>
									</span>
								</li>
							{/if}
						{/each}
					</ul>
				{:else}
					<p class="rounded-sm border border-border bg-bg px-4 py-3 text-sm text-muted">Nothing planned this day.</p>
				{/if}
			</section>
		</div>
	{:else}
		<CalendarGrid days={[anchor]} actions={visible} {busy} allDay={events} {selected} onopen={onedit ? openAction : undefined} onedit={onedit ? edit : undefined} onpick={onchoose ? (id) => (choosing = id) : undefined} />
	{/if}
	</div>
</div>
{#if open}
	<ActionSheet action={open} {confirming} {inline} onsave={edit} {onlock} ondelete={remove} onclose={() => (editing = null)} />
{/if}
