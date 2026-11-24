<script lang="ts">
	import Segmented from '$lib/ui/components/Segmented.svelte';
	import { formatDate, formatNumber } from '$lib/ui/format';
	import { yOf } from '$lib/ui/chart';
	import { barSeries, type DashGoal } from '$lib/app/dashboard';
	import { barScale, stackWeek } from './geometry';

	/**
	 * Minutes done per week as bars stacked by goal or category, with recessive ticks, a 2px gap
	 * between segments, a tap or key readout of the picked week and a legend with that week's
	 * minutes per series. `weeks` are the Mondays, oldest first; the current week shows until a tap dims the others.
	 */
	let { goals, weeks }: { goals: DashGoal[]; weeks: string[] } = $props();

	const H = 148;
	const AXIS = 36;
	const TOP = 8;
	const BASE = H - 22;
	let by = $state<'goal' | 'category'>('goal');
	let picked = $state<number | null>(null);
	let width = $state(320);

	const week = $derived(picked ?? weeks.length - 1);
	const series = $derived(barSeries(goals, by));
	const totals = $derived(weeks.map((_, i) => series.reduce((s, x) => s + x.minutes[i], 0)));
	const scale = $derived(barScale(totals));
	const ticks = $derived([scale.min, scale.ticks[Math.floor(scale.ticks.length / 2)], scale.max]);
	const slot = $derived(Math.max(1, (width - AXIS) / weeks.length));
	const barW = $derived(Math.min(28, slot * 0.55));
	const day = (ymd: string) => formatDate(ymd, { day: 'numeric', month: 'short' });
	const short = (ymd: string) => `${Number(ymd.slice(8))}/${Number(ymd.slice(5, 7))}`;
</script>

<section class="flex flex-col gap-2 rounded-sm border border-border bg-bg p-3" aria-labelledby="dash-minutes">
	<div class="flex flex-wrap items-center justify-between gap-2">
		<h2 id="dash-minutes" class="text-sm font-semibold">Minutes per week</h2>
		<Segmented label="Stack by" compact bind:value={by} options={[{ value: 'goal', label: 'Goal' }, { value: 'category', label: 'Category' }]} />
	</div>
	<div bind:clientWidth={width} class="w-full">
		<svg {width} height={H} role="img" aria-label="Minutes done per week, last {weeks.length} weeks, this week {totals[totals.length - 1]} min">
			{#each ticks as t (t)}
				<line x1={AXIS} x2={width} y1={yOf(t, scale, TOP, BASE)} y2={yOf(t, scale, TOP, BASE)} class="stroke-border" stroke-width="1" />
				<text x={AXIS - 6} y={yOf(t, scale, TOP, BASE)} dy="0.32em" text-anchor="end" class="fill-muted text-xs tabular-nums">{formatNumber(t)}</text>
			{/each}
			{#each weeks as w, i (w)}
				{@const cx = AXIS + slot * i + slot / 2}
				{@const segs = stackWeek(series, i, scale, TOP, BASE)}
				{@const top = segs.at(-1)?.y ?? BASE}
				<clipPath id="dash-bar-{i}"><rect x={cx - barW / 2} y={top} width={barW} height={BASE - top + 4} rx="4" /></clipPath>
				<g clip-path="url(#dash-bar-{i})">
					{#each segs as s (s.key)}<rect x={cx - barW / 2} y={s.y} width={barW} height={s.h} fill={s.color} opacity={picked === null || i === week ? 1 : 0.55} />{/each}
				</g>
				<text x={cx} y={H - 6} text-anchor="middle" class="text-xs tabular-nums {i === week ? 'fill-fg' : 'fill-muted'}">{short(w)}</text>
				<rect x={cx - slot / 2} y="0" width={slot} height={H} fill="transparent" role="button" tabindex="0" aria-label="Week of {day(w)}, {totals[i]} min" aria-pressed={i === week} class="outline-none" onclick={() => (picked = i)} onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && (picked = i)} />
			{/each}
		</svg>
	</div>
	<p class="text-xs text-muted tabular-nums">Week of {day(weeks[week])} · <span class="font-medium text-fg">{totals[week]} min</span></p>
	{#if series.length}
		<ul class="flex flex-col gap-1" aria-label="Legend">
			{#each series as s (s.key)}
				<li class="flex items-center gap-2 text-xs"><span class="size-2.5 shrink-0 rounded-sm" style="background: {s.color}" aria-hidden="true"></span><span class="min-w-0 flex-1 truncate">{s.label}</span><span class="text-muted tabular-nums">{s.minutes[week]} min</span></li>
			{/each}
		</ul>
	{/if}
</section>
