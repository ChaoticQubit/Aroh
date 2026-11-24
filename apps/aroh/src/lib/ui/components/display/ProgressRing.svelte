<script lang="ts">
	import Panel from '../Panel.svelte';
	import { formatNumber } from '$lib/ui/format';
	import type { SpecOf } from '$lib/ui/contract';

	/**
	 * Concentric progress rings, outermost first, one per tracker in its series hue over a faint
	 * track of the same hue. Values past the target close the ring. A legend row per ring follows.
	 */
	let { spec }: { spec: SpecOf<'progress_ring'> } = $props();

	const SIZE = 96;
	const STROKE = 10;
	const GAP = 2;
	const rings = $derived(
		spec.rings.slice(0, 3).map((r, i) => {
			const radius = SIZE / 2 - STROKE / 2 - i * (STROKE + GAP);
			const ratio = r.target > 0 ? Math.min(1, r.value / r.target) : 0;
			return { ...r, radius, circumference: 2 * Math.PI * radius, ratio, color: `var(--series-${r.series})` };
		})
	);
</script>

<Panel title={spec.label}>
	<div class="flex items-center gap-4">
		<svg width={SIZE} height={SIZE} viewBox="0 0 {SIZE} {SIZE}" class="shrink-0" role="img" aria-label={rings.map((r) => `${r.label} ${formatNumber(r.value)} of ${formatNumber(r.target, r.unit)}`).join(', ')}>
			{#each rings as r (r.label)}
				<circle cx={SIZE / 2} cy={SIZE / 2} r={r.radius} fill="none" stroke={r.color} stroke-opacity="0.18" stroke-width={STROKE} />
				<circle
					cx={SIZE / 2}
					cy={SIZE / 2}
					r={r.radius}
					fill="none"
					stroke={r.color}
					stroke-width={STROKE}
					stroke-linecap="round"
					stroke-dasharray="{r.circumference * r.ratio} {r.circumference}"
					transform="rotate(-90 {SIZE / 2} {SIZE / 2})"
				/>
			{/each}
		</svg>
		<dl class="flex min-w-0 flex-1 flex-col gap-2">
			{#each rings as r (r.label)}
				<div class="flex items-baseline gap-2">
					<dt class="flex min-w-0 flex-1 items-center gap-2 text-sm text-fg"><span class="size-2 shrink-0 rounded-full" style="background: {r.color}"></span><span class="truncate">{r.label}</span></dt>
					<dd class="text-sm font-semibold tabular-nums">{formatNumber(r.value)} <span class="font-normal text-muted">/ {formatNumber(r.target, r.unit)}</span></dd>
				</div>
			{/each}
		</dl>
	</div>
</Panel>
