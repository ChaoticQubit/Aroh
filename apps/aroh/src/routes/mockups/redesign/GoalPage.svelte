<script lang="ts">
	import Bot from '@lucide/svelte/icons/bot';
	import Check from '@lucide/svelte/icons/check';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import CircleDot from '@lucide/svelte/icons/circle-dot';
	import FileText from '@lucide/svelte/icons/file-text';
	import Link from '@lucide/svelte/icons/link';
	import Mic from '@lucide/svelte/icons/mic';
	import Puzzle from '@lucide/svelte/icons/puzzle';
	import SendHorizontal from '@lucide/svelte/icons/send-horizontal';
	import Wrench from '@lucide/svelte/icons/wrench';
	import { Button } from '$lib/ui/primitives/button';
	import { Input } from '$lib/ui/primitives/input';
	import StatusChip from '$lib/ui/components/StatusChip.svelte';
	import PlanCard, { type PlanContext } from '$lib/ui/components/plan/PlanCard.svelte';
	import type { BusyBlock } from '$lib/app/client';
	import { shiftDay } from '$lib/ui/format';
	import Fold from '$lib/ui/components/Fold.svelte';
	import type { GoalSeed } from './data';

	/**
	 * The redesigned goal page (Q72): header, then Overview, Plan, Questions and answers, Research,
	 * Agent activity, Coaches and plugins and Chat as collapsible cards. `expand` opens every card
	 * (the long-data frames); otherwise Overview and Plan open, Questions only while one waits.
	 */
	let { goal, expand = false }: { goal: GoalSeed; expand?: boolean } = $props();

	const at = (d: string, hm: string) => `${d}T${hm}:00Z`;
	const busy: BusyBlock[] = [];
	for (let d = '2026-11-23'; d <= '2026-12-20'; d = shiftDay(d, 1)) {
		const wd = new Date(`${d}T12:00:00Z`).getUTCDay();
		if (wd >= 1 && wd <= 5) busy.push({ id: `w${d}`, calendar_id: 'c', event_id: `w${d}`, title: 'Work', start_at: at(d, '09:00'), end_at: at(d, '17:00'), all_day: false, group_name: 'Work calendar', day_kind: null });
	}
	const context = $derived<PlanContext>({ busy, actions: [], goalId: 'g', deadline: goal.slot === 1 ? '2026-12-31' : '2027-03-14', knownUntil: '2026-12-10' });
	const reading = [
		{ title: 'Read a chapter', start: '2026-11-26T18:00', minutes: 20, rrule: 'FREQ=DAILY;COUNT=19' }
	];
	const running = [
		{ title: 'Easy run', start: '2026-11-26T07:00', minutes: 40, rrule: 'FREQ=WEEKLY;BYDAY=MO,TH;COUNT=30' },
		{ title: 'Strength for runners', start: '2026-11-27T18:30', minutes: 30, rrule: 'FREQ=WEEKLY;BYDAY=FR,TU;COUNT=30' },
		{ title: 'Long run', start: '2026-11-29T08:00', minutes: 85, rrule: 'FREQ=WEEKLY;BYDAY=SU;COUNT=15' }
	];
	const items = $derived(goal.slot === 1 ? reading : running);
	const pct = $derived(Math.round((goal.progress.done / goal.progress.total) * 100));
	const open = $derived({ overview: true, plan: true, questions: expand || goal.waiting, research: expand, activity: expand, helpers: expand, chat: expand });
	const asked = $derived(goal.answers.filter((a) => a.a === null).length);
	const coaches = $derived(goal.helpers.filter((h) => h.kind === 'coach'));
	const plugins = $derived(goal.helpers.filter((h) => h.kind === 'plugin'));
	const fresh = $derived(goal.helpers.filter((h) => h.fresh).length);
	const steps = $derived(goal.runs.reduce((n, r) => n + r.steps.length, 0));
	const STATE = { done: { tone: 'done', label: 'Done' }, waiting: { tone: 'warn', label: 'Waiting for you' }, stopped: { tone: 'skipped', label: 'Stopped' } } as const;
</script>

