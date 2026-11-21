<script lang="ts">
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import Plus from '@lucide/svelte/icons/plus';
	import { Button } from '$lib/ui/primitives/button';
	import { getClock } from '$lib/ui/clock';
	import { formatDay } from '$lib/ui/format';
	import type { GoalRow } from './types';

	/** Goals list: one row per goal with its category dot, deadline and autonomy, plus New goal. */
	let { goals, onopen, onnew }: { goals: GoalRow[]; onopen?: (id: string) => void; onnew?: () => void } = $props();

	const { zone } = getClock();
</script>

<div class="flex w-full max-w-3xl flex-col gap-3 p-4 @3xl:p-6">
	<div class="flex items-center justify-between">
		<h2 class="text-sm font-semibold text-fg">Your goals <span class="text-xs font-normal text-muted tabular-nums">{goals.length}</span></h2>
		<Button variant="outline" size="touch" class="@3xl:h-8" onclick={() => onnew?.()}><Plus />New goal</Button>
	</div>
	{#if goals.length}
		<ul class="divide-y divide-border overflow-hidden rounded-md border border-border bg-bg">
			{#each goals as g (g.id)}
				<li>
					<button type="button" class="flex min-h-14 w-full items-center gap-3 px-4 py-2 text-left outline-none hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset" onclick={() => onopen?.(g.id)}>
						<span class="size-2 shrink-0 rounded-full" style="background: var(--cat-{g.slot})"></span>
						<span class="flex min-w-0 flex-1 flex-col">
							<span class="truncate text-sm font-medium">{g.title}</span>
							<span class="truncate text-xs text-muted">{g.category}{#if g.deadline}{' · '}Due {formatDay(g.deadline, zone)}{/if}{' · '}{g.autonomy}</span>
						</span>
						<ChevronRight class="size-4 shrink-0 text-muted" aria-hidden="true" />
					</button>
				</li>
			{/each}
		</ul>
	{:else}
		<p class="text-sm text-muted">No goals yet. Start one with New goal.</p>
	{/if}
</div>
