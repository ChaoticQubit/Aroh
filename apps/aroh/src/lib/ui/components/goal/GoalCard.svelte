<script lang="ts">
	import Check from '@lucide/svelte/icons/check';
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';
	import type { Snippet } from 'svelte';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import { Input } from '$lib/ui/primitives/input';
	import { Switch } from '$lib/ui/primitives/switch';
	import Fold from '../Fold.svelte';
	import Render from '../Render.svelte';
	import Segmented from '../Segmented.svelte';
	import StatusChip from '../StatusChip.svelte';
	import Persona from '../agent/Persona.svelte';
	import PersonaChat from '../agent/PersonaChat.svelte';
	import type { PlanContext } from '../plan/PlanCard.svelte';
	import AgentControls from './AgentControls.svelte';
	import Attachments, { type Attachment } from './Attachments.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDay } from '$lib/ui/format';
	import type { ChatMessage, OnChange, ViewSpec } from '$lib/ui/contract';
	import type { CoachView, GoalHeader, PluginUsed } from '$lib/ui/screens/types';
	import type { AgentSettings } from '$lib/app/newgoal';
	import type { GoalStatus } from '$lib/app/goalcard';
	import type { RunAnswer, RunView } from '$lib/app/engine';
	import type { Speech } from '$lib/app/speech.svelte';

	/**
	 * The goal page (Q72, Q76): header and status card (Stop, Retry, Change time limit), then
	 * cards: Overview, Plan (open while agents wait, closed after approval; "Keep sessions in
	 * order" through `ordered` and `onordered`, Q79), Questions and answers
	 * (`onanswers`; open for Run again with `rerun`, `onrerun`, `oncancelrerun`), Research with
	 * files, Agent activity (`steps`), Coaches and plugins (`coaches`, `plugins`, `helpersLoading`, `oncontrols`)
	 * and Chat, where questions and the plan to approve appear.
	 */
	let {
		goal,
		view,
		status,
		steps = [],
		research = null,
		answers = [],
		messages,
		onchat,
		voice,
		controls,
		defaultLimit = 5,
		oncontrols,
		coaches = [],
		plugins = [],
		helpersLoading = false,
		oncoaches,
		versions = [],
		version = '',
		onversion,
		onstop,
		onretry,
		onanswers,
		rerun = false,
		onrerun,
		oncancelrerun,
		files = [],
		upload = false,
		onfiles,
		onlink,
		onretryfile,
		onremove,
		planContext,
		ordered = false,
		onordered,
		children
	}: {
		goal: GoalHeader;
		view: ViewSpec;
		status: GoalStatus;
		steps?: { text: string; done: boolean }[];
		research?: RunView['research'];
		answers?: RunAnswer[];
		messages: ChatMessage[];
		onchat?: OnChange;
		voice?: Speech;
		controls: AgentSettings;
		defaultLimit?: number;
		oncontrols?: (value: AgentSettings) => void;
		coaches?: CoachView[];
		plugins?: PluginUsed[];
		helpersLoading?: boolean;
		oncoaches?: () => void;
		versions?: { value: string; label: string }[];
		version?: string;
		onversion?: (value: string) => void;
		onstop?: () => void;
		onretry?: () => void;
		onanswers?: (changed: RunAnswer[]) => void;
		rerun?: boolean;
		onrerun?: (answers: RunAnswer[]) => void;
		oncancelrerun?: () => void;
		files?: Attachment[];
		upload?: boolean;
		onfiles?: (files: File[]) => void;
		onlink?: (url: string) => void;
		onretryfile?: (id: string) => void;
		onremove?: (id: string) => void;
		planContext?: PlanContext;
		ordered?: boolean;
		onordered?: (on: boolean) => void;
		children?: Snippet;
	} = $props();

	const id = $props.id();
	const { zone } = getClock();
	let edits = $state<Record<string, string>>({});
	const changed = $derived(answers.filter((a) => edits[a.id] !== undefined && edits[a.id].trim() !== (a.answer ?? '')).map((a) => ({ ...a, answer: edits[a.id].trim() || null })));
	const limit = $derived(controls.runLimit ?? defaultLimit);
	const waiting = $derived(status.chip === 'Waiting for you');
	const planSections = $derived(view.sections.filter((x) => x.items.some((i) => i.kind === 'action_list')));
	const overviewSections = $derived(view.sections.filter((x) => !planSections.includes(x)));
	const sessions = $derived(planSections.flatMap((x) => x.items).reduce((n, i) => n + (i.kind === 'action_list' ? i.actions.length : 0), 0));
	const fresh = $derived(coaches.some((c) => c.fresh) || plugins.some((p) => p.fresh));
	const overviewSummary = $derived(goal.deadline ? `By ${formatDay(goal.deadline, zone)}` : `${view.sections.length} sections`);
	const planSummary = $derived(waiting ? 'Waiting for you' : sessions === 1 ? '1 upcoming session' : `${sessions} upcoming sessions`);
	const researchSummary = $derived([research ? `${research.sources.length} sources` : 'No research yet', files.length ? `${files.length} ${files.length === 1 ? 'file' : 'files'}` : ''].filter(Boolean).join(' · '));
	const activitySummary = $derived(steps.length ? (steps.find((x) => !x.done)?.text ?? `${steps.length} steps done`) : 'No run yet');
	const helperSummary = $derived(
		[helpersLoading ? 'Loading coaches' : coaches.length === 1 ? '1 coach' : `${coaches.length} coaches`, plugins.length ? (plugins.length === 1 ? '1 plugin' : `${plugins.length} plugins`) : '', `${controls.autonomy[0].toUpperCase()}${controls.autonomy.slice(1)}`, `${limit} min limit`].filter(Boolean).join(' · ')
	);
	let folds = $state({ overview: true, plan: false, answers: false, research: false, activity: false, helpers: false, chat: false });
	let open = $state(false);
	$effect(() => {
		folds.plan = waiting;
		if (waiting) folds.chat = true;
	});
	$effect(() => {
		if (open) {
			folds.helpers = true;
			open = false;
		}
	});

	/** Sends the changed answers and clears the edits. */
	function send() {
		onanswers?.(changed);
		edits = {};
	}

	/** Sends every answer, edited ones changed, to Run again and clears the edits. */
	function again() {
		onrerun?.(answers.map((a) => (edits[a.id] === undefined ? a : { ...a, answer: edits[a.id].trim() || null })));
		edits = {};
	}

	/** Leaves Run again edit mode with the saved answers back in the fields. */
	function cancelAgain() {
		edits = {};
		oncancelrerun?.();
	}

	$effect(() => {
		if (rerun) folds.answers = true;
	});

	$effect(() => {
		if (rerun) document.getElementById(`${id}-answers`)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
	});