{#snippet newBadge()}<span class="rounded-sm border border-status-planned/40 bg-status-planned/10 px-1.5 text-xs leading-5 font-medium text-status-planned">New</span>{/snippet}

<div class="flex flex-col gap-3 p-4">
	<header class="flex flex-col gap-1 pb-1">
		<h2 class="text-2xl leading-tight font-semibold">{goal.title}</h2>
		<p class="flex items-center gap-2 text-sm text-muted"><span class="size-2 shrink-0 rounded-full" style="background: var(--cat-{goal.slot})" aria-hidden="true"></span>{goal.category} · by {goal.deadline}</p>
	</header>

	<Fold title="Overview" open={open.overview} summary="{goal.status.chip} · {goal.progress.done} of {goal.progress.total} {goal.progress.unit}">
		<div class="flex items-center gap-2"><StatusChip tone={goal.status.tone} label={goal.status.chip} /><p class="min-w-0 text-sm text-muted">{goal.status.line}</p></div>
		<div class="flex flex-col gap-1.5">
			<div class="flex items-baseline justify-between gap-2"><p class="text-md font-semibold tabular-nums">{goal.progress.done} of {goal.progress.total} {goal.progress.unit}</p><p class="text-sm text-muted tabular-nums">{pct}%</p></div>
			<div class="h-1.5 overflow-hidden rounded-full bg-surface-2" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100} aria-label="Progress"><div class="h-full rounded-full bg-fg" style="width: {pct}%"></div></div>
			<p class="text-sm text-muted">{goal.progress.finish}, {goal.progress.pace}</p>
		</div>
		<div class="flex flex-col">
			<p class="text-xs font-medium text-muted">Next sessions</p>
			<ul class="flex flex-col divide-y divide-border">
				{#each goal.next as s (s.day + s.title)}
					<li class="flex min-h-12 items-center gap-3 py-2">
						<span class="w-14 shrink-0 text-sm tabular-nums"><span class="block font-medium">{s.day}</span><span class="text-muted">{s.time}</span></span>
						<span class="min-w-0 flex-1 text-sm" style="border-left: 2px solid var(--cat-{goal.slot}); padding-left: 8px">{s.title}</span>
						<span class="shrink-0 text-xs text-muted tabular-nums">{s.minutes} min</span>
					</li>
				{/each}
			</ul>
		</div>
	</Fold>

	<Fold title="Plan" open={open.plan} summary="This week and later weeks">
		<div class="-mx-4 -mb-4 [&>*]:rounded-none [&>*]:border-0">
			<PlanCard title="" plan={{ items, timezone: 'UTC' }} {context} slot={goal.slot} approved />
		</div>
	</Fold>

	<Fold title="Questions and answers" open={open.questions} summary={asked ? `${asked} waiting for you` : `${goal.answers.length} answered`}>
		{#snippet badge()}{#if asked}<StatusChip tone="warn" label="{asked} waiting" />{/if}{/snippet}
		<ul class="flex flex-col gap-3">
			{#each goal.answers as qa (qa.q)}
				<li class="flex flex-col gap-1">
					<p class="text-sm">{qa.q}</p>
					{#if qa.a === null}
						<div class="flex items-center gap-2"><Input class="h-11 text-lg" placeholder={qa.unit ? `Minutes` : 'Your answer'} />{#if qa.unit}<span class="text-sm text-muted">{qa.unit}</span>{/if}</div>
					{:else}
						<p class="rounded-sm bg-surface-2 px-3 py-2 text-sm">{qa.a}{qa.unit ? ` ${qa.unit}` : ''}</p>
					{/if}
				</li>
			{/each}
		</ul>
		{#if asked}<div class="flex gap-2"><Button size="touch" class="flex-1">Submit answers</Button><Button variant="outline" size="touch">Skip</Button></div>{:else}<p class="text-xs text-muted">Tap an answer to change it. Changed answers replan the goal.</p>{/if}
	</Fold>

	<Fold title="Research" open={open.research} summary="{goal.research.sources.length} sources{goal.research.files.length ? ` · ${goal.research.files.length} of your files` : ''}">
		<p class="text-sm">{goal.research.summary}</p>
		<ol class="flex flex-col divide-y divide-border">
			{#each goal.research.sources as s, i (s.title)}
				<li class="flex gap-3 py-2">
					<span class="w-4 shrink-0 text-xs leading-5 text-muted tabular-nums">{i + 1}</span>
					<span class="flex min-w-0 flex-col"><span class="text-sm font-medium underline decoration-border-strong underline-offset-2">{s.title}</span><span class="truncate text-xs text-muted">{s.site} · {s.note}</span></span>
				</li>
			{/each}
		</ol>
		<div class="flex flex-col gap-2">
			<p class="text-xs font-medium text-muted">Your files</p>
			{#if goal.research.files.length}
				<ul class="flex flex-col gap-1.5">
					{#each goal.research.files as f (f)}<li class="flex items-center gap-2 text-sm">{#if f.includes('.com')}<Link class="size-4 text-muted" aria-hidden="true" />{:else}<FileText class="size-4 text-muted" aria-hidden="true" />{/if}{f}</li>{/each}
				</ul>
			{:else}
				<p class="text-sm text-muted">None yet. Agents read what you add.</p>
			{/if}
			<div class="flex gap-2"><Button variant="outline" size="touch">Add file</Button><Button variant="outline" size="touch">Add link</Button></div>
		</div>
	</Fold>

	<Fold title="Agent activity" open={open.activity} summary="{goal.runs.length} {goal.runs.length === 1 ? 'run' : 'runs'} · {steps} steps · last {goal.runs[0].when}">
		{#each goal.runs as r (r.title + r.when)}
			<div class="flex flex-col gap-2">
				<div class="flex items-center gap-2">
					<p class="min-w-0 flex-1 text-sm font-semibold">{r.title}</p>
					<StatusChip tone={STATE[r.state].tone} label={STATE[r.state].label} />
				</div>
				<p class="text-xs text-muted tabular-nums">{r.when} · {r.cost}</p>
				<ol class="flex flex-col border-l border-border pl-3">
					{#each r.steps as s (s.step + s.output)}
						<li class="relative flex flex-col gap-0.5 py-1.5">
							<span class="absolute top-2.5 -left-[17px] flex size-2.5 items-center justify-center rounded-full bg-bg" aria-hidden="true">{#if r.state === 'waiting' && s === r.steps[r.steps.length - 1]}<CircleDot class="size-2.5 text-status-warn" />{:else}<Check class="size-2.5 text-status-done" />{/if}</span>
							<p class="text-sm"><span class="font-medium">{s.step}</span><span class="text-muted">{` · ${s.coach} · ${s.seconds} s`}</span></p>
							<p class="text-sm text-muted">{s.output}</p>
							{#if s.tools.length}<p class="flex flex-wrap gap-1">{#each s.tools as t, i (i)}<span class="rounded-sm bg-surface-2 px-1.5 text-xs leading-5 text-muted">{t}</span>{/each}</p>{/if}
						</li>
					{/each}
				</ol>
			</div>
		{/each}
	</Fold>

	<Fold title="Coaches and plugins" open={open.helpers} summary="{coaches.length} coaches · {plugins.length} {plugins.length === 1 ? 'plugin' : 'plugins'}{fresh ? ` · ${fresh} new` : ''}">
		{#snippet badge()}{#if fresh}{@render newBadge()}{/if}{/snippet}
		{#each [{ label: 'Coaches', list: coaches }, { label: 'Plugins', list: plugins }] as g (g.label)}
			<div class="flex flex-col">
				<p class="text-xs font-medium text-muted">{g.label}</p>
				<ul class="flex flex-col divide-y divide-border">
					{#each g.list as h (h.name)}
						<li class="flex min-h-14 items-center gap-3 py-2">
							<span class="flex size-8 shrink-0 items-center justify-center rounded-full bg-surface-2" aria-hidden="true">{#if h.kind === 'coach'}<Bot class="size-4" />{:else}<Puzzle class="size-4" />{/if}</span>
							<span class="flex min-w-0 flex-1 flex-col">
								<span class="flex items-center gap-2 text-sm font-medium">{h.name}{#if h.version}<span class="text-xs font-normal text-muted">v{h.version}</span>{/if}{#if h.fresh}{@render newBadge()}{/if}</span>
								<span class="text-xs text-muted">{h.role} · {h.runs} {h.runs === 1 ? 'run' : 'runs'}</span>
							</span>
							<ChevronRight class="size-4 shrink-0 text-muted" aria-hidden="true" />
						</li>
					{/each}
				</ul>
			</div>
		{/each}
		<p class="text-xs text-muted">New means an agent created it while working on this goal.</p>
		<button type="button" class="flex min-h-12 items-center gap-3 rounded-sm border border-border px-3 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50">
			<Wrench class="size-4 text-muted" aria-hidden="true" /><span class="flex flex-1 flex-col"><span class="text-sm font-medium">Agent controls</span><span class="text-xs text-muted">Balanced · Reminders on · Calendars on · 5 min limit</span></span><ChevronRight class="size-4 text-muted" aria-hidden="true" />
		</button>
	</Fold>

	<Fold title="Chat" open={open.chat} summary="{goal.chat[goal.chat.length - 1].who}: {goal.chat[goal.chat.length - 1].text}">
		<ul class="flex flex-col gap-2">
			{#each goal.chat as m, i (i)}
				<li class="flex flex-col gap-0.5 {m.who === 'You' ? 'items-end' : 'items-start'}">
					{#if m.who !== 'You'}<span class="text-xs text-muted">{m.who}</span>{/if}
					<p class="max-w-[85%] rounded-sm px-3 py-2 text-sm {m.who === 'You' ? 'bg-fg text-bg' : 'bg-surface-2'}">{m.text}</p>
				</li>
			{/each}
		</ul>
		<div class="flex items-center gap-2"><Input class="h-11 text-lg" placeholder="Ask or change something" /><Button variant="ghost" size="icon-touch" aria-label="Speak"><Mic /></Button><Button size="icon-touch" aria-label="Send"><SendHorizontal /></Button></div>
	</Fold>

	<button type="button" class="mt-2 min-h-11 w-full rounded-sm border border-border bg-bg text-sm text-muted outline-none focus-visible:ring-3 focus-visible:ring-ring/50">Delete goal</button>
</div>
