<script lang="ts">
	import Flame from '@lucide/svelte/icons/flame';
	import Segmented from '$lib/ui/components/Segmented.svelte';
	import StatusChip from '$lib/ui/components/StatusChip.svelte';
	import { daysBetween, formatDate } from '$lib/ui/format';
	import { goals, onTrack, weekStarts, type DashGoal } from './dashboard';

	/**
	 * Proposed Dashboard tab (Q69): filter chips (All, each category, On track, Behind) over every
	 * section; combined tiles; on track vs behind per goal with done vs planned to date and the
	 * projected finish; minutes per week stacked by goal or category with a tap readout; streaks and
	 * completion; and each goal's tracker trend as a small line. `filter` and `group` set the start.
	 */
	let { filter = 'All', group = 'goal' }: { filter?: string; group?: 'goal' | 'category' } = $props();

	const day = (ymd: string) => formatDate(ymd, { day: 'numeric', month: 'short' });
	const CATS = [...new Map(goals.map((g) => [g.category, g.cat])).entries()];
	const CHIPS = ['All', ...CATS.map(([c]) => c), 'On track', 'Behind'];
	let chip = $derived(filter);
	let by = $derived<'goal' | 'category'>(group);
	let week = $state(weekStarts.length - 1);
	let width = $state(320);

	const shown = $derived(goals.filter((g) => chip === 'All' || g.category === chip || (chip === 'On track' && onTrack(g)) || (chip === 'Behind' && !onTrack(g))));
	const done = $derived(shown.reduce((s, g) => s + g.done, 0));
	const planned = $derived(shown.reduce((s, g) => s + g.planned, 0));
	const mean = (f: (g: DashGoal) => number) => (shown.length ? Math.round(shown.reduce((s, g) => s + f(g), 0) / shown.length) : 0);
	const best = $derived(shown.reduce<DashGoal | null>((b, g) => (!b || g.streak > b.streak ? g : b), null));
	const series = $derived(
		by === 'goal'
			? shown.map((g) => ({ key: g.id, label: g.title, color: `var(--series-${g.series})`, minutes: g.minutes }))
			: CATS.filter(([c]) => shown.some((g) => g.category === c)).map(([c, n]) => ({ key: c, label: c, color: `var(--cat-${n})`, minutes: weekStarts.map((_, i) => shown.filter((g) => g.category === c).reduce((s, g) => s + g.minutes[i], 0)) }))
	);
	const totals = $derived(weekStarts.map((_, i) => series.reduce((s, x) => s + x.minutes[i], 0)));
	const H = 140;
	const AXIS = 32;
	const BASE = H - 20;
	const max = $derived(Math.max(60, Math.ceil(Math.max(...totals) / 100) * 100));
	const y = (v: number) => BASE - (v / max) * (BASE - 8);
	const slot = $derived((width - AXIS) / weekStarts.length);
	const barW = $derived(Math.min(28, slot * 0.55));

	/** Stacked segments of one week's bar, bottom up, each 2px shorter so a surface gap separates them. */
	function stack(i: number) {
		let acc = 0;
		return series
			.filter((s) => s.minutes[i] > 0)
			.map((s) => {
				const y0 = y(acc);
				acc += s.minutes[i];
				return { ...s, top: y(acc), h: Math.max(0, y0 - y(acc) - 2) };
			});
	}

	/** SVG points of a tracker trend inside a w x h box with 4px padding, plus the last point. */
	function line(points: number[], target: number | undefined, w: number, h: number) {
		const lo = Math.min(...points, target ?? Infinity);
		const hi = Math.max(...points, target ?? -Infinity);
		const sy = (v: number) => h - 4 - ((v - lo) / Math.max(1, hi - lo)) * (h - 8);
		const xy = points.map((v, i) => [4 + (i / (points.length - 1)) * (w - 8), sy(v)] as const);
		return { path: xy.map(([a, b]) => `${a},${b}`).join(' '), end: xy[xy.length - 1], target: target === undefined ? null : sy(target) };
	}
</script>

