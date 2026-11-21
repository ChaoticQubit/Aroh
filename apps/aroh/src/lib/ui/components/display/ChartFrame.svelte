<script lang="ts" module>
	/** Plot area handed to the chart body: pixel bounds and the value-to-y mapping. */
	export type Plot = { left: number; right: number; top: number; bottom: number; height: number; y: (value: number) => number };
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import Panel from '../Panel.svelte';
	import { yOf, type Scale } from '$lib/ui/chart';
	import { formatNumber } from '$lib/ui/format';

	/**
	 * Shared frame of the line and bar charts: titled panel, faint horizontal grid with muted tick
	 * labels, the body drawn by `plot`, a dashed target line on top, its legend and the empty state.
	 * `scale` is null when there are no points.
	 */
	let { label, unit, target, scale, summary, aside, plot }: { label: string; unit: string; target?: number; scale: Scale | null; summary: string; aside?: Snippet; plot: Snippet<[Plot]> } = $props();

	const HEIGHT = 160;
	const PAD = { top: 8, right: 8, bottom: 22, left: 34 };
	let width = $state(320);
	const area = $derived<Plot | null>(
		scale
			? {
					left: PAD.left,
					right: Math.max(PAD.left, width - PAD.right),
					top: PAD.top,
					bottom: HEIGHT - PAD.bottom,
					height: HEIGHT,
					y: (v: number) => yOf(v, scale, PAD.top, HEIGHT - PAD.bottom)
				}
			: null
	);
</script>

<Panel title={label} {aside}>
	<div bind:clientWidth={width} class="w-full">
		{#if scale && area}
			<svg {width} height={HEIGHT} role="img" aria-label="{label}, {summary}{target === undefined ? '' : `, target ${formatNumber(target, unit)}`}">
				{#each scale.ticks as t (t)}
					<line x1={area.left} x2={area.right} y1={area.y(t)} y2={area.y(t)} class="stroke-border" stroke-width="1" />
					<text x={area.left - 6} y={area.y(t)} dy="0.32em" text-anchor="end" class="fill-muted text-xs tabular-nums">{formatNumber(t)}</text>
				{/each}
				{@render plot(area)}
				{#if target !== undefined}
					<line x1={area.left} x2={area.right} y1={area.y(target)} y2={area.y(target)} class="stroke-muted" stroke-width="1" stroke-dasharray="4 4" />
				{/if}
			</svg>
		{:else}
			<p class="text-sm text-muted">No entries yet</p>
		{/if}
	</div>
	{#if target !== undefined}
		<p class="flex items-center gap-2 text-xs text-muted"><span class="inline-block w-4 border-t border-dashed border-muted"></span>Target {formatNumber(target, unit)}</p>
	{/if}
</Panel>
