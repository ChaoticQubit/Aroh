<script lang="ts">
	import ChartFrame from './ChartFrame.svelte';
	import { niceScale, xOf } from '$lib/ui/chart';
	import { getClock } from '$lib/ui/clock';
	import { formatDay, formatNumber } from '$lib/ui/format';
	import type { SpecOf } from '$lib/ui/contract';

	/** Tracker series as a line in its series hue with a grey dashed target and the latest point marked. */
	let { spec }: { spec: SpecOf<'line_chart'> } = $props();

	const { zone } = getClock();
	const scale = $derived(spec.points.length ? niceScale(spec.points.map((p) => p.value), spec.target) : null);
	const last = $derived(spec.points.at(-1));
	const color = $derived(`var(--series-${spec.series ?? 1})`);
</script>

<ChartFrame label={spec.label} unit={spec.unit} target={spec.target} {scale} summary={last ? `latest ${formatNumber(last.value, spec.unit)}` : 'no entries'}>
	{#snippet aside()}
		{#if last}<span class="text-sm font-semibold text-fg">{formatNumber(last.value, spec.unit)}</span>{/if}
	{/snippet}
	{#snippet plot(a)}
		{@const xy = spec.points.map((p, i) => [xOf(i, spec.points.length, a.left, a.right), a.y(p.value)] as const)}
		<polyline points={xy.map(([x, y]) => `${x},${y}`).join(' ')} fill="none" stroke={color} stroke-width="2" stroke-linejoin="round" stroke-linecap="round" />
		<circle cx={xy.at(-1)?.[0]} cy={xy.at(-1)?.[1]} r="3.5" fill={color} class="stroke-bg" stroke-width="1.5" />
		<text x={a.left} y={a.height - 4} class="fill-muted text-xs">{formatDay(spec.points[0].at, zone)}</text>
		{#if spec.points.length > 1}<text x={a.right} y={a.height - 4} text-anchor="end" class="fill-muted text-xs">{formatDay(spec.points[spec.points.length - 1].at, zone)}</text>{/if}
	{/snippet}
</ChartFrame>