<div class="flex flex-col gap-5 p-4">
	<div class="-mx-4 flex gap-2 overflow-x-auto px-4" role="radiogroup" aria-label="Filter goals">
		{#each CHIPS as c (c)}
			<button type="button" role="radio" aria-checked={chip === c} {@attach (el) => { if (chip === c) el.scrollIntoView({ inline: 'nearest', block: 'nearest' }); }} class="min-h-9 shrink-0 rounded-sm border px-3 text-sm font-medium outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {chip === c ? 'border-fg bg-fg text-bg' : 'border-border bg-bg text-fg'}" onclick={() => (chip = c)}>{c}</button>
		{/each}
	</div>

	<dl class="grid grid-cols-2 gap-px overflow-hidden rounded-sm border border-border bg-border">
		<div class="flex flex-col gap-0.5 bg-bg px-3 py-2"><dt class="text-xs text-muted">On track</dt><dd class="text-lg font-semibold tabular-nums">{shown.filter(onTrack).length} of {shown.length}</dd></div>
		<div class="flex flex-col gap-0.5 bg-bg px-3 py-2"><dt class="text-xs text-muted">Done to date</dt><dd class="text-lg font-semibold tabular-nums">{done} of {planned}<span class="ml-1 text-sm font-normal text-muted">{planned ? Math.round((done / planned) * 100) : 0}%</span></dd></div>
		<div class="flex flex-col gap-0.5 bg-bg px-3 py-2"><dt class="text-xs text-muted">Completion</dt><dd class="text-lg font-semibold tabular-nums">{mean((g) => g.week)}%<span class="ml-1 text-sm font-normal text-muted">week · {mean((g) => g.month)}% month</span></dd></div>
		<div class="flex flex-col gap-0.5 bg-bg px-3 py-2"><dt class="text-xs text-muted">Best streak</dt><dd class="flex items-center gap-1 text-lg font-semibold tabular-nums"><Flame class="size-4 text-muted" aria-hidden="true" />{best?.streak ?? 0} days</dd></div>
	</dl>

	<section class="flex flex-col gap-2" aria-labelledby="dash-goals">
		<h2 id="dash-goals" class="text-sm font-semibold">Goals</h2>
		<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
			{#each shown as g (g.id)}
				{@const ok = onTrack(g)}
				{@const gap = daysBetween(g.target, g.finish)}
				<li class="flex flex-col gap-1.5 px-3 py-2.5">
					<div class="flex items-center gap-2">
						<span class="size-2 shrink-0 rounded-full" style="background: var(--cat-{g.cat})" aria-hidden="true"></span>
						<p class="min-w-0 flex-1 truncate text-sm font-medium">{g.title}</p>
						<StatusChip tone={ok ? 'done' : 'warn'} label={ok ? 'On track' : 'Behind'} />
					</div>
					<div class="flex h-1.5 gap-0.5 overflow-hidden rounded-full" role="img" aria-label="{g.done} of {g.planned} planned sessions done">
						<span class="rounded-l-full bg-fg" style="width: {(g.done / g.planned) * 100}%"></span>
						<span class="flex-1 rounded-r-full bg-surface-2"></span>
					</div>
					<p class="flex justify-between text-xs text-muted tabular-nums">
						<span>{g.done} of {g.planned} done to date</span>
						<span>Finish {day(g.finish)}{gap > 0 ? `, ${gap} days late` : gap < 0 ? `, ${-gap} days early` : ''}</span>
					</p>
				</li>
			{:else}
				<li class="px-3 py-3 text-sm text-muted">No goals match this filter.</li>
			{/each}
		</ul>
	</section>

	<section class="flex flex-col gap-2 rounded-sm border border-border bg-bg p-3" aria-labelledby="dash-minutes">
		<div class="flex items-center justify-between gap-2">
			<h2 id="dash-minutes" class="text-sm font-semibold">Minutes per week</h2>
			<Segmented label="Stack by" compact bind:value={by} options={[{ value: 'goal', label: 'Goal' }, { value: 'category', label: 'Category' }]} />
		</div>
		<div bind:clientWidth={width} class="w-full">
			<svg {width} height={H} role="img" aria-label="Minutes done per week, last 6 weeks, latest {totals[totals.length - 1]} min">
				{#each [0, max / 2, max] as t (t)}
					<line x1={AXIS} x2={width} y1={y(t)} y2={y(t)} class="stroke-border" stroke-width="1" />
					<text x={AXIS - 6} y={y(t)} dy="0.32em" text-anchor="end" class="fill-muted text-xs tabular-nums">{t}</text>
				{/each}
				{#each weekStarts as w, i (w)}
					{@const cx = AXIS + slot * i + slot / 2}
					{@const segs = stack(i)}
					<clipPath id="bar-{i}"><rect x={cx - barW / 2} y={segs.at(-1)?.top ?? BASE} width={barW} height={BASE - (segs.at(-1)?.top ?? BASE) + 4} rx="4" /></clipPath>
					<g clip-path="url(#bar-{i})">
						{#each segs as s (s.key)}<rect x={cx - barW / 2} y={s.top} width={barW} height={s.h} fill={s.color} opacity={i === week ? 1 : 0.55} />{/each}
					</g>
					<text x={cx} y={H - 4} text-anchor="middle" class="text-xs tabular-nums {i === week ? 'fill-fg' : 'fill-muted'}">{Number(w.slice(8))}/{Number(w.slice(5, 7))}</text>
					<rect x={cx - slot / 2} y="0" width={slot} height={H} fill="transparent" role="button" tabindex="0" aria-label="Week of {day(w)}, {totals[i]} min" onclick={() => (week = i)} onkeydown={(e) => e.key === 'Enter' && (week = i)} />
				{/each}
			</svg>
		</div>
		<p class="text-xs text-muted tabular-nums">Week of {day(weekStarts[week])} · <span class="font-medium text-fg">{totals[week]} min</span></p>
		<ul class="flex flex-col gap-1" aria-label="Legend">
			{#each series as s (s.key)}
				<li class="flex items-center gap-2 text-xs"><span class="size-2.5 shrink-0 rounded-sm" style="background: {s.color}" aria-hidden="true"></span><span class="min-w-0 flex-1 truncate">{s.label}</span><span class="text-muted tabular-nums">{s.minutes[week]} min</span></li>
			{/each}
		</ul>
	</section>

	<section class="flex flex-col gap-2" aria-labelledby="dash-trends">
		<h2 id="dash-trends" class="text-sm font-semibold">Tracker trends, last 7 weeks</h2>
		<ul class="grid grid-cols-2 gap-px overflow-hidden rounded-sm border border-border bg-border">
			{#each shown as g (g.id)}
				{@const l = line(g.tracker.points, g.tracker.target, 150, 44)}
				{@const last = g.tracker.points[g.tracker.points.length - 1]}
				<li class="flex flex-col gap-1 bg-bg px-3 py-2">
					<p class="truncate text-xs text-muted">{g.tracker.label}</p>
					<p class="text-md font-semibold tabular-nums">{last}{g.tracker.unit ? ` ${g.tracker.unit}` : ''}{#if g.tracker.target}<span class="ml-1 text-xs font-normal text-muted">of {g.tracker.target}</span>{/if}</p>
					<svg viewBox="0 0 150 44" class="h-11 w-full" preserveAspectRatio="none" role="img" aria-label="{g.tracker.label} trend for {g.title}, latest {last}">
						{#if l.target !== null}<line x1="0" x2="150" y1={l.target} y2={l.target} class="stroke-muted" stroke-width="1" stroke-dasharray="3 3" vector-effect="non-scaling-stroke" />{/if}
						<polyline points={l.path} fill="none" stroke="var(--series-{g.series})" stroke-width="2" stroke-linejoin="round" stroke-linecap="round" vector-effect="non-scaling-stroke" />
					</svg>
					<p class="flex items-center gap-1.5 truncate text-xs"><span class="size-2 shrink-0 rounded-full" style="background: var(--series-{g.series})" aria-hidden="true"></span>{g.title}</p>
				</li>
			{/each}
		</ul>
		<p class="flex items-center gap-2 text-xs text-muted"><span class="inline-block w-4 border-t border-dashed border-muted"></span>Target</p>
	</section>
</div>
