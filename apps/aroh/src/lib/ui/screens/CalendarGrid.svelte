<script lang="ts">
	import AllDayChip from '../components/planning/AllDayChip.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDate, formatTime, formatYmd, minuteOfDay } from '$lib/ui/format';
	import { allDayOn, shiftMinutes } from './edits';
	import type { ActionView } from '$lib/ui/contract';
	import type { ActionEdit, AllDayView, BusyView } from './types';

	/**
	 * Hour grid for one or more days, like a plain web calendar: an all-day row when any device
	 * all-day event falls in range (a chip tap calls `onpick`), hour rows from `startHour` to
	 * `endHour`, actions as light cards with a category edge, busy device events as neutral grey
	 * blocks, Busy days shaded, today tinted, and a now line. Tapping an action calls `onopen`; the
	 * `selected` action drags at once, any other after a long press, and a drop calls `onedit`.
	 */
	let {
		days,
		actions,
		busy = [],
		allDay = [],
		selected = null,
		startHour = 6,
		endHour = 22,
		onopen,
		onedit,
		onpick
	}: {
		days: string[];
		actions: ActionView[];
		busy?: BusyView[];
		allDay?: AllDayView[];
		selected?: string | null;
		startHour?: number;
		endHour?: number;
		onopen?: (id: string) => void;
		onedit?: (edit: ActionEdit) => void;
		onpick?: (id: string) => void;
	} = $props();

	const { now, zone } = getClock();
	const HOUR = 48;
	const SNAP = 15;
	const LONG_PRESS_MS = 350;
	const SLOP_PX = 6;
	const today = formatYmd(now, zone);
	const hours = $derived(Array.from({ length: endHour - startHour + 1 }, (_, i) => startHour + i));
	const height = $derived((endHour - startHour) * HOUR);

	/** A press on an action: `live` once it may drag, `delta` the snapped minutes moved so far. */
	type Press = { id: string; mode: 'move' | 'resize'; y0: number; delta: number; live: boolean };
	let press = $state<Press | null>(null);
	let timer: ReturnType<typeof setTimeout> | undefined;
	let grid = $state<HTMLElement | null>(null);

	$effect(() => {
		const el = grid;
		if (!el) return;
		const hold = (e: TouchEvent) => {
			if (press?.live) e.preventDefault();
		};
		el.addEventListener('touchmove', hold, { passive: false });
		return () => el.removeEventListener('touchmove', hold);
	});

	/** Pixel offset of a minute of day from the top of the grid, clamped to the visible hours. */
	function top(minute: number): number {
		return Math.min(height, Math.max(0, ((minute - startHour * 60) / 60) * HOUR));
	}

	const columns = $derived(
		days.map((date) => {
			const events = allDayOn(allDay, date);
			return {
				date,
				name: formatDate(date, { weekday: 'short' }),
				num: Number(date.slice(8)),
				events,
				blocked: events.find((e) => e.kind === 'busy') ?? null,
				items: actions
					.filter((a) => formatYmd(a.start, zone) === date)
					.map((a) => {
						const y = top(minuteOfDay(a.start, zone));
						return { ...a, y, h: Math.max(22, top(minuteOfDay(a.end, zone)) - y) };
					}),
				blocks: busy
					.filter((b) => formatYmd(b.start, zone) === date)
					.map((b) => {
						const y = top(minuteOfDay(b.start, zone));
						return { ...b, y, h: Math.max(22, top(minuteOfDay(b.end, zone)) - y) };
					})
			};
		})
	);
	const showAllDay = $derived(columns.some((c) => c.events.length > 0));
	const nowY = top(minuteOfDay(now, zone));

	/** Starts a press on an action body (`move`) or its bottom edge (`resize`). */
	function down(e: PointerEvent, id: string, mode: Press['mode']) {
		e.stopPropagation();
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		const live = Boolean(onedit) && (mode === 'resize' || id === selected);
		press = { id, mode, y0: e.clientY, delta: 0, live };
		clearTimeout(timer);
		if (!live && onedit) timer = setTimeout(() => press && (press.live = true), LONG_PRESS_MS);
	}

	/** Tracks the pointer: snaps a live drag to 15 minutes, drops a press that turns into a scroll. */
	function drag(e: PointerEvent) {
		if (!press) return;
		const dy = e.clientY - press.y0;
		if (!press.live) {
			if (Math.abs(dy) > SLOP_PX) cancel();
			return;
		}
		press.delta = Math.round(((dy / HOUR) * 60) / SNAP) * SNAP;
	}

	/** Ends a press: a drop with movement reports the edit, a plain tap opens the action. */
	function up(a: ActionView) {
		const p = press;
		cancel();
		if (!p || p.id !== a.id) return;
		if (p.live && p.delta !== 0) {
			const end = shiftMinutes(a.end, p.delta);
			const start = p.mode === 'move' ? shiftMinutes(a.start, p.delta) : a.start;
			if (Date.parse(end) - Date.parse(start) >= SNAP * 60_000) onedit?.({ id: a.id, start, end });
			return;
		}
		onopen?.(a.id);
	}

	/** Forgets the current press and its long-press timer. */
	function cancel() {
		clearTimeout(timer);
		press = null;
	}

	/** Live position of an action while it is dragged: its shifted top, height and times. */
	function placed(a: ActionView & { y: number; h: number }) {
		const p = press?.id === a.id && press.live ? press : null;
		if (!p || p.delta === 0) return { y: a.y, h: a.h, start: a.start, end: a.end, moving: Boolean(p) };
		const dy = (p.delta / 60) * HOUR;
		const end = shiftMinutes(a.end, p.delta);
		if (p.mode === 'resize') return { y: a.y, h: Math.max(HOUR / 4, a.h + dy), start: a.start, end, moving: true };
		return { y: a.y + dy, h: a.h, start: shiftMinutes(a.start, p.delta), end, moving: true };
	}
