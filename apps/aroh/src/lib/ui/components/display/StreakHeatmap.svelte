<script lang="ts">
	import Panel from '../Panel.svelte';
	import type { SpecOf } from '$lib/ui/contract';

	/** Daily completion as a 7-column grid of squares, four intensity steps of one series hue. */
	let { spec }: { spec: SpecOf<'streak_heatmap'> } = $props();

	const MIX = [0, 35, 65, 100] as const;
	const color = $derived(`var(--series-${spec.series ?? 2})`);
	const fill = (level: 0 | 1 | 2 | 3) => `background: color-mix(in oklch, ${color} ${MIX[level]}%, var(--surface-2))`;
	const NAMES = ['nothing done', 'some done', 'most done', 'all done'] as const;
	const streak = $derived.by(() => {
		let n = 0;
		for (let i = spec.days.length - 1; i >= 0 && spec.days[i].level > 0; i--) n++;
		return n;
	});
</script>

<Panel title={spec.label}>
	{#snippet aside()}<span>{streak} day streak</span>{/snippet}
	{#if spec.days.length}
		<ol class="grid max-w-60 grid-cols-7 gap-1" aria-label={spec.label}>
			{#each spec.days as d (d.date)}
				<li class="aspect-square rounded-[2px]" style={fill(d.level)} title="{d.date}: {NAMES[d.level]}"><span class="sr-only">{d.date}: {NAMES[d.level]}</span></li>
			{/each}
		</ol>
		<div class="flex max-w-60 items-center justify-end gap-1.5 text-xs text-muted" aria-hidden="true">
			Less {#each [0, 1, 2, 3] as const as level (level)}<span class="size-2.5 rounded-[2px]" style={fill(level)}></span>{/each} More
		</div>
	{:else}
		<p class="text-sm text-muted">No days logged yet</p>
	{/if}
</Panel>
