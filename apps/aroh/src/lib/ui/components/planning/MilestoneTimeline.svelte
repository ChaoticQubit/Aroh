<script lang="ts">
	import Check from '@lucide/svelte/icons/check';
	import Panel from '../Panel.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDay } from '$lib/ui/format';
	import type { SpecOf } from '$lib/ui/contract';

	/** Milestones on a vertical line: done ones ticked, overdue ones flagged. */
	let { spec }: { spec: SpecOf<'milestone_timeline'> } = $props();

	const { now, zone } = getClock();
	const done = $derived(spec.milestones.filter((m) => m.done).length);
</script>

<Panel title={spec.label}>
	{#snippet aside()}<span>{done} of {spec.milestones.length} reached</span>{/snippet}
	<ol class="flex flex-col">
		{#each spec.milestones as m, i (m.title)}
			{@const overdue = !m.done && Date.parse(m.due) < Date.parse(now)}
			<li class="relative flex min-h-11 gap-3 pb-3">
				{#if i < spec.milestones.length - 1}<span class="absolute top-5 bottom-0 left-[7.5px] w-px bg-border" aria-hidden="true"></span>{/if}
				<span class="z-10 mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-[3px] border {m.done ? 'border-status-done bg-status-done text-bg' : overdue ? 'border-status-missed bg-bg' : 'border-border-strong bg-bg'}">
					{#if m.done}<Check class="size-3" strokeWidth={3} />{/if}
				</span>
				<div class="flex flex-col">
					<span class="text-sm font-medium {m.done ? 'text-muted' : 'text-fg'}">{m.title}</span>
					<span class="text-xs {overdue ? 'font-medium text-status-missed' : 'text-muted'}">{overdue ? 'Overdue, was due ' : 'Due '}{formatDay(m.due, zone)}</span>
				</div>
			</li>
		{/each}
	</ol>
</Panel>
