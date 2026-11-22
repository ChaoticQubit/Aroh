<script lang="ts">
	import { getClock } from '$lib/ui/clock';
	import { formatDate, formatTime, formatYmd, minuteOfDay } from '$lib/ui/format';
	import type { ActionView } from '$lib/ui/contract';
	import type { BusyView } from './types';

	/**
	 * Hour grid for one or more days, like a plain web calendar: hour rows from `startHour` to
	 * `endHour`, actions as light cards with a category edge, busy device events as neutral grey
	 * blocks, today tinted, and a now line.
	 */
	let { days, actions, busy = [], startHour = 6, endHour = 22 }: { days: string[]; actions: ActionView[]; busy?: BusyView[]; startHour?: number; endHour?: number } = $props();

	const { now, zone } = getClock();
	const HOUR = 48;
	const today = formatYmd(now, zone);
	const hours = $derived(Array.from({ length: endHour - startHour + 1 }, (_, i) => startHour + i));
	const height = $derived((endHour - startHour) * HOUR);

	/** Pixel offset of a minute of day from the top of the grid, clamped to the visible hours. */
	function top(minute: number): number {
		return Math.min(height, Math.max(0, ((minute - startHour * 60) / 60) * HOUR));
	}

	const columns = $derived(
		days.map((date) => ({
			date,
			name: formatDate(date, { weekday: 'short' }),
			num: Number(date.slice(8)),
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
		}))
	);
	const nowY = top(minuteOfDay(now, zone));
</script>

<div class="flex flex-col overflow-hidden rounded-md border border-border bg-bg">
	<div class="grid border-b border-border" style="grid-template-columns: 52px repeat({days.length}, minmax(0, 1fr))">
		<div></div>
		{#each columns as c (c.date)}
			<div class="flex flex-col items-center gap-0.5 border-l border-border py-2">
				<span class="text-xs text-muted">{c.name}</span>
				<span class="min-w-6 rounded-sm px-1 text-center text-sm leading-6 font-semibold tabular-nums {c.date === today ? 'bg-accent text-accent-fg' : 'text-fg'}">{c.num}</span>
			</div>
		{/each}
	</div>
	<div class="relative grid" style="grid-template-columns: 52px repeat({days.length}, minmax(0, 1fr)); height: {height}px">
		<div class="relative">
			{#each hours.slice(0, -1) as h (h)}
				<span class="absolute right-2 -translate-y-1/2 text-xs text-muted tabular-nums" style="top: {(h - startHour) * HOUR}px">{h === startHour ? '' : `${String(h).padStart(2, '0')}:00`}</span>
			{/each}
		</div>
		{#each columns as c (c.date)}
			<div class="relative border-l border-border" role="list" aria-label="{c.name} {c.num}, {c.items.length} actions">
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
					<div
						role="listitem"
						class="absolute inset-x-1 overflow-hidden rounded-sm border border-l-3 px-1.5 py-0.5 {a.status === 'missed' ? 'border-dashed border-status-missed' : 'border-border'}"
						style="top: {a.y + 1}px; height: {a.h - 2}px; border-left-color: var(--cat-{a.category}); border-left-style: solid; background: color-mix(in oklch, var(--cat-{a.category}) 8%, var(--bg))"
						title="{a.title}, {formatTime(a.start, zone)} - {formatTime(a.end, zone)}"
					>
						<p class="truncate text-xs leading-4 font-medium {a.status === 'missed' ? 'text-status-missed line-through' : a.status === 'done' ? 'text-muted' : 'text-fg'}">{a.title}</p>
						{#if a.h >= 36}<p class="truncate text-xs leading-4 text-muted tabular-nums">{#if a.status === 'missed'}<span class="font-medium text-status-missed">Missed</span> · {/if}{formatTime(a.start, zone)} - {formatTime(a.end, zone)}</p>{/if}
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
