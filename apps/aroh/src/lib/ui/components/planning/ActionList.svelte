<script lang="ts">
	import Check from '@lucide/svelte/icons/check';
	import Lock from '@lucide/svelte/icons/lock';
	import Panel from '../Panel.svelte';
	import StatusChip, { DOT } from '../StatusChip.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatTime } from '$lib/ui/format';
	import type { ActionView, OnChange, SpecOf } from '$lib/ui/contract';

	/**
	 * Actions in time order with a done toggle. Emits `{ id, status }` when one is ticked or unticked.
	 * With `onopen`, tapping a row's time and title opens that action by id.
	 */
	let { spec, onchange, onopen }: { spec: SpecOf<'action_list'>; onchange?: OnChange; onopen?: (id: string) => void } = $props();

	const { zone } = getClock();
	let overrides = $state<Record<string, ActionView['status']>>({});
	const items = $derived(spec.actions.map((a) => ({ ...a, status: overrides[a.id] ?? a.status })));
	const done = $derived(items.filter((a) => a.status === 'done').length);

	/** Flips one action between planned and done and reports it. */
	function toggle(a: ActionView) {
		const status = a.status === 'done' ? 'planned' : 'done';
		overrides[a.id] = status;
		onchange?.({ id: a.id, status });
	}
</script>

<Panel title={spec.label}>
	{#snippet aside()}<span>{done} of {items.length} done</span>{/snippet}
	{#if items.length}
		<ul class="-mx-4 -mb-4 divide-y divide-border border-t border-border">
			{#each items as a (a.id)}
				<li class="flex min-h-12 items-center gap-3 py-1 pr-1 pl-4">
					<svelte:element
						this={onopen ? 'button' : 'div'}
						type={onopen ? 'button' : undefined}
						role={onopen ? 'button' : undefined}
						aria-label={onopen ? `Edit ${a.title}` : undefined}
						class="flex min-w-0 flex-1 items-center gap-3 self-stretch rounded-sm text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
						onclick={onopen ? () => onopen(a.id) : undefined}
					>
						<span class="w-12 shrink-0 text-xs leading-4 text-muted tabular-nums">{formatTime(a.start, zone)}<br />{formatTime(a.end, zone)}</span>
						<span class="size-2 shrink-0 rounded-full {DOT[a.status]}" aria-hidden="true"></span>
						<span class="flex min-w-0 flex-1 flex-col">
							<span class="truncate text-sm font-medium {a.status === 'done' ? 'text-muted line-through' : 'text-fg'}">{a.title}</span>
							<span class="truncate text-xs text-muted">{a.goalTitle}</span>
						</span>
					</svelte:element>
					{#if a.locked}<Lock class="size-4 shrink-0 text-muted" aria-label="Locked" />{/if}
					{#if a.status === 'missed' || a.status === 'skipped'}
						<span class="mr-3"><StatusChip tone={a.status} label={a.status === 'missed' ? 'Missed' : 'Skipped'} /></span>
					{:else}
						<button
							type="button"
							aria-pressed={a.status === 'done'}
							aria-label="{a.status === 'done' ? 'Mark not done' : 'Mark done'}: {a.title}"
							class="flex size-11 shrink-0 items-center justify-center rounded-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
							onclick={() => toggle(a)}
						>
							<span class="flex size-4 items-center justify-center rounded-[3px] border {a.status === 'done' ? 'border-status-done bg-status-done text-bg' : 'border-border-strong text-transparent hover:border-fg'}">
								<Check class="size-3" strokeWidth={3} />
							</span>
						</button>
					{/if}
				</li>
			{/each}
		</ul>
	{:else}
		<p class="text-sm text-muted">Nothing planned</p>
	{/if}
</Panel>
