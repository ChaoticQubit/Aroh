<script lang="ts">
	import ArrowDownRight from '@lucide/svelte/icons/arrow-down-right';
	import ArrowUpRight from '@lucide/svelte/icons/arrow-up-right';
	import * as Card from '$lib/ui/primitives/card';
	import { formatNumber, unitLabel } from '$lib/ui/format';
	import type { SpecOf } from '$lib/ui/contract';

	/** One headline number with its unit and the change since the last period. */
	let { spec }: { spec: SpecOf<'stat_tile'> } = $props();

	const unit = $derived(unitLabel(spec.unit));
</script>

<Card.Root size="sm" class="gap-1 px-4">
	<p class="text-xs text-muted">{spec.label}</p>
	<p class="text-lg font-semibold tabular-nums">{formatNumber(spec.value)} {#if unit}<span class="text-sm font-normal text-muted">{unit}</span>{/if}</p>
	{#if spec.delta !== undefined && spec.delta !== 0}
		<p class="flex items-center gap-1 text-xs text-muted">
			{#if spec.delta > 0}<ArrowUpRight class="size-3.5" />{:else}<ArrowDownRight class="size-3.5" />{/if}
			{spec.delta > 0 ? '+' : ''}{formatNumber(spec.delta, unit)} this week
		</p>
	{:else}
		<p class="text-xs text-muted">No change this week</p>
	{/if}
</Card.Root>
