<script lang="ts">
	import Check from '@lucide/svelte/icons/check';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import Circle from '@lucide/svelte/icons/circle';
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';
	import Mic from '@lucide/svelte/icons/mic';
	import Paperclip from '@lucide/svelte/icons/paperclip';
	import Send from '@lucide/svelte/icons/send';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import { Input } from '$lib/ui/primitives/input';
	import { Progress } from '$lib/ui/primitives/progress';
	import Render from '$lib/ui/components/Render.svelte';
	import StatusChip, { type Tone } from '$lib/ui/components/StatusChip.svelte';
	import NumberInput from '$lib/ui/components/inputs/NumberInput.svelte';
	import Choice from '$lib/ui/components/inputs/Choice.svelte';
	import Persona from '$lib/ui/components/agent/Persona.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDay, formatTime } from '$lib/ui/format';
	import AgentControls from '$lib/ui/components/goal/AgentControls.svelte';
	import { blankDraft } from '$lib/app/newgoal';
	import ChapterWheel from './ChapterWheel.svelte';
	import StillTrue from '$lib/ui/components/goal/StillTrue.svelte';
	import { attachments, book, chat, found, proposed, steps, type Phase } from './data';

	/**
	 * The one goal card: what the goal is, a status header saying what agents do or wait for, the
	 * phase body (research steps, questions with one Submit, the plan to approve, or progress and
	 * upcoming sessions), the goal's chat with a pinned composer, and a compact agents row that
	 * opens the agents sheet. `sheet` starts with that sheet open. `known` shows profile facts
	 * already learned as "Still true?" rows instead of asking them again.
	 */
	let { phase, sheet = false, known = false }: { phase: Phase; sheet?: boolean; known?: boolean } = $props();

	const id = $props.id();
	const { zone } = getClock();
	const CHAPTERS = 24;
	const PAGES = 320;
	const PAGES_PER_HOUR = 35;
	const FROM_CHAPTER = 6;
	const STATUS: Record<Phase, { tone: Tone; chip: string; title: string; detail: string }> = {
		researching: { tone: 'planned', chip: 'Working', title: "Researching: looking up the book's chapters", detail: '0:42 of 5:00 · $0.01 so far' },
		questions: { tone: 'warn', chip: 'Waiting for you', title: 'Waiting for 3 answers below', detail: 'Research took 1:12 · $0.02. The timer is paused while you answer.' },
		stopped: { tone: 'skipped', chip: 'Stopped', title: 'Stopped after 5 min', detail: 'Reached the run time limit. Nothing more was spent: $0.04 in total.' },
		plan: { tone: 'warn', chip: 'Waiting for you', title: 'Plan ready to approve', detail: 'Nothing is on your calendar until you approve. 2:05 · $0.03 in total.' },
		active: { tone: 'done', chip: 'On track', title: 'Agents are idle', detail: 'Next: weekly review on Sun 29 Nov at 18:00. Last run today 08:02, 1:40, $0.03.' }
	};
	const status = $derived(known && phase === 'questions' ? { ...STATUS.questions, title: 'Waiting for 1 answer and 2 checks below' } : STATUS[phase]);
	const shown = $derived(phase === 'stopped' ? found.slice(0, 2) : found);
	let minutes = $state(30);
	let open = $derived(sheet);
	let draft = $state('');
	let composer = $state<HTMLInputElement | null>(null);

	const finish = $derived.by(() => {
		const left = (PAGES * (CHAPTERS - FROM_CHAPTER + 1)) / CHAPTERS;
		const days = Math.ceil(((left / PAGES_PER_HOUR) * 60) / Math.max(1, minutes));
		const at = new Date(Date.UTC(2026, 10, 26 + days - 1));
		return { days, label: at.toLocaleDateString('en-GB', { day: 'numeric', month: 'short', timeZone: 'UTC' }) };
	});
</script>

