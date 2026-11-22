<script lang="ts">
	import MessageSquare from '@lucide/svelte/icons/message-square';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import Persona from '../components/agent/Persona.svelte';
	import Render from '../components/Render.svelte';
	import Segmented from '../components/Segmented.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDay, formatNumber } from '$lib/ui/format';
	import type { ViewSpec } from '$lib/ui/contract';
	import type { AutonomyLabel, GoalHeader } from './types';

	/**
	 * Goal dashboard built from the goal's view spec, with the persona chat entry and autonomy level.
	 * `onautonomy` gets the picked level, `onchat` fires on the chat button; `chatStatus` shows under it.
	 * With `versions`, a Dashboard card picks the plugin version shown; `onversion` gets its value.
	 */
	let {
		goal,
		view,
		chatStatus = null,
		versions = [],
		version = '',
		onautonomy,
		onchat,
		onversion
	}: {
		goal: GoalHeader;
		view: ViewSpec;
		chatStatus?: string | null;
		versions?: { value: string; label: string }[];
		version?: string;
		onautonomy?: (level: AutonomyLabel) => void;
		onchat?: () => void;
		onversion?: (value: string) => void;
	} = $props();

	const { zone } = getClock();
	let picked = $state<AutonomyLabel | null>(null);
	const autonomy = $derived(picked ?? goal.autonomy);
	const AUTONOMY = {
		Ask: 'Every change waits for your approval.',
		Balanced: 'Small changes apply on their own with undo. Big ones ask first.',
		Auto: 'All changes apply with undo, plus a daily digest.'
	} as const;
	const lead = $derived(goal.lead);

	/** Records the picked level and reports it. */
	function pick(level: AutonomyLabel) {
		picked = level;
		onautonomy?.(level);
	}
</script>

<div class="flex w-full max-w-5xl flex-col gap-4 p-4 @3xl:p-6">
	<div class="flex flex-col gap-1">
		<p class="flex items-center gap-2 text-xs font-medium text-muted">
			<span class="size-2 rounded-full" style="background: var(--cat-{goal.slot})"></span>{goal.category}
		</p>
		<h2 class="text-xl font-semibold">{view.title}</h2>
		{#if goal.deadline || lead}
			<p class="text-sm text-muted">{#if goal.deadline}Due {formatDay(goal.deadline, zone)}{/if}{#if goal.deadline && lead}{' · '}{/if}{#if lead}{formatNumber(lead.now, lead.unit)} now{#if lead.target !== undefined}, {formatNumber(lead.target, lead.unit)} target{/if}{/if}</p>
		{/if}
	</div>

	<div class="grid gap-4 @3xl:grid-cols-2">
		<Card.Root size="sm" class="gap-3 px-4">
			<div class="flex items-center justify-between gap-2">
				<p class="text-sm font-medium">Your personas</p>
				<MessageSquare class="size-4 text-muted" aria-hidden="true" />
			</div>
			<div class="flex flex-wrap gap-x-4 gap-y-2">
				{#each goal.personas as p (p)}<Persona name={p} />{/each}
			</div>
			<div><Button variant="outline" size="touch" class="@3xl:h-8" onclick={() => onchat?.()}>Chat with your personas</Button></div>
			{#if chatStatus}<p class="text-sm text-muted" role="status">{chatStatus}</p>{/if}
		</Card.Root>
		<Card.Root size="sm" class="gap-3 px-4">
			<p class="text-sm font-medium">How much can agents change on their own?</p>
			<Segmented label="Autonomy level" bind:value={() => autonomy, pick} options={[{ value: 'Ask', label: 'Ask' }, { value: 'Balanced', label: 'Balanced' }, { value: 'Auto', label: 'Auto' }]} />
			<p class="text-sm text-muted">{AUTONOMY[autonomy]} Locked and hand-edited actions are never moved without asking.</p>
		</Card.Root>
		{#if versions.length > 1}
			<Card.Root size="sm" class="gap-3 px-4">
				<p class="text-sm font-medium">Dashboard</p>
				<Segmented label="Dashboard version" bind:value={() => version, (v) => onversion?.(v)} options={versions} />
				<p class="text-sm text-muted">Pick a plugin version, or Derived to build the view from your plan and trackers.</p>
			</Card.Root>
		{/if}
	</div>

	{#each view.sections as section (section.title)}
		<section class="flex flex-col gap-3" aria-label={section.title}>
			<h3 class="text-sm font-semibold text-fg">{section.title}</h3>
			<div class="grid gap-4 @3xl:grid-cols-2">
				{#each section.items as item, i (i)}
					<div class="min-w-0 {item.kind === 'line_chart' || item.kind === 'action_list' ? '@3xl:col-span-2' : ''}"><Render spec={item} /></div>
				{/each}
			</div>
		</section>
	{/each}
</div>
