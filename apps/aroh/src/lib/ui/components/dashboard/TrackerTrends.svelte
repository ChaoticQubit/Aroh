<script lang="ts">
	import { formatNumber } from '$lib/ui/format';
	import type { DashGoal } from '$lib/app/dashboard';
	import { sparkline } from './geometry';

	/**
	 * Each shown goal's tracker trend as a small line in its chart colour (muted past the sixth
	 * goal), with an 8px dot on the latest value, the plugin target as a dashed line and the goal name.
	 * Goals without a number tracker are left out; the section hides when none has one.
	 */
	let { goals, weeks }: { goals: DashGoal[]; weeks: number } = $props();

	const W = 150;
	const H = 44;
	const shown = $derived(goals.filter((g) => g.trend !== null));
	const hasTarget = $derived(shown.some((g) => g.trend?.target != null));
	const color = (g: DashGoal) => (g.series ? `var(--chart-${g.series})` : 'var(--muted)');
</script>

{#if shown.length}
	<section class="flex flex-col gap-2" aria-labelledby="dash-trends">
		<h2 id="dash-trends" class="text-sm font-semibold">Tracker trends, last {weeks} weeks</h2>
		<ul class="grid grid-cols-2 gap-px overflow-hidden rounded-sm border border-border bg-border">
			{#each shown as g (g.id)}
				{@const t = g.trend!}
				{@const l = sparkline(t.points, t.target, W, H)}
				{@const last = t.points.findLast((p) => p !== null) ?? null}
				<li class="flex min-w-0 flex-col gap-1 bg-bg px-3 py-2">
					<p class="truncate text-xs text-muted">{t.label}</p>
					{#if last !== null}
						<p class="truncate text-md font-semibold tabular-nums">{formatNumber(last, t.unit ?? undefined)}{#if t.target != null}<span class="ml-1 text-xs font-normal text-muted">of {formatNumber(t.target)}</span>{/if}</p>
					{:else}
						<p class="text-md text-muted">No entries yet</p>
					{/if}
					<svg viewBox="0 0 {W} {H}" class="h-11 w-full" preserveAspectRatio="none" role="img" aria-label="{t.label} trend for {g.title}{last !== null ? `, latest ${last}` : ', no entries'}">
						{#if l.target !== null}<line x1="0" x2={W} y1={l.target} y2={l.target} class="stroke-muted" stroke-width="1" stroke-dasharray="3 3" vector-effect="non-scaling-stroke" />{/if}
						{#if l.path}<polyline points={l.path} fill="none" stroke={color(g)} stroke-width="2" stroke-linejoin="round" stroke-linecap="round" vector-effect="non-scaling-stroke" />{/if}
						{#if l.end}<line x1={l.end[0]} y1={l.end[1]} x2={l.end[0]} y2={l.end[1]} stroke={color(g)} stroke-width="8" stroke-linecap="round" vector-effect="non-scaling-stroke" />{/if}
					</svg>
					<p class="flex min-w-0 items-center gap-1.5 text-xs"><span class="size-2 shrink-0 rounded-full" style="background: {color(g)}" aria-hidden="true"></span><span class="truncate">{g.title}</span></p>
				</li>
			{/each}
			{#if shown.length % 2}<li class="bg-bg" aria-hidden="true"></li>{/if}
		</ul>
		{#if hasTarget}<p class="flex items-center gap-2 text-xs text-muted"><span class="inline-block w-4 border-t border-dashed border-muted"></span>Target</p>{/if}
	</section>
{/if}