{#snippet minutesInput()}
	<div class="flex flex-col gap-1">
		<NumberInput spec={{ kind: 'number_input', label: 'Minutes a day for reading', unit: 'min', value: minutes, step: 5, min: 10, max: 120 }} onchange={(v) => (minutes = typeof v === 'number' ? v : 30)} />
		<p class="text-xs text-muted tabular-nums">At {minutes} min a day you finish about {finish.label}.</p>
	</div>
{/snippet}
{#snippet whenInput()}
	<Choice spec={{ kind: 'choice', label: 'When do you like to read?', options: ['Morning', 'Lunch', 'Evening', 'Before bed'], multi: true, value: ['Evening'] }} />
{/snippet}

<div class="flex min-h-full flex-col">
	<div class="flex flex-1 flex-col gap-4 p-4">
		<header class="flex flex-col gap-1">
			<p class="flex items-center gap-2 text-xs font-medium text-muted"><span class="size-2 rounded-full" style="background: var(--cat-{book.slot})" aria-hidden="true"></span>{book.category}</p>
			<h2 class="text-xl font-semibold">{book.title}</h2>
			<p class="text-sm text-muted">By 31 Dec · 35 days left · {attachments.length} attachments</p>
		</header>

		<Card.Root size="sm" class="gap-2 px-4" role="status" aria-label="Agent status">
			<div class="flex items-center gap-2">
				{#if phase === 'researching'}<LoaderCircle class="size-4 animate-spin text-status-planned" aria-hidden="true" />{/if}
				<StatusChip tone={status.tone} label={status.chip} />
			</div>
			<p class="text-md font-semibold">{status.title}</p>
			<p class="text-sm text-muted tabular-nums">{status.detail}</p>
			{#if phase === 'researching'}
				<div class="flex"><Button variant="outline" size="touch">Stop</Button></div>
			{:else if phase === 'stopped'}
				<div class="flex gap-2"><Button size="touch">Retry</Button><Button variant="ghost" size="touch">Change time limit</Button></div>
			{/if}
		</Card.Root>

		{#if phase === 'researching'}
			<section class="flex flex-col gap-2" aria-labelledby="{id}-steps">
				<h3 id="{id}-steps" class="text-sm font-semibold">What the agents are doing</h3>
				<ol class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
					{#each steps as s (s.text)}
						<li class="flex min-h-11 items-center gap-3 px-3 py-2 text-sm {s.state === 'pending' ? 'text-muted' : 'text-fg'}">
							{#if s.state === 'done'}<Check class="size-4 shrink-0 text-status-done" aria-label="Done" />{:else if s.state === 'running'}<LoaderCircle class="size-4 shrink-0 animate-spin text-status-planned" aria-label="Running" />{:else}<Circle class="size-4 shrink-0" aria-label="Next" />{/if}
							{s.text}
						</li>
					{/each}
				</ol>
			</section>
		{:else}
			<section class="flex flex-col gap-2" aria-labelledby="{id}-found">
				<div class="flex items-baseline justify-between">
					<h3 id="{id}-found" class="text-sm font-semibold">{phase === 'stopped' ? 'Found before it stopped' : 'Research found'}</h3>
					<span class="text-xs text-muted">From your PDF and a web search</span>
				</div>
				<dl class="grid grid-cols-2 gap-px overflow-hidden rounded-sm border border-border bg-border">
					{#each shown as f (f.label)}
						<div class="flex flex-col gap-0.5 bg-bg px-3 py-2"><dt class="text-xs text-muted">{f.label}</dt><dd class="text-md font-semibold tabular-nums">{f.value}</dd></div>
					{/each}
				</dl>
				{#if phase === 'stopped'}<p class="text-sm text-muted">Not done yet: checking your calendars and drafting the plan. Retry picks up from here.</p>{/if}
			</section>
		{/if}

		{#if phase === 'questions'}
			<Card.Root size="sm" class="gap-5 px-4">
				<div class="flex flex-col gap-0.5">
					<Persona name={book.coach} note={known ? '1 question, 2 checks' : '3 questions'} />
					<p class="text-sm text-muted">Only what research could not find.</p>
				</div>
				{#if known}
					<StillTrue label="Minutes a day for reading" value="30 min" source="From your answer on Learn Spanish, 3 Nov">{@render minutesInput()}</StillTrue>
					<StillTrue label="When do you like to read?" value="Evening" source="From your edits: you often move sessions to the evening">{@render whenInput()}</StillTrue>
					<ChapterWheel count={CHAPTERS} value={FROM_CHAPTER} label="Which chapter are you on?" />
				{:else}
					<ChapterWheel count={CHAPTERS} value={FROM_CHAPTER} label="Which chapter are you on?" />
					{@render minutesInput()}
					{@render whenInput()}
				{/if}
				<div class="flex flex-col gap-1.5">
					<Button size="touch" class="w-full">Submit answers</Button>
					<p class="text-center text-xs text-muted">The agents continue planning as soon as you submit.</p>
				</div>
			</Card.Root>
		{:else if phase === 'plan'}
			<Card.Root size="sm" class="gap-3 px-4">
				<Persona name={book.coach} note="Proposed plan" />
				<ul class="flex flex-col gap-1 text-sm">
					<li>From chapter {FROM_CHAPTER}: 19 chapters, about 250 pages</li>
					<li>30 min a day, evenings at 20:00, Sundays at 10:00</li>
					<li>15 sessions, done about 10 Dec, 21 days before your date</li>
				</ul>
				<ol class="flex flex-col divide-y divide-border rounded-sm border border-dashed border-border-strong" aria-label="First sessions">
					{#each proposed as a (a.id)}
						<li class="flex min-h-11 items-center gap-3 px-3 py-2 text-sm"><span class="w-32 shrink-0 text-xs whitespace-nowrap text-muted tabular-nums">{formatDay(a.start, zone)} {formatTime(a.start, zone)}</span>{a.title}</li>
					{/each}
					<li class="px-3 py-2 text-xs text-muted">and 11 more</li>
				</ol>
				<div class="flex flex-col gap-2">
					<Button size="touch" class="w-full">Approve plan</Button>
					<Button variant="outline" size="touch" class="w-full" onclick={() => composer?.focus()}>Change something</Button>
				</div>
			</Card.Root>
		{:else if phase === 'active'}
			<Card.Root size="sm" class="gap-2 px-4">
				<div class="flex items-baseline justify-between">
					<p class="text-sm font-medium">Progress</p>
					<span class="text-xs text-muted tabular-nums">5 of 24 chapters</span>
				</div>
				<Progress value={21} aria-label="Chapters done" />
				<p class="text-sm text-muted">On pace to finish about 10 Dec, 21 days early.</p>
			</Card.Root>
			<Render spec={{ kind: 'action_list', label: 'Upcoming', actions: proposed }} />
		{/if}

		<section class="flex flex-col gap-2" aria-labelledby="{id}-chat">
			<h3 id="{id}-chat" class="text-sm font-semibold">Chat about this goal</h3>
			{#if chat[phase].length}
				<ol class="flex flex-col gap-2">
					{#each chat[phase] as m, i (i)}
						<li class="flex flex-col {m.from === 'owner' ? 'items-end' : 'items-start'}">
							{#if m.from === 'owner'}
								<p class="max-w-[85%] rounded-sm border border-border bg-surface-2 px-3 py-2 text-sm">{m.text}</p>
							{:else}
								<div class="flex max-w-[92%] flex-col gap-1"><Persona name={book.coach} /><p class="text-sm">{m.text}</p></div>
							{/if}
						</li>
					{/each}
				</ol>
			{:else}
				<p class="text-sm text-muted">Tell the agents anything while they work, for example "I read on the train". They read it at their next step.</p>
			{/if}
		</section>

		<button type="button" class="flex min-h-14 items-center gap-3 rounded-sm border border-border bg-bg px-3 py-2 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => (open = true)}>
			<span class="flex flex-1 flex-col">
				<span class="text-sm font-medium">Agents and coaches</span>
				<span class="text-xs text-muted">Balanced · Reminders on · Calendars on · {book.coach}</span>
			</span>
			<ChevronRight class="size-4 text-muted" aria-hidden="true" />
		</button>
	</div>

	<form class="sticky bottom-0 flex items-center gap-1 border-t border-border bg-bg p-2" onsubmit={(e) => (e.preventDefault(), (draft = ''))}>
		<Button variant="ghost" size="icon-touch" aria-label="Attach"><Paperclip /></Button>
		<Input bind:ref={composer} class="h-11 flex-1 text-lg" placeholder="Message {book.coach}" aria-label="Message" bind:value={draft} />
		<Button variant="ghost" size="icon-touch" aria-label="Voice input"><Mic /></Button>
		<Button type="submit" size="icon-touch" aria-label="Send" disabled={!draft.trim()}><Send /></Button>
	</form>
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
			<AgentControls value={blankDraft()} limit="goal" />
			<section class="flex flex-col gap-2" aria-labelledby="{id}-coaches">
				<h3 id="{id}-coaches" class="text-sm font-semibold">Coaches</h3>
				<ul class="flex flex-col divide-y divide-border rounded-sm border border-border">
					<li class="flex min-h-14 flex-col justify-center px-3 py-2"><span class="text-sm font-medium">{book.coach}</span><span class="text-xs text-muted">Lead. Plans sessions and asks the questions.</span></li>
					<li class="flex min-h-14 flex-col justify-center px-3 py-2"><span class="text-sm font-medium">Habit coach</span><span class="text-xs text-muted">Weekly review of missed evenings.</span></li>
				</ul>
				<div class="flex"><Button variant="outline" size="touch">Add a coach</Button></div>
			</section>
			<p class="text-xs text-muted">Each run stops after 5 min of active work. Change the limit in Settings.</p>
		</div>
	</div>
{/if}
