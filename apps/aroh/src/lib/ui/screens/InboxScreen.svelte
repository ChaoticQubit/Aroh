<script lang="ts">
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';
	import * as Card from '$lib/ui/primitives/card';
	import { Progress } from '$lib/ui/primitives/progress';
	import Persona from '../components/agent/Persona.svelte';
	import Render from '../components/Render.svelte';
	import type { InboxItem, RunView } from './types';

	/**
	 * Inbox: proposals to approve, preference suggestions, insights and agent runs in progress.
	 * Calendar groups from triage come first. Empty groups are left out. `ondecide` gets the group,
	 * the item id and the card's decision.
	 */
	let {
		approvals,
		suggestions,
		insights,
		runs,
		triage = [],
		ondecide
	}: {
		approvals: InboxItem[];
		suggestions: InboxItem[];
		insights: InboxItem[];
		runs: RunView[];
		triage?: InboxItem[];
		ondecide?: (group: 'triage' | 'approval' | 'suggestion' | 'insight', id: string, decision: unknown) => void;
	} = $props();

	const groups = $derived(
		[
			{ key: 'triage' as const, title: 'Calendar groups', items: triage },
			{ key: 'approval' as const, title: 'Needs your approval', items: approvals },
			{ key: 'suggestion' as const, title: 'Suggestions', items: suggestions },
			{ key: 'insight' as const, title: 'Insights', items: insights }
		].filter((g) => g.items.length > 0)
	);
</script>

<div class="flex w-full max-w-3xl flex-col gap-8 p-4 @3xl:p-6">
	{#each groups as g (g.title)}
		<section class="flex flex-col gap-2" aria-label={g.title}>
			<h2 class="flex items-center gap-2 text-sm font-semibold text-fg">{g.title} <span class="text-xs font-normal text-muted tabular-nums">{g.items.length}</span></h2>
			<div class="divide-y divide-border overflow-hidden rounded-md border border-border bg-bg [&_[data-slot=card]]:rounded-none [&_[data-slot=card]]:border-0">
				{#each g.items as item (item.id)}<Render spec={item.spec} onchange={(d) => ondecide?.(g.key, item.id, d)} />{/each}
			</div>
		</section>
	{/each}
	{#if runs.length}
		<section class="flex flex-col gap-2" aria-label="Runs in progress">
			<h2 class="flex items-center gap-2 text-sm font-semibold text-fg">Runs in progress <span class="text-xs font-normal text-muted tabular-nums">{runs.length}</span></h2>
			<Card.Root size="sm" class="gap-0 divide-y divide-border px-4 py-0">
				{#each runs as run (run.title)}
					<div class="flex flex-col gap-2 py-3">
						<div class="flex items-center justify-between gap-2">
							<Persona name={run.persona} />
							<LoaderCircle class="size-4 animate-spin text-status-planned" aria-hidden="true" />
						</div>
						<p class="text-sm font-medium">{run.title}</p>
						<Progress class="[&>[data-slot=progress-indicator]]:bg-status-planned" value={run.progress} aria-label="{run.title}, {run.progress} percent" />
						{#if run.href}
							<a href={run.href} class="flex min-h-11 items-center text-sm font-medium text-fg underline underline-offset-4">{run.step}</a>
						{:else}
							<p class="text-xs text-muted">{run.step}</p>
						{/if}
					</div>
				{/each}
			</Card.Root>
		</section>
	{/if}
	{#if !groups.length && !runs.length}
		<p class="text-sm text-muted">Nothing needs you right now.</p>
	{/if}
</div>
