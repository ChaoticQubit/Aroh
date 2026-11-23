<script lang="ts">
	import Check from '@lucide/svelte/icons/check';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';
	import type { Snippet } from 'svelte';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import { Input } from '$lib/ui/primitives/input';
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
	import type { CoachView, GoalHeader } from '$lib/ui/screens/types';
	import type { AgentSettings } from '$lib/app/newgoal';
	import type { GoalStatus } from '$lib/app/goalcard';
	import type { RunAnswer, RunView } from '$lib/app/engine';
	import type { Speech } from '$lib/app/speech.svelte';

	/**
	 * The one goal card: header, a status card saying what agents do or wait for (Stop, Retry,
	 * Change time limit), research steps or what research found, the goal's view sections
	 * (progress, Upcoming, plugin dashboards), the answers given (editable, `onanswers` gets the
	 * changed ones), the attachments (`files`; Photo and File with `upload`, `onfiles`, `onlink`,
	 * `onretryfile` for a failed upload, `onremove`), the goal chat with its questions and plan, and an agents row opening a sheet
	 * with per-goal controls (`oncontrols`) and coaches.
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
		oncoaches,
		versions = [],
		version = '',
		onversion,
		onstop,
		onretry,
		onanswers,
		files = [],
		upload = false,
		onfiles,
		onlink,
		onretryfile,
		onremove,
		planContext,
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
		oncoaches?: () => void;
		versions?: { value: string; label: string }[];
		version?: string;
		onversion?: (value: string) => void;
		onstop?: () => void;
		onretry?: () => void;
		onanswers?: (changed: RunAnswer[]) => void;
		files?: Attachment[];
		upload?: boolean;
		onfiles?: (files: File[]) => void;
		onlink?: (url: string) => void;
		onretryfile?: (id: string) => void;
		onremove?: (id: string) => void;
		planContext?: PlanContext;
		children?: Snippet;
	} = $props();

	const id = $props.id();
	const { zone } = getClock();
	let open = $state(false);
	let edits = $state<Record<string, string>>({});
	const changed = $derived(answers.filter((a) => edits[a.id] !== undefined && edits[a.id].trim() !== (a.answer ?? '')).map((a) => ({ ...a, answer: edits[a.id].trim() || null })));
	const limit = $derived(controls.runLimit ?? defaultLimit);
	const summary = $derived(
		[
			controls.autonomy[0].toUpperCase() + controls.autonomy.slice(1),
			controls.reminders ? 'Reminders on' : 'Reminders off',
			controls.useCalendars ? 'Calendars on' : 'Calendars off',
			`${limit} min limit`
		].join(' · ')
	);

	/** Sends the changed answers and clears the edits. */
	function send() {
		onanswers?.(changed);
		edits = {};
	}
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
			<div class="flex gap-2"><Button size="touch" onclick={() => onretry?.()}>Retry</Button><Button variant="ghost" size="touch" onclick={() => (open = true)}>Change time limit</Button></div>
		{/if}
	</Card.Root>

	{#if status.stop && steps.length}
		<section class="flex flex-col gap-2" aria-labelledby="{id}-steps">
			<h3 id="{id}-steps" class="text-sm font-semibold">What the agents are doing</h3>
			<ol class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
				{#each steps as s (s.text)}
					<li class="flex min-h-11 items-center gap-3 px-3 py-2 text-sm text-fg">
						{#if s.done}<Check class="size-4 shrink-0 text-status-done" aria-label="Done" />{:else}<LoaderCircle class="size-4 shrink-0 animate-spin text-status-planned" aria-label="Running" />{/if}
						{s.text}
					</li>
				{/each}
			</ol>
		</section>
	{/if}
	{#if research}
		<section class="flex flex-col gap-2" aria-labelledby="{id}-found">
			<h3 id="{id}-found" class="text-sm font-semibold">{status.retry ? 'Found before it stopped' : 'Research found'}</h3>
			<p class="text-sm">{research.summary}</p>
			{#if research.sources.length}
				<ul class="flex flex-col gap-1 text-xs text-muted">
					{#each research.sources as s (s.url)}<li><a class="underline" href={s.url} target="_blank" rel="noreferrer">{s.title}</a></li>{/each}
				</ul>
			{/if}
		</section>
	{/if}

	{#if versions.length > 1}
		<Card.Root size="sm" class="gap-3 px-4">
			<p class="text-sm font-medium">Dashboard</p>
			<Segmented label="Dashboard version" bind:value={() => version, (v) => onversion?.(v)} options={versions} />
		</Card.Root>
	{/if}

	{@render children?.()}

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

	{#if answers.length}
		<section class="flex flex-col gap-2" aria-labelledby="{id}-answers">
			<h3 id="{id}-answers" class="text-sm font-semibold">Your answers</h3>
			<ul class="flex flex-col gap-3">
				{#each answers as a (a.id)}
					<li class="flex flex-col gap-1">
						<label for="{id}-a-{a.id}" class="text-sm">{a.text}</label>
						<Input id="{id}-a-{a.id}" class="h-11 text-lg" placeholder="Skipped" value={edits[a.id] ?? a.answer ?? ''} oninput={(e) => (edits[a.id] = e.currentTarget.value)} />
					</li>
				{/each}
			</ul>
			<div class="flex"><Button variant="outline" size="touch" disabled={!changed.length} onclick={send}>Replan with these answers</Button></div>
		</section>
	{/if}

	{#if onlink}<Attachments {files} {upload} {onfiles} {onlink} onretry={onretryfile} {onremove} />{/if}

	<button type="button" class="flex min-h-14 items-center gap-3 rounded-sm border border-border bg-bg px-3 py-2 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => (open = true)}>
		<span class="flex flex-1 flex-col">
			<span class="text-sm font-medium">Agents and coaches</span>
			<span class="text-xs text-muted">{summary}</span>
		</span>
		<ChevronRight class="size-4 text-muted" aria-hidden="true" />
	</button>

	<section class="flex flex-col gap-2" aria-labelledby="{id}-chat">
		<h3 id="{id}-chat" class="text-sm font-semibold">Chat about this goal</h3>
		<PersonaChat {messages} onchange={onchat} echo={false} {voice} inline {planContext} slot={goal.slot} />
	</section>
</div>

{#if open}
	<div class="fixed inset-0 z-40 flex flex-col justify-end">
		<button type="button" class="flex-1 bg-scrim outline-none" aria-label="Close" tabindex="-1" onclick={() => (open = false)}></button>
		<div role="dialog" aria-labelledby="{id}-sheet" class="flex max-h-[85%] flex-col gap-4 overflow-y-auto rounded-t-[var(--radius-dialog)] border-t border-border bg-bg px-4 pt-2 pb-4">
			<span class="mx-auto h-1 w-9 shrink-0 rounded-full bg-border-strong" aria-hidden="true"></span>
			<div class="flex items-center justify-between">
				<h2 id="{id}-sheet" class="text-lg font-semibold">Agents on this goal</h2>
				<Button variant="ghost" size="touch" onclick={() => (open = false)}>Done</Button>
			</div>
			<AgentControls value={controls} limit="goal" {defaultLimit} onchange={(v) => oncontrols?.(v)} />
			<section class="flex flex-col gap-2" aria-labelledby="{id}-coaches">
				<h3 id="{id}-coaches" class="text-sm font-semibold">Coaches</h3>
				{#if coaches.length}
					<ul class="flex flex-col divide-y divide-border rounded-sm border border-border">
						{#each coaches as c (c.name)}<li class="flex min-h-14 items-center px-3 py-2"><Persona name={c.display_name} note={c.role} /></li>{/each}
					</ul>
				{:else}
					<p class="text-sm text-muted">No coach has worked on this goal yet.</p>
				{/if}
				{#if oncoaches}<div class="flex"><Button variant="outline" size="touch" onclick={() => oncoaches?.()}>All coaches</Button></div>{/if}
			</section>
			<p class="text-xs text-muted">Each run stops after {limit} min of active work. Locked and hand-edited actions are never moved without asking.</p>
		</div>
	</div>
{/if}
