<script lang="ts">
	import Panel from '../Panel.svelte';
	import { formatNumber } from '$lib/ui/format';
	import type { SpecOf } from '$lib/ui/contract';

	/** Actual against target as two aligned bars, with the gap stated in words. */
	let { spec }: { spec: SpecOf<'target_vs_actual'> } = $props();

	const top = $derived(Math.max(spec.target, spec.actual, 1));
	const gap = $derived(spec.actual - spec.target);
	const rows = $derived([
		{ name: 'Actual', value: spec.actual, cls: 'bg-fg' },
		{ name: 'Target', value: spec.target, cls: 'bg-border-strong' }
	]);
</script>

<Panel title={spec.label}>
	{#snippet aside()}
		<span class={gap > 0 ? 'font-medium text-status-warn' : ''}>{gap === 0 ? 'On target' : gap > 0 ? `${formatNumber(gap, spec.unit)} over` : `${formatNumber(-gap, spec.unit)} short`}</span>
	{/snippet}
	<dl class="flex flex-col gap-2">
		{#each rows as row (row.name)}
			<div class="grid grid-cols-[56px_1fr_auto] items-center gap-3">
				<dt class="text-xs text-muted">{row.name}</dt>
				<dd class="h-1.5 overflow-hidden bg-surface-2"><div class="h-full {row.cls}" style="width: {(row.value / top) * 100}%"></div></dd>
				<dd class="text-sm font-medium tabular-nums">{formatNumber(row.value, spec.unit)}</dd>
			</div>
		{/each}
	</dl>
</Panel>
