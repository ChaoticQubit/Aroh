<script lang="ts">
	import Check from '@lucide/svelte/icons/check';
	import Lock from '@lucide/svelte/icons/lock';
	import Panel from '../Panel.svelte';
	import StatusChip, { DOT } from '../StatusChip.svelte';
	import { getClock } from '$lib/ui/clock';
	import { dayLabels, formatTime, formatYmd } from '$lib/ui/format';
	import { actionStyle } from '$lib/ui/status';
	import type { ActionView, OnChange, SpecOf } from '$lib/ui/contract';

	/**
	 * Actions in time order with a done toggle; emits `{ id, status }` on a tick. With `onopen` a row's
	 * time and title open that action by id, else they link to its card page (Q71). Titles wrap to two
	 * lines and goal names to one, so a row never outgrows its container. A list that spans days
	 * shows each row's day ("Today", "Tue 24") above its times.
	 */
	let { spec, onchange, onopen }: { spec: SpecOf<'action_list'>; onchange?: OnChange; onopen?: (id: string) => void } = $props();

	const { now, zone } = getClock();
	const today = formatYmd(now, zone);
	let overrides = $state<Record<string, ActionView['status']>>({});
	$effect.pre(() => {
		void spec.actions;
		overrides = {};
	});
	const items = $derived(spec.actions.map((a) => ({ ...a, status: overrides[a.id] ?? a.status })));
	const days = $derived(dayLabels(spec.actions.map((a) => a.start), zone, today));
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
			{#each items as a, i (a.id)}
				{@const day = days[i]}
				<li class="flex min-h-12 items-center gap-3 py-1 pr-1 pl-4">
					<svelte:element
						this={onopen ? 'button' : 'a'}
						type={onopen ? 'button' : undefined}
						role={onopen ? 'button' : 'link'}
						href={onopen ? undefined : `/actions/${a.id}`}
						aria-label={onopen ? `Edit ${a.title}` : `Open ${a.title}`}
						class="flex min-w-0 flex-1 items-center gap-3 self-stretch rounded-sm text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
						onclick={onopen ? () => onopen(a.id) : undefined}
					>
						<span class="w-12 shrink-0 text-xs leading-4 text-muted tabular-nums">{#if day}<span class="font-medium text-fg">{day}</span><br />{/if}{formatTime(a.start, zone)}<br />{formatTime(a.end, zone)}</span>
						<span class="size-2 shrink-0 rounded-full {DOT[a.status]}" aria-hidden="true"></span>
						<span class="flex min-w-0 flex-1 flex-col">
							<span class="line-clamp-2 text-sm font-medium wrap-anywhere {actionStyle(a.status).title}">{a.title}</span>
							<span class="line-clamp-1 text-xs text-muted wrap-anywhere">{a.goalTitle}</span>
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
