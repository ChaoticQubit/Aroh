<script lang="ts">
	import Plus from '@lucide/svelte/icons/plus';
	import { Button } from '$lib/ui/primitives/button';
	import StatusChip from '../components/StatusChip.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDay } from '$lib/ui/format';
	import type { GoalProgress } from '$lib/app/views';
	import type { CategoryView, GoalRow } from './types';

	/**
	 * Goals list: title with counts and New goal, one-pick filter chips (All, each status shown, each
	 * category used), then one card of goals with category dot, status chip, progress bar to date and
	 * next step from `progress` (by goal id; rows without one show no chip). `onopen` gets a goal id.
	 */
	let {
		goals,
		categories = [],
		progress = {},
		onopen,
		onnew
	}: { goals: GoalRow[]; categories?: CategoryView[]; progress?: Record<string, GoalProgress>; onopen?: (id: string) => void; onnew?: () => void } = $props();

	const { zone } = getClock();
	let chip = $state('All');
	const statuses = $derived([...new Set(goals.flatMap((g) => (progress[g.id] ? [progress[g.id].label] : [])))]);
	const used = $derived(categories.filter((c) => goals.some((g) => g.category === c.name)).map((c) => c.name));
	const chips = $derived(['All', ...statuses, ...used.filter((c) => !statuses.includes(c))]);
	const current = $derived(chips.includes(chip) ? chip : 'All');
	const visible = $derived(current === 'All' ? goals : goals.filter((g) => progress[g.id]?.label === current || g.category === current));
	const summary = $derived(
		statuses
			.map((label) => `${goals.filter((g) => progress[g.id]?.label === label).length} ${label.toLowerCase()}`)
			.join(', ')
	);
</script>

<div class="flex w-full max-w-3xl flex-col gap-4 p-4 @3xl:p-6">
	<div class="flex items-start justify-between gap-3">
		<header class="flex min-w-0 flex-col gap-1">
			<h2 class="text-2xl leading-tight font-semibold">Goals</h2>
			<p class="text-sm text-muted">{goals.length} {goals.length === 1 ? 'goal' : 'goals'}{#if summary}{' · '}{summary}{/if}</p>
		</header>
		<Button size="touch" class="shrink-0 gap-1.5" onclick={() => onnew?.()}><Plus />New goal</Button>
	</div>
	{#if chips.length > 2}
		<div class="-mx-4 flex gap-2 overflow-x-auto px-4 pb-1" role="radiogroup" aria-label="Filter goals">
			{#each chips as c (c)}
				<button
					type="button"
					role="radio"
					aria-checked={current === c}
					class="min-h-11 shrink-0 rounded-sm border px-3 text-sm whitespace-nowrap outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {current === c ? 'border-fg bg-fg text-bg' : 'border-border bg-bg text-fg'}"
					onclick={() => (chip = c)}>{c}</button
				>
			{/each}
		</div>
	{/if}
	{#if visible.length}
		<ul class="flex flex-col divide-y divide-border overflow-hidden rounded-sm border border-border bg-bg">
			{#each visible as g (g.id)}
				{@const p = progress[g.id]}
				<li>
					<button type="button" class="flex w-full flex-col gap-2 px-4 py-3 text-left outline-none hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset" data-goal={g.id} onclick={() => onopen?.(g.id)}>
						<span class="flex w-full items-start gap-2">
							<span class="mt-1.5 size-2 shrink-0 rounded-full" style="background: var(--cat-{g.slot})" aria-hidden="true"></span>
							<span class="flex min-w-0 flex-1 flex-col">
								<span class="text-base font-medium wrap-anywhere">{g.title}</span>
								<span class="truncate text-xs text-muted">{g.category}{#if g.deadline}{' · '}Due {formatDay(g.deadline, zone)}{/if}{' · '}{g.autonomy}</span>
							</span>
							{#if p}<StatusChip tone={p.tone} label={p.label} />{/if}
						</span>
						{#if p && p.due > 0}
							<span class="ml-4 flex h-1.5 overflow-hidden rounded-full bg-surface-2" role="img" aria-label="{p.done} of {p.due} sessions done to date">
								<span class="h-full rounded-full bg-fg" style="width: {Math.min(100, Math.round((p.done / p.due) * 100))}%"></span>
							</span>
						{/if}
						{#if p?.line}<span class="ml-4 text-sm text-muted">{p.line}</span>{/if}
					</button>
				</li>
			{/each}
		</ul>
	{:else if goals.length}
		<p class="text-sm text-muted">No goals match this filter.</p>
	{:else}
		<p class="text-sm text-muted">No goals yet. Start one with New goal.</p>
	{/if}
</div>
