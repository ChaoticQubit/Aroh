<script lang="ts">
	import ChevronDown from '@lucide/svelte/icons/chevron-down';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import { Input } from '$lib/ui/primitives/input';
	import { getClock } from '$lib/ui/clock';
	import { formatDay } from '$lib/ui/format';
	import type { ProfileView } from './types';

	/**
	 * "What Aroh knows about you": every kept fact with its source and date, from answers and from
	 * edits. Tapping a fact shows Edit (`onedit` gets the new text), Delete and, for a fact with a
	 * topic, Never learn this (`onblock`); blocked topics are listed below with Allow again
	 * (`onallow`). `open` starts with that fact expanded.
	 */
	let {
		profile,
		open = null,
		onedit,
		ondelete,
		onblock,
		onallow
	}: {
		profile: ProfileView;
		open?: string | null;
		onedit?: (id: string, text: string) => void;
		ondelete?: (id: string) => void;
		onblock?: (id: string) => void;
		onallow?: (id: string) => void;
	} = $props();

	const id = $props.id();
	const { zone } = getClock();
	let expanded = $derived(open);
	let editing = $state<string | null>(null);
	let draft = $state('');

	/** Saves the edited text of the fact being edited; a blank text is ignored. */
	function save(factId: string) {
		const text = draft.trim();
		if (text) onedit?.(factId, text);
		editing = null;
	}
</script>

<div class="flex w-full max-w-3xl flex-col gap-6 p-4">
	<p class="text-sm text-muted">Learned from your answers and your edits. Agents read this before asking you again, and show it as "Still true?" when it matters.</p>

	<section class="flex flex-col gap-3" aria-labelledby="{id}-facts">
		<div class="flex items-baseline justify-between">
			<h2 id="{id}-facts" class="text-sm font-semibold">Facts</h2>
			<span class="text-xs text-muted tabular-nums">{profile.facts.length}</span>
		</div>
		{#if profile.facts.length}
			<Card.Root size="sm" class="gap-0 divide-y divide-border px-0 py-0">
				{#each profile.facts as f (f.id)}
					<div class="flex flex-col">
						<button type="button" aria-expanded={expanded === f.id} class="flex min-h-14 items-center gap-3 px-4 py-2 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset" onclick={() => (expanded = expanded === f.id ? null : f.id)}>
							<span class="flex flex-1 flex-col">
								<span class="text-sm font-medium">{f.text}</span>
								<span class="text-xs text-muted">{f.source} · {formatDay(f.date, zone)}</span>
							</span>
							<ChevronDown class="size-4 shrink-0 text-muted transition-transform {expanded === f.id ? 'rotate-180' : ''}" aria-hidden="true" />
						</button>
						{#if expanded === f.id}
							<div class="flex flex-col gap-2 px-4 pb-3">
								{#if editing === f.id}
									<div class="flex gap-2">
										<Input class="h-11 flex-1 text-lg" aria-label="Fact" bind:value={draft} />
										<Button size="touch" onclick={() => save(f.id)}>Save</Button>
									</div>
								{:else}
									<div class="flex flex-wrap gap-2">
										<Button variant="outline" size="touch" onclick={() => ((editing = f.id), (draft = f.text))}>Edit</Button>
										<Button variant="outline" size="touch" onclick={() => ondelete?.(f.id)}>Delete</Button>
										{#if f.blockable}<Button variant="ghost" size="touch" onclick={() => onblock?.(f.id)}>Never learn this</Button>{/if}
									</div>
									{#if f.blockable}<p class="text-xs text-muted">Never learn this deletes the fact and stops Aroh keeping it again. Agents will ask each time.</p>{/if}
								{/if}
							</div>
						{/if}
					</div>
				{/each}
			</Card.Root>
		{:else}
			<p class="text-sm text-muted">Nothing kept yet. Facts appear here once you answer a goal's questions.</p>
		{/if}
	</section>

	<section class="flex flex-col gap-3" aria-labelledby="{id}-never">
		<h2 id="{id}-never" class="text-sm font-semibold">Never learned</h2>
		{#if profile.blocked.length}
			<Card.Root size="sm" class="gap-0 divide-y divide-border px-4 py-0">
				{#each profile.blocked as b (b.id)}
					<div class="flex min-h-14 items-center gap-3 py-2">
						<span class="flex flex-1 flex-col"><span class="text-sm font-medium">{b.label}</span><span class="text-xs text-muted">Asked each time, never kept</span></span>
						<Button variant="ghost" size="touch" onclick={() => onallow?.(b.id)}>Allow again</Button>
					</div>
				{/each}
			</Card.Root>
		{:else}
			<p class="text-sm text-muted">Nothing blocked.</p>
		{/if}
	</section>
</div>
