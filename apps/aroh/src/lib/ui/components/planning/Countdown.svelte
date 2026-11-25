<script lang="ts">
	import * as Card from '$lib/ui/primitives/card';
	import { getClock } from '$lib/ui/clock';
	import { deadlineState, formatDay } from '$lib/ui/format';
	import type { SpecOf } from '$lib/ui/contract';

	/** Whole days left to a deadline with the date underneath; amber within 14 days and on the due day, red once passed. */
	let { spec }: { spec: SpecOf<'countdown'> } = $props();

	const { now, zone } = getClock();
	const at = $derived(deadlineState(spec.deadline, now));
	const soon = $derived(at.state === 'left' && at.days <= 14);
</script>

<Card.Root size="sm" class="gap-1 px-4">
	<p class="text-xs text-muted">{spec.label}</p>
	<p class="text-lg font-semibold tabular-nums">
		{#if at.state === 'passed'}<span class="text-status-missed">Deadline passed</span>{:else if at.state === 'today'}<span class="text-status-warn">Due today</span>{:else}<span class={soon ? 'text-status-warn' : ''}>{at.days}</span> <span class="text-sm font-normal text-muted">{at.days === 1 ? 'day' : 'days'} left</span>{/if}
	</p>
	<p class="text-xs text-muted">{formatDay(spec.deadline, zone)}</p>
</Card.Root>
