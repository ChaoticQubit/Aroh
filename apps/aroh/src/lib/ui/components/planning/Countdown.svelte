<script lang="ts">
	import * as Card from '$lib/ui/primitives/card';
	import { getClock } from '$lib/ui/clock';
	import { daysUntil, formatDay } from '$lib/ui/format';
	import type { SpecOf } from '$lib/ui/contract';

	/** Whole days left to a deadline with the date underneath; amber within 14 days, red once passed. */
	let { spec }: { spec: SpecOf<'countdown'> } = $props();

	const { now, zone } = getClock();
	const days = $derived(daysUntil(spec.deadline, now));
	const passed = $derived(Date.parse(spec.deadline) <= Date.parse(now));
	const soon = $derived(!passed && days <= 14);
</script>

<Card.Root size="sm" class="gap-1 px-4">
	<p class="text-xs text-muted">{spec.label}</p>
	<p class="text-lg font-semibold tabular-nums">
		{#if passed}<span class="text-status-missed">Deadline passed</span>{:else}<span class={soon ? 'text-status-warn' : ''}>{days}</span> <span class="text-sm font-normal text-muted">{days === 1 ? 'day' : 'days'} left</span>{/if}
	</p>
	<p class="text-xs text-muted">{formatDay(spec.deadline, zone)}</p>
</Card.Root>
