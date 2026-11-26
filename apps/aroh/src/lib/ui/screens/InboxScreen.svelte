<script lang="ts">
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';
	import * as Card from '$lib/ui/primitives/card';
	import { Progress } from '$lib/ui/primitives/progress';
	import Persona from '../components/agent/Persona.svelte';
	import StatusChip, { type Tone } from '../components/StatusChip.svelte';
	import Render from '../components/Render.svelte';
	import PlacePrompt from '../components/location/PlacePrompt.svelte';
	import type { PlacePromptView } from '../components/location/types';
	import Trash from '@lucide/svelte/icons/trash';
	import { Button } from '$lib/ui/primitives/button';
	import * as Sheet from '$lib/ui/primitives/sheet';
	import type { DeleteGroup, InboxItem, RunView } from './types';

	const BAR: Record<Tone, string> = {
		done: '[&>[data-slot=progress-indicator]]:bg-status-done',
		planned: '[&>[data-slot=progress-indicator]]:bg-status-planned',
		warn: '[&>[data-slot=progress-indicator]]:bg-status-warn',
		missed: '[&>[data-slot=progress-indicator]]:bg-status-missed',
		skipped: '[&>[data-slot=progress-indicator]]:bg-status-skipped'
	};

	/**
	 * Inbox: proposals to approve, preference suggestions, insights and agent runs in progress.
	 * Calendar groups from triage come first. Empty groups are left out. `ondecide` gets the group,
	 * the item id and the card's decision. `places` are new-place prompts, listed first; `onplace` gets
	 * the prompt id and its answer. Every entry has a delete button (B115): `ondelete` gets its
	 * group and id, after a confirm for a run that the delete cancels. Clear all asks once, then
	 * calls `onclear`.
	 */
	let {
		approvals,
		suggestions,
		insights,
		runs,
		triage = [],
		places = [],
		ondecide,
		onplace,
		ondelete,
		onclear
	}: {
		approvals: InboxItem[];
		suggestions: InboxItem[];
		insights: InboxItem[];
		runs: RunView[];
		triage?: InboxItem[];
		places?: PlacePromptView[];
		ondecide?: (group: 'triage' | 'approval' | 'suggestion' | 'insight', id: string, decision: unknown) => void;
		onplace?: (id: string, decision: unknown) => void;
		ondelete?: (group: DeleteGroup, id: string) => void;
		onclear?: () => void;
	} = $props();

	let asking = $state<{ title: string; detail: string; yes: string; run: () => void } | null>(null);

	/** The name a delete button reads out for an Inbox card. */
	function nameOf(spec: InboxItem['spec']): string {
		if (spec.kind === 'proposal_card') return spec.summary;
		if (spec.kind === 'insight_card') return spec.title;
		if (spec.kind === 'question_card') return spec.question;
		return 'entry';
	}

	/** Deletes a run at once, or asks first when the delete cancels it. */
	function deleteRun(run: RunView) {
		const id = run.id;
		if (!id) return;
		if (!run.confirm) return ondelete?.('run', id);
		asking = { title: `Stop "${run.title}"?`, detail: 'The run is cancelled and leaves the Inbox. This cannot be undone.', yes: 'Stop and delete', run: () => ondelete?.('run', id) };
	}

	/** Asks once, then clears every entry. */
	function clearAll() {
		asking = { title: 'Clear the Inbox?', detail: 'Proposals are dismissed, runs still going are stopped, and every entry leaves the Inbox.', yes: 'Clear all', run: () => onclear?.() };
	}

	/** Runs the confirmed choice and closes the sheet. */
	function confirm() {
		const a = asking;
		asking = null;
		a?.run();
	}

	const DEL = 'shrink-0 text-muted';
	const ROW = 'flex min-h-14 w-full items-center justify-center px-4 text-base outline-none focus-visible:bg-surface';

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
	{#if groups.length || runs.length || places.length}
		<div class="-mb-6 flex justify-end">
			<Button variant="ghost" size="touch" class="text-muted" onclick={clearAll}>Clear all</Button>
		</div>
	{/if}
	{#if places.length}
		<section class="flex flex-col gap-2" aria-label="New places">
			<h2 class="flex items-center gap-2 text-sm font-semibold text-fg">New places <span class="text-xs font-normal text-muted tabular-nums">{places.length}</span></h2>
			<div class="divide-y divide-border overflow-hidden rounded-md border border-border bg-bg [&_[data-slot=card]]:rounded-none [&_[data-slot=card]]:border-0">
				{#each places as p (p.id)}
					<div class="flex items-start">
						<div class="min-w-0 flex-1"><PlacePrompt prompt={p} ondecide={(d) => onplace?.(p.id, d)} /></div>
						<Button variant="ghost" size="icon-touch" class={DEL} aria-label="Delete {p.name}" onclick={() => ondelete?.('place', p.id)}><Trash /></Button>
					</div>
				{/each}
			</div>
		</section>
	{/if}
	{#each groups as g (g.title)}
		<section class="flex flex-col gap-2" aria-label={g.title}>
			<h2 class="flex items-center gap-2 text-sm font-semibold text-fg">{g.title} <span class="text-xs font-normal text-muted tabular-nums">{g.items.length}</span></h2>
			<div class="divide-y divide-border overflow-hidden rounded-md border border-border bg-bg [&_[data-slot=card]]:rounded-none [&_[data-slot=card]]:border-0">
				{#each g.items as item (item.id)}
					<div class="flex items-start">
						<div class="min-w-0 flex-1"><Render spec={item.spec} onchange={(d) => ondecide?.(g.key, item.id, d)} /></div>
						<Button variant="ghost" size="icon-touch" class={DEL} aria-label="Delete {nameOf(item.spec)}" onclick={() => ondelete?.(g.key, item.id)}><Trash /></Button>
					</div>
				{/each}
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
							<div class="flex items-center gap-1">
								{#if run.working}<LoaderCircle class="size-4 animate-spin text-status-planned" aria-hidden="true" />{:else}<StatusChip tone={run.tone} label={run.step.split(':')[0]} />{/if}
								{#if run.id}<Button variant="ghost" size="icon-touch" class={DEL} aria-label="Delete {run.title}" onclick={() => deleteRun(run)}><Trash /></Button>{/if}
							</div>
						</div>
						<p class="text-sm font-medium">{run.title}</p>
						<Progress class={BAR[run.tone]} value={run.progress} aria-label="{run.title}, {run.progress} percent" />
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
	{#if !groups.length && !runs.length && !places.length}
		<p class="text-sm text-muted">Nothing needs you right now.</p>
	{/if}
</div>
{#if asking}
	<Sheet.Root open={true} onOpenChange={(open) => !open && (asking = null)}>
		<Sheet.Content side="bottom" showCloseButton={false} class="flex flex-col gap-2 border-none bg-transparent pr-[max(0.5rem,env(safe-area-inset-right))] pb-[max(0.5rem,env(safe-area-inset-bottom))] pl-[max(0.5rem,env(safe-area-inset-left))] text-fg shadow-none">
			<div class="flex flex-col divide-y divide-border overflow-hidden rounded-[var(--radius-dialog)] bg-bg">
				<div class="flex flex-col gap-1 px-4 py-3 text-center">
					<Sheet.Title class="text-sm font-medium text-fg">{asking.title}</Sheet.Title>
					<Sheet.Description class="text-xs text-muted">{asking.detail}</Sheet.Description>
				</div>
				<button type="button" class="{ROW} text-danger" onclick={confirm}>{asking.yes}</button>
			</div>
			<button type="button" class="{ROW} rounded-[var(--radius-dialog)] bg-bg font-medium" onclick={() => (asking = null)}>Cancel</button>
		</Sheet.Content>
	</Sheet.Root>
{/if}
