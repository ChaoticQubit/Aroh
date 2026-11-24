<script lang="ts">
	import ChartFrame from './ChartFrame.svelte';
	import { niceScale, xOf } from '$lib/ui/chart';
	import { getClock } from '$lib/ui/clock';
	import { formatDate, formatNumber, formatYmd } from '$lib/ui/format';
	import type { SpecOf } from '$lib/ui/contract';

	/** Tracker series as bars in its series hue from zero with a grey dashed target; the latest bar is full strength. */
	let { spec }: { spec: SpecOf<'bar_chart'> } = $props();

	const { zone } = getClock();
	const scale = $derived(spec.points.length ? niceScale(spec.points.map((p) => p.value), spec.target, true) : null);
	const color = $derived(`var(--series-${spec.series ?? 1})`);
	const below = $derived(spec.target === undefined ? 0 : spec.points.filter((p) => p.value < spec.target!).length);
</script>

<ChartFrame label={spec.label} unit={spec.unit} target={spec.target} {scale} summary="{spec.points.length} days">
	{#snippet aside()}
		{#if spec.target !== undefined && spec.points.length}<span>{below} of {spec.points.length} below target</span>{/if}
	{/snippet}
	{#snippet plot(a)}
		{@const slot = (a.right - a.left) / spec.points.length}
		{@const bar = Math.min(20, slot * 0.5)}
		{#each spec.points as p, i (p.at)}
			{@const cx = xOf(i, spec.points.length, a.left + slot / 2, a.right - slot / 2)}
			{@const top = a.y(p.value)}
			<rect x={cx - bar / 2} y={top} width={bar} height={a.bottom - top} fill={color} fill-opacity={i === spec.points.length - 1 ? 1 : 0.55}>
				<title>{formatNumber(p.value, spec.unit)}</title>
			</rect>
			<text x={cx} y={a.height - 4} text-anchor="middle" class="fill-muted text-xs">{formatDate(formatYmd(p.at, zone), { weekday: 'narrow' })}</text>
		{/each}
	{/snippet}
</ChartFrame>
