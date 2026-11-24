<script lang="ts">
	import Panel from '../Panel.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDay, formatNumber, formatTime } from '$lib/ui/format';
	import type { SpecOf } from '$lib/ui/contract';

	/** Logged entries, newest first, each with its time and optional value. */
	let { spec }: { spec: SpecOf<'log_list'> } = $props();

	const { zone } = getClock();
</script>

<Panel title={spec.label}>
	{#snippet aside()}<span>{spec.entries.length} entries</span>{/snippet}
	{#if spec.entries.length}
		<ul class="-my-1 divide-y divide-border">
			{#each spec.entries as e (e.at + e.text)}
				<li class="flex min-h-11 items-center gap-3 py-2">
					<time datetime={e.at} class="w-20 shrink-0 text-xs text-muted tabular-nums">{formatDay(e.at, zone).split(' ').slice(1).join(' ')} {formatTime(e.at, zone)}</time>
					<span class="flex-1 text-sm">{e.text}</span>
					{#if e.value !== undefined}<span class="text-sm font-medium tabular-nums">{formatNumber(e.value, e.unit)}</span>{/if}
				</li>
			{/each}
		</ul>
	{:else}
		<p class="text-sm text-muted">Nothing logged yet</p>
	{/if}
</Panel>
