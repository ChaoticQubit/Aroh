<script lang="ts" module>
	import type { Action, BusyBlock } from '$lib/app/client';

	/** What the plan card needs from the device: busy time, other actions, this goal, its date and how far the calendar is known. */
	export type PlanContext = { busy: BusyBlock[]; actions: Action[]; goalId: string | null; deadline: string | null; knownUntil: string | null };
</script>

<script lang="ts">
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import Flag from '@lucide/svelte/icons/flag';
	import Lock from '@lucide/svelte/icons/lock';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import { Switch } from '$lib/ui/primitives/switch';
	import Persona from '../agent/Persona.svelte';
	import { getClock } from '$lib/ui/clock';
	import { daysBetween, formatDate, formatYmd, shiftDay } from '$lib/ui/format';
	import { lockAxis, swipeStep } from '$lib/ui/swipe';
	import { busyStrip, columns, planBlocks, scaleFor, summarize, summaryLine, weeks, type PlanItemLike, type Placed } from '$lib/ui/plan/strip';

	/**
	 * The plan to approve (Q65): a one-line summary, a 7-day strip with the proposed sessions as
	 * category-tinted blocks and busy time, busy days and fixed actions in neutral grey, arrows and a
	 * sideways swipe to page weeks, a detail row for the tapped block with the Fixed time switch on
	 * one-off sessions (Q53), and Approve and Change. `onapprove` gets the fixed item indexes.
	 */
	let {
		title,
		plan,
		context = { busy: [], actions: [], goalId: null, deadline: null, knownUntil: null },
		slot = 1,
		approved = false,
		onapprove,
		onchange
	}: {
		title: string;
		plan: { items: PlanItemLike[]; timezone: string | null };
		context?: PlanContext;
		slot?: number;
		approved?: boolean;
		onapprove?: (fixed: number[]) => void;
		onchange?: () => void;
	} = $props();

	const HORIZON_DAYS = 366;
	const PX = { hourPx: 11, minPx: 12 };
	const clock = getClock();
	const zone = $derived(plan.timezone ?? clock.zone);
	const today = $derived(formatYmd(clock.now, zone));
	const own = $derived(planBlocks(plan.items, HORIZON_DAYS));
	const others = $derived(busyStrip(context.busy, context.actions, context.goalId, zone));
	const sum = $derived(summarize(own.blocks));
	const pages = $derived(weeks(own.blocks[0]?.date ?? today, sum.finish ?? today));
	const hours = $derived(scaleFor([...own.blocks, ...others.blocks.filter((b) => b.date >= pages[0] && b.date <= shiftDay(pages[pages.length - 1], 6))]));
	const height = $derived((hours.endHour - hours.startHour) * PX.hourPx);
	let page = $state(0);
	let picked = $state<string | null>(null);
	let fixed = $state<number[]>([]);
	const week = $derived(pages[Math.min(page, pages.length - 1)]);
	const cols = $derived(columns(week, [...others.blocks, ...own.blocks], others.busyDays, hours, PX, today, sum.finish));
	const detail = $derived(picked ? (cols.flatMap((c) => c.blocks).find((b) => b.id === picked) ?? null) : null);
	const pickedDay = $derived(picked?.startsWith('day-') ? cols.find((c) => `day-${c.date}` === picked)?.busy : null);
	const range = $derived(`${formatDate(week, { day: 'numeric', month: 'short' })} - ${formatDate(shiftDay(week, 6), { day: 'numeric', month: 'short' })}`);
	const early = $derived(context.deadline && sum.finish ? daysBetween(sum.finish, context.deadline) : null);
	const unknown = $derived(context.knownUntil && shiftDay(week, 6) > context.knownUntil ? context.knownUntil : null);
	const ticks = $derived([hours.startHour, 12, 18].filter((h) => h >= hours.startHour && h < hours.endHour));
	const tint = $derived(`border-left-color: var(--cat-${slot}); background: color-mix(in oklab, var(--cat-${slot}) 55%, var(--bg))`);
	let press: { x: number; y: number; t: number; axis: 'x' | 'y' | null } | null = null;

	/** Pages one week back or forward within the plan's weeks and clears the tapped block. */
	function go(delta: number) {
		page = Math.min(pages.length - 1, Math.max(0, page + delta));
		picked = null;
	}

	/** Ends a press on the strip: a sideways swipe pages a week (shared swipe rules), anything else is left alone. */
	function release(e: PointerEvent) {
		if (!press) return;
		const dx = e.clientX - press.x;
		const width = (e.currentTarget as HTMLElement).clientWidth || 1;
		if ((press.axis ?? lockAxis(dx, e.clientY - press.y)) === 'x') {
			const step = swipeStep(dx, width, dx / Math.max(1, e.timeStamp - press.t));
			if (step) go(step);
		}
		press = null;
	}

	/** Toggles Fixed time for plan item `item`. */
	function setFixed(item: number, on: boolean) {
		fixed = on ? [...new Set([...fixed, item])] : fixed.filter((i) => i !== item);
	}

	/** One-line time span of a block, "Thu 26 Nov · 18:00 - 18:20". */
	function when(b: Placed): string {
		const hm = (m: number) => `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
		return `${formatDate(b.date, { weekday: 'short', day: 'numeric', month: 'short' })} · ${hm(b.start)} - ${hm(b.end)}`;
	}
</script>

<Card.Root size="sm" class="gap-3 px-4">
	<Persona name="Orchestrator" note={approved ? 'Plan approved' : 'Plan ready'} />
	<div class="flex flex-col gap-0.5">
		<p class="text-md font-semibold tabular-nums">{summaryLine(sum)}</p>
		<p class="text-sm text-muted tabular-nums">{title}{sum.sessions ? ` ${sum.sessions} sessions.` : ''}{early !== null && early > 0 ? ` ${early} days before your date.` : early !== null && early < 0 ? ` ${-early} days after your date.` : ''}</p>
	</div>

	<div class="-my-1 flex items-center justify-between gap-2">
		<Button variant="ghost" size="icon-touch" aria-label="Previous week" disabled={page === 0} onclick={() => go(-1)}><ChevronLeft /></Button>
		<p class="flex flex-col items-center text-sm font-medium tabular-nums">{range}<span class="text-xs font-normal text-muted">Week {page + 1} of {pages.length}</span></p>
		<Button variant="ghost" size="icon-touch" aria-label="Next week" disabled={page >= pages.length - 1} onclick={() => go(1)}><ChevronRight /></Button>
	</div>

	<div
		class="grid touch-pan-y select-none"
		style="grid-template-columns: 22px repeat(7, minmax(0, 1fr))"
		role="group"
		aria-label="Week of {range}; swipe for other weeks"
		onpointerdown={(e) => (press = { x: e.clientX, y: e.clientY, t: e.timeStamp, axis: null })}
		onpointermove={(e) => press && !press.axis && (press.axis = lockAxis(e.clientX - press.x, e.clientY - press.y))}
		onpointerup={release}
		onpointercancel={() => (press = null)}
	>
		<span></span>
		{#each cols as c (c.date)}
			<span class="flex flex-col items-center pb-1 text-xs {c.past ? 'text-muted/60' : 'text-muted'}">
				{formatDate(c.date, { weekday: 'narrow' })}
				<span class="flex min-w-6 items-center justify-center gap-0.5 rounded-sm px-0.5 leading-5 font-semibold tabular-nums {c.date === today ? 'bg-accent text-accent-fg' : c.past ? '' : 'text-fg'}">{Number(c.date.slice(8))}{#if c.finish}<Flag class="size-3" aria-label="Finish day" />{/if}</span>
			</span>
		{/each}
		<div class="relative" style="height: {height}px">
			{#each ticks as h (h)}
				<span class="absolute right-1 text-xs leading-3 text-muted tabular-nums" style="top: {(h - hours.startHour) * PX.hourPx}px">{h}</span>
			{/each}
		</div>
		{#each cols as c (c.date)}
			<div class="relative border-l border-border {c.busy ? 'bg-surface-2' : ''} {c.past ? 'opacity-50' : ''}" style="height: {height}px" role="group" aria-label="{formatDate(c.date, { weekday: 'long', day: 'numeric', month: 'short' })}{c.busy ? `, busy day, ${c.busy.title}` : ''}">
				{#each [12, 18].filter((h) => h > hours.startHour && h < hours.endHour) as h (h)}
					<div class="absolute inset-x-0 border-t border-border/70" style="top: {(h - hours.startHour) * PX.hourPx}px"></div>
				{/each}
				{#if c.busy}
					<button type="button" class="absolute inset-x-0.5 top-1 bottom-1 flex items-center justify-center rounded-sm border border-dashed border-border-strong outline-none {picked === `day-${c.date}` ? 'ring-2 ring-fg' : ''}" aria-label="Busy day: {c.busy.title}" onclick={() => (picked = `day-${c.date}`)}>
						<span class="text-xs text-muted [writing-mode:vertical-rl]">Busy day</span>
					</button>
				{/if}
				{#each c.blocks as b (b.id)}
					<button
						type="button"
						class="absolute inset-x-0.5 flex items-start justify-center overflow-hidden rounded-sm border border-l-3 border-border outline-none {b.kind === 'plan' ? '' : 'bg-surface-2'} {b.id === picked ? 'z-10 ring-2 ring-fg' : ''}"
						style="top: {b.y}px; height: {b.h}px; {b.kind === 'plan' ? tint : 'border-left-color: var(--muted)'}"
						aria-label="{b.kind === 'plan' ? '' : b.kind === 'fixed' ? 'Fixed: ' : 'Busy: '}{b.title}, {when(b)}"
						onclick={() => (picked = b.id)}
					>
						{#if b.kind === 'fixed'}<Lock class="mt-px size-2.5 text-muted" aria-hidden="true" />{/if}
					</button>
				{/each}
			</div>
		{/each}
	</div>

	<ul class="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted" aria-label="Legend">
		<li class="flex items-center gap-1.5"><span class="h-3 w-2.5 rounded-sm border border-l-3 border-border" style={tint}></span>This plan</li>
		<li class="flex items-center gap-1.5"><span class="h-3 w-2.5 rounded-sm border border-l-3 border-border bg-surface-2" style="border-left-color: var(--muted)"></span>Busy</li>
		<li class="flex items-center gap-1.5"><Lock class="size-3" aria-hidden="true" />Fixed</li>
		<li class="flex items-center gap-1.5"><Flag class="size-3" aria-hidden="true" />Finish</li>
	</ul>
	{#if unknown}<p class="text-xs text-muted">Your calendar is checked up to {formatDate(unknown, { weekday: 'short', day: 'numeric', month: 'short' })}.</p>{/if}
	{#if own.unexpanded.length}<p class="text-xs text-muted">Not drawn: {own.unexpanded.map((i) => plan.items[i].title).join(', ')} (a repeat the strip cannot show).</p>{/if}

	<div class="flex min-h-14 flex-col justify-center gap-0.5 rounded-sm border border-border bg-bg px-3 py-2" role="status" aria-live="polite">
		{#if detail?.kind === 'plan'}
			<div class="flex items-center justify-between gap-3">
				<p class="min-w-0 truncate text-sm font-medium">{detail.title}</p>
				{#if detail.fixable && detail.item !== undefined && !approved}
					{@const item = detail.item}
					<label class="flex shrink-0 items-center gap-2 text-xs text-muted">Fixed time<Switch checked={fixed.includes(item)} aria-label="Fixed time for {detail.title}" onCheckedChange={(v) => setFixed(item, v)} /></label>
				{/if}
			</div>
			<p class="text-xs text-muted tabular-nums">{when(detail)} · {detail.end - detail.start} min</p>
			{#if detail.date === sum.finish}<p class="text-xs text-muted">Last session of the plan.</p>{/if}
		{:else if detail}
			<p class="truncate text-sm font-medium">{detail.title}{detail.kind === 'fixed' ? ' · fixed' : ''}</p>
			<p class="text-xs text-muted tabular-nums">{when(detail)}{detail.source ? ` · ${detail.source}` : ''}</p>
			<p class="text-xs text-muted">{detail.kind === 'fixed' ? 'Fixed time. No agent moves it; this plan fits around it.' : 'Busy. This plan fits around it.'}</p>
		{:else if pickedDay}
			<p class="truncate text-sm font-medium">{pickedDay.title} · busy day</p>
			<p class="text-xs text-muted">All day on your calendar. No session that day.</p>
		{:else}
			<p class="text-sm text-muted">Tap a block for details. Swipe or use the arrows for other weeks.</p>
		{/if}
	</div>

	{#if approved}
		<p class="text-sm text-muted" role="status">Approved. The plan is on your calendar.</p>
	{:else}
		<div class="flex flex-col gap-2">
			<div class="flex gap-2">
				<Button size="touch" class="flex-1 @3xl:h-8" onclick={() => onapprove?.(fixed)}>Approve plan</Button>
				<Button size="touch" variant="outline" class="flex-1 @3xl:h-8" onclick={() => onchange?.()}>Change</Button>
			</div>
			<p class="text-center text-xs text-muted">Nothing is on your calendar until you approve.</p>
		</div>
	{/if}
</Card.Root>