</script>

<div bind:this={grid} class="flex flex-col overflow-hidden rounded-md border border-border bg-bg">
	<div class="grid border-b border-border" style="grid-template-columns: 52px repeat({days.length}, minmax(0, 1fr))">
		<div></div>
		{#each columns as c (c.date)}
			<div class="flex flex-col items-center gap-0.5 border-l border-border py-2">
				<span class="text-xs text-muted">{c.name}</span>
				<span class="min-w-6 rounded-sm px-1 text-center text-sm leading-6 font-semibold tabular-nums {c.date === today ? 'bg-accent text-accent-fg' : 'text-fg'}">{c.num}</span>
			</div>
		{/each}
	</div>
	{#if showAllDay}
		<div class="grid border-b border-border" style="grid-template-columns: 52px repeat({days.length}, minmax(0, 1fr))">
			<span class="self-center pr-2 text-right text-xs text-muted">All day</span>
			{#each columns as c (c.date)}
				<ul class="flex min-w-0 flex-col gap-1 border-l border-border p-1" aria-label="{c.name} {c.num}, all-day events">
					{#each c.events as e (e.id)}<li class="min-w-0"><AllDayChip event={e} detail={days.length === 1} {onpick} /></li>{/each}
				</ul>
			{/each}
		</div>
	{/if}
	{#if days.length === 1 && columns[0]?.blocked}
		<p class="border-b border-border bg-surface-2 px-3 py-1.5 text-center text-xs text-muted">{columns[0].blocked.title} is a busy day.</p>
	{/if}
	<div class="relative grid" style="grid-template-columns: 52px repeat({days.length}, minmax(0, 1fr)); height: {height}px">
		<div class="relative">
			{#each hours.slice(0, -1) as h (h)}
				<span class="absolute right-2 text-xs text-muted tabular-nums {h === startHour ? 'translate-y-0.5' : '-translate-y-1/2'}" style="top: {(h - startHour) * HOUR}px">{`${String(h).padStart(2, '0')}:00`}</span>
			{/each}
		</div>
		{#each columns as c (c.date)}
			<div class="relative border-l border-border {c.blocked ? 'bg-surface-2' : ''}" role="list" aria-label="{c.name} {c.num}, {c.blocked ? `busy day, ${c.blocked.title}, ` : ''}{c.items.length} actions">
				{#each hours.slice(1, -1) as h (h)}
					<div class="absolute inset-x-0 border-t border-border/70" style="top: {(h - startHour) * HOUR}px"></div>
				{/each}
				{#each c.blocks as b (b.id)}
					<div
						role="listitem"
						class="absolute inset-x-1 overflow-hidden rounded-sm border border-l-3 border-border bg-surface px-1.5 py-0.5"
						style="top: {b.y + 1}px; height: {b.h - 2}px; border-left-color: var(--muted)"
						title="{b.title}, {b.group}, {formatTime(b.start, zone)} - {formatTime(b.end, zone)}"
					>
						<p class="truncate text-xs leading-4 font-medium text-muted">{b.title}</p>
						{#if b.h >= 36}<p class="truncate text-xs leading-4 text-muted tabular-nums">Busy · {formatTime(b.start, zone)} - {formatTime(b.end, zone)}</p>{/if}
					</div>
				{/each}
				{#each c.items as a (a.id)}
					{@const at = placed(a)}
					{@const active = a.id === selected || at.moving}
					{#if at.moving}
						<div class="absolute inset-x-1 rounded-sm border border-dashed border-border-strong" style="top: {a.y + 1}px; height: {a.h - 2}px" aria-hidden="true"></div>
					{/if}
					<div
						role="listitem"
						class="absolute inset-x-1 overflow-hidden rounded-sm border border-l-3 px-1.5 py-0.5 outline-none select-none {active ? 'z-20 ring-2 ring-fg' : ''} {a.status === 'missed' ? 'border-dashed border-status-missed' : 'border-border'} {onopen ? 'cursor-pointer' : ''}"
						style="top: {at.y + 1}px; height: {at.h - 2}px; border-left-color: var(--cat-{a.category}); border-left-style: solid; background: color-mix(in oklch, var(--cat-{a.category}) {active ? 16 : 8}%, var(--bg)); touch-action: {a.id === selected ? 'none' : 'pan-y'}"
						title="{a.title}, {formatTime(at.start, zone)} - {formatTime(at.end, zone)}"
						onpointerdown={(e) => down(e, a.id, 'move')}
						onpointermove={drag}
						onpointerup={() => up(a)}
						onpointercancel={cancel}
					>
						<p class="truncate text-xs leading-4 font-medium {a.status === 'missed' ? 'text-status-missed line-through' : a.status === 'done' ? 'text-muted' : 'text-fg'}">{a.title}</p>
						{#if at.h >= 36}<p class="truncate text-xs leading-4 text-muted tabular-nums">{#if a.status === 'missed'}<span class="font-medium text-status-missed">Missed</span> · {/if}<span class={at.moving ? 'font-medium text-fg' : ''}>{formatTime(at.start, zone)} - {formatTime(at.end, zone)}</span></p>{/if}
						{#if onopen}
							<button type="button" class="sr-only" onclick={() => onopen?.(a.id)}>Edit {a.title}</button>
						{/if}
						{#if active && onedit}
							<span
								class="absolute inset-x-0 bottom-0 flex h-3 cursor-ns-resize items-end justify-center pb-0.5"
								style="touch-action: none"
								aria-hidden="true"
								onpointerdown={(e) => down(e, a.id, 'resize')}
								onpointermove={drag}
								onpointerup={() => up(a)}
								onpointercancel={cancel}
							>
								<span class="h-1 w-6 rounded-full bg-fg"></span>
							</span>
						{/if}
					</div>
				{/each}
				{#if c.date === today}
					<div class="pointer-events-none absolute inset-x-0 z-10 border-t border-accent" style="top: {nowY}px" aria-label="Now, {formatTime(now, zone)}">
						<span class="absolute -top-[4px] -left-px h-1.5 w-1.5 bg-accent"></span>
					</div>
				{/if}
			</div>
		{/each}
	</div>
</div>