</script>

<div class="flex w-full max-w-5xl flex-col gap-4 p-4 @3xl:p-6">
	<header class="flex flex-col gap-1">
		<p class="flex items-center gap-2 text-xs font-medium text-muted"><span class="size-2 rounded-full" style="background: var(--cat-{goal.slot})" aria-hidden="true"></span>{goal.category}</p>
		<h2 class="text-xl font-semibold">{view.title}</h2>
		{#if goal.deadline || files.length}<p class="text-sm text-muted">{[goal.deadline ? `By ${formatDay(goal.deadline, zone)}` : '', files.length ? `${files.length} ${files.length === 1 ? 'attachment' : 'attachments'}` : ''].filter(Boolean).join(' · ')}</p>{/if}
	</header>

	<Card.Root size="sm" class="gap-2 px-4" role="status" aria-label="Agent status">
		<div class="flex items-center gap-2">
			{#if status.stop}<LoaderCircle class="size-4 animate-spin text-status-planned" aria-hidden="true" />{/if}
			<StatusChip tone={status.tone} label={status.chip} />
		</div>
		<p class="text-md font-semibold">{status.title}</p>
		<p class="text-sm text-muted tabular-nums">{status.detail}</p>
		{#if status.stop}
			<div class="flex"><Button variant="outline" size="touch" onclick={() => onstop?.()}>Stop</Button></div>
		{:else if status.retry}
			<div class="flex gap-2"><Button size="touch" onclick={() => onretry?.()}>Retry</Button>{#if status.limit}<Button variant="ghost" size="touch" onclick={() => (open = true)}>Change time limit</Button>{/if}</div>
		{/if}
	</Card.Root>

	<Fold title="Overview" summary={overviewSummary} bind:open={folds.overview}>
		{#each overviewSections as section (section.title)}
			<section class="flex flex-col gap-3" aria-label={section.title}>
				<h4 class="text-sm font-semibold text-fg">{section.title}</h4>
				<div class="grid gap-4 @3xl:grid-cols-2">
					{#each section.items as item, i (i)}
						<div class="min-w-0 {item.kind === 'line_chart' ? '@3xl:col-span-2' : ''}"><Render spec={item} /></div>
					{/each}
				</div>
			</section>
		{:else}
			<p class="text-sm text-muted">Progress shows here once sessions are logged.</p>
		{/each}
		{@render children?.()}
	</Fold>

	<Fold title="Plan" summary={planSummary} bind:open={folds.plan}>
		{#if waiting}<p class="text-sm">The agents wait for you in Chat below: answer their questions or approve the plan.</p>{/if}
		{#each planSections as section (section.title)}
			<section class="flex flex-col gap-3" aria-label={section.title}>
				<h4 class="text-sm font-semibold text-fg">{section.title}</h4>
				{#each section.items as item, i (i)}<div class="min-w-0"><Render spec={item} /></div>{/each}
			</section>
		{:else}
			{#if !waiting}<p class="text-sm text-muted">No sessions planned yet.</p>{/if}
		{/each}
		{#if onordered}
			<label class="flex min-h-14 cursor-pointer items-center gap-3 rounded-sm border border-border bg-bg px-3 py-2" for="{id}-ordered">
				<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Keep sessions in order</span><span class="text-xs text-muted">A missed session moves ahead and later ones shift forward.</span></span>
				<Switch id="{id}-ordered" checked={ordered} onCheckedChange={(on) => onordered(on)} />
			</label>
		{/if}
		{#if versions.length > 1}
			<div class="flex flex-col gap-2">
				<p class="text-sm font-medium">Dashboard version</p>
				<Segmented label="Dashboard version" bind:value={() => version, (v) => onversion?.(v)} options={versions} />
			</div>
		{/if}
	</Fold>

	{#if answers.length}
		<div id="{id}-answers" class="scroll-mt-4"></div>
		<Fold title="Questions and answers" summary={answers.length === 1 ? '1 answer' : `${answers.length} answers`} bind:open={folds.answers}>
			<ul class="flex flex-col gap-3">
				{#each answers as a (a.id)}
					<li class="flex flex-col gap-1">
						<label for="{id}-a-{a.id}" class="text-sm">{a.text}</label>
						<Input id="{id}-a-{a.id}" class="h-11 text-lg" placeholder="Skipped" value={edits[a.id] ?? a.answer ?? ''} oninput={(e) => (edits[a.id] = e.currentTarget.value)} />
					</li>
				{/each}
			</ul>
			{#if rerun}
				<div class="flex flex-wrap gap-2"><Button size="touch" onclick={again}>Run again with these answers</Button><Button variant="outline" size="touch" onclick={cancelAgain}>Cancel</Button></div>
			{:else}
				<div class="flex"><Button variant="outline" size="touch" disabled={!changed.length} onclick={send}>Replan with these answers</Button></div>
			{/if}
		</Fold>
	{/if}

	<Fold title="Research" summary={researchSummary} bind:open={folds.research}>
		{#if research}
			<div class="flex flex-col gap-2">
				<p class="text-sm font-medium">{status.retry ? 'Found before it stopped' : 'What the agents found'}</p>
				<p class="text-sm">{research.summary}</p>
				{#if research.sources.length}
					<ul class="flex flex-col gap-1 text-sm text-muted">
						{#each research.sources as s (s.url)}<li class="break-words"><a class="underline" href={s.url} target="_blank" rel="noreferrer">{s.title}</a></li>{/each}
					</ul>
				{/if}
			</div>
		{:else}
			<p class="text-sm text-muted">No research yet.</p>
		{/if}
		{#if onlink}<Attachments {files} {upload} {onfiles} {onlink} onretry={onretryfile} {onremove} />{/if}
	</Fold>

	<Fold title="Agent activity" summary={activitySummary} bind:open={folds.activity}>
		{#if steps.length}
			<ol class="flex flex-col divide-y divide-border rounded-sm border border-border">
				{#each steps as s (s.text)}
					<li class="flex min-h-11 items-center gap-3 px-3 py-2 text-sm text-fg">
						{#if s.done}<Check class="size-4 shrink-0 text-status-done" aria-label="Done" />{:else}<LoaderCircle class="size-4 shrink-0 animate-spin text-status-planned" aria-label="Running" />{/if}
						<span class="min-w-0 break-words">{s.text}</span>
					</li>
				{/each}
			</ol>
		{:else}
			<p class="text-sm text-muted">No agent run on this goal yet.</p>
		{/if}
	</Fold>

	<Fold title="Coaches and plugins" summary={helperSummary} bind:open={folds.helpers}>
		{#snippet badge()}{#if fresh}<span class="rounded-sm bg-fg px-1.5 text-xs leading-5 font-medium text-bg">New</span>{/if}{/snippet}
		{#if coaches.length || plugins.length}
			<ul class="flex flex-col divide-y divide-border rounded-sm border border-border">
				{#each coaches as c (c.name)}
					<li class="flex min-h-14 items-center gap-3 px-3 py-2">
						<span class="min-w-0 flex-1"><Persona name={c.display_name} note={c.role} /></span>
						{#if c.fresh}<span class="shrink-0 rounded-sm border border-border px-1.5 text-xs leading-5 font-medium">New</span>{/if}
						{#if c.runs}<span class="shrink-0 text-sm text-muted tabular-nums">{c.runs === 1 ? '1 run' : `${c.runs} runs`}</span>{/if}
					</li>
				{/each}
				{#each plugins as p (p.name)}
					<li class="flex min-h-14 items-center gap-3 px-3 py-2">
						<span class="flex min-w-0 flex-1 flex-col"><span class="text-sm font-medium break-words">{p.name}</span><span class="text-xs text-muted">Plugin, v{p.version}</span></span>
						{#if p.fresh}<span class="shrink-0 rounded-sm border border-border px-1.5 text-xs leading-5 font-medium">New</span>{/if}
						{#if p.runs}<span class="shrink-0 text-sm text-muted tabular-nums">{p.runs === 1 ? '1 run' : `${p.runs} runs`}</span>{/if}
					</li>
				{/each}
			</ul>
		{:else}
			<p class="text-sm text-muted">No coach has worked on this goal yet.</p>
		{/if}
		{#if oncoaches}<div class="flex"><Button variant="outline" size="touch" onclick={() => oncoaches?.()}>All coaches</Button></div>{/if}
		<div class="flex flex-col gap-2 border-t border-border pt-3">
			<p class="text-sm font-medium">Agent controls</p>
			<AgentControls value={controls} limit="goal" {defaultLimit} onchange={(v) => oncontrols?.(v)} />
			<p class="text-xs text-muted">Each run stops after {limit} min of active work. Locked and hand-edited actions are never moved without asking.</p>
		</div>
	</Fold>

	<Fold title="Chat" summary="Tell the agents anything about this goal." bind:open={folds.chat}>
		<div class="min-w-0"><PersonaChat {messages} onchange={onchat} echo={false} {voice} inline {planContext} slot={goal.slot} keep="goal:{goal.id}" /></div>
	</Fold>
</div>
