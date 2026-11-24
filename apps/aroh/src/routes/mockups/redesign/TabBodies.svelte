<script lang="ts">
	import Check from '@lucide/svelte/icons/check';
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import Lock from '@lucide/svelte/icons/lock';
	import Droplet from '@lucide/svelte/icons/droplet';
	import Dumbbell from '@lucide/svelte/icons/dumbbell';
	import Mic from '@lucide/svelte/icons/mic';
	import Plus from '@lucide/svelte/icons/plus';
	import Scale from '@lucide/svelte/icons/scale';
	import SendHorizontal from '@lucide/svelte/icons/send-horizontal';
	import Utensils from '@lucide/svelte/icons/utensils';
	import { Button } from '$lib/ui/primitives/button';
	import { Input } from '$lib/ui/primitives/input';
	import StatusChip from '$lib/ui/components/StatusChip.svelte';

	/**
	 * Restyled bodies of the Today, Calendar, Goals and Log tabs in the pod 30 language: a 24 px page title
	 * with a muted one-line summary, bordered white cards on the grey page, category as a dot or
	 * a tinted left edge, status as a chip with its label. `tab` picks the body.
	 */
	let { tab }: { tab: 'today' | 'calendar' | 'goals' | 'log' } = $props();

	const TODAY = [
		{ time: '07:00', end: '07:40', title: 'Easy run, 6 km at zone 2', goal: 'Lisbon half marathon', slot: 2, done: true },
		{ time: '12:30', end: '12:45', title: 'Duolingo, unit 14', goal: 'Learn Spanish basics', slot: 1, done: true },
		{ time: '18:00', end: '18:20', title: 'Read chapter 10', goal: 'Finish reading Harbor Lights', slot: 1, done: false, next: true },
		{ time: '20:00', end: '20:30', title: 'Piano: left hand scales', goal: 'Learn piano', slot: 3, done: false },
		{ time: '21:30', end: '21:40', title: 'Evening meditation', goal: 'Meditate daily', slot: 4, done: false }
	];
	const DAYS = [['M', 23], ['T', 24], ['W', 25], ['T', 26], ['F', 27], ['S', 28], ['S', 29]] as const;
	const START = 6;
	const END = 22;
	type Block = { day: number; from: number; to: number; slot?: number; busy?: boolean; fixed?: boolean };
	const WEEK: Block[] = [
		...[0, 1, 2, 3, 4].map((day) => ({ day, from: 9, to: 17, busy: true })),
		...[0, 3].map((day) => ({ day, from: 7, to: 7.67, slot: 2 })),
		...[1, 4].map((day) => ({ day, from: 18.5, to: 19, slot: 2 })),
		{ day: 6, from: 8, to: 9.4, slot: 2 },
		...[3, 4, 5, 6].map((day) => ({ day, from: 18, to: 18.33, slot: 1 })),
		...[0, 1, 2, 3, 4, 5, 6].map((day) => ({ day, from: 21.5, to: 21.67, slot: 4 })),
		...[0, 2, 3].map((day) => ({ day, from: 20, to: 20.5, slot: 3 })),
		{ day: 3, from: 12.5, to: 12.75, slot: 1 },
		{ day: 5, from: 10, to: 13, busy: true, fixed: true }
	];
	const top = (h: number) => ((h - START) / (END - START)) * 100;
	const GOALS = [
		{ title: 'Run the Lisbon half marathon in under two hours', cat: 'Health and fitness', slot: 2, tone: 'warn', chip: 'Waiting for you', done: 31, total: 96, line: 'Next: Easy run today 07:00' },
		{ title: 'Finish reading Harbor Lights', cat: 'Learning', slot: 1, tone: 'done', chip: 'On track', done: 9, total: 24, line: 'Next: Read chapter 10 today 18:00' },
		{ title: 'Learn Spanish basics', cat: 'Learning', slot: 1, tone: 'missed', chip: 'Behind', done: 18, total: 24, line: '2 sessions missed this week' },
		{ title: 'Learn piano', cat: 'Hobby and skill', slot: 3, tone: 'planned', chip: 'Planning', done: 0, total: 0, line: 'Agents are researching, 1 min in' },
		{ title: 'Meditate daily', cat: 'Habit', slot: 4, tone: 'done', chip: 'On track', done: 26, total: 35, line: '6 day streak' },
		{ title: 'Save 3,000 for Japan', cat: 'Financial', slot: 5, tone: 'done', chip: 'On track', done: 1250, total: 3000, line: '+250 this month' }
	] as const;
	const CHIPS = [
		{ icon: Droplet, label: 'Water' },
		{ icon: Utensils, label: 'Meal' },
		{ icon: Scale, label: 'Weight' },
		{ icon: Dumbbell, label: 'Workout' }
	];
	const LOG = [
		{ time: '13:10', text: 'Chicken salad and an apple', kind: 'Meal', detail: '540 kcal' },
		{ time: '12:30', text: 'Water', kind: 'Water', detail: '500 ml' },
		{ time: '08:05', text: 'Weight', kind: 'Weight', detail: '78.4 kg' },
		{ time: '07:45', text: 'Easy run, felt good, knee fine', kind: 'Workout', detail: '6.1 km · 39 min' },
		{ time: '07:00', text: 'Water', kind: 'Water', detail: '250 ml' }
	];
</script>

{#snippet title(t: string, sub: string)}
	<header class="flex flex-col gap-1 pb-1">
		<h2 class="text-2xl leading-tight font-semibold">{t}</h2>
		<p class="text-sm text-muted">{sub}</p>
	</header>
{/snippet}

<div class="flex flex-col gap-4 p-4">
	{#if tab === 'today'}
		{@render title('Thursday 26 November', '2 of 5 done. Next: Read chapter 10 at 18:00, in 9 h 45 min.')}
		<section class="flex flex-col gap-3 rounded-sm border border-border bg-bg p-4" aria-label="Next up">
			<div class="flex items-center justify-between"><p class="text-xs font-medium text-muted">Next up · 18:00</p><StatusChip tone="planned" label="In 9 h 45 min" /></div>
			<div class="flex flex-col gap-0.5 border-l-2 pl-3" style="border-color: var(--cat-1)"><p class="text-lg font-semibold">Read chapter 10</p><p class="text-sm text-muted">Finish reading Harbor Lights · 20 min</p></div>
			<div class="flex gap-2"><Button size="touch" class="flex-1">Mark done</Button><Button variant="outline" size="touch" class="flex-1">Move</Button></div>
		</section>
		<section class="flex flex-col gap-2" aria-label="Today's actions">
			<div class="flex items-baseline justify-between px-1"><h3 class="text-sm font-semibold">Today</h3><p class="text-sm text-muted tabular-nums">2 of 5 done</p></div>
			<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
				{#each TODAY as a (a.title)}
					<li class="flex min-h-15 items-center gap-3 px-4 py-2.5 {a.next ? 'bg-surface' : ''}">
						<span class="w-11 shrink-0 text-sm tabular-nums {a.done ? 'text-muted' : ''}">{a.time}<span class="block text-xs text-muted">{a.end}</span></span>
						<span class="flex min-w-0 flex-1 flex-col border-l-2 pl-3" style="border-color: var(--cat-{a.slot})">
							<span class="truncate text-md font-medium {a.done ? 'text-muted line-through' : ''}">{a.title}</span>
							<span class="truncate text-xs text-muted">{a.goal}</span>
						</span>
						<span class="flex size-6 shrink-0 items-center justify-center rounded-sm border {a.done ? 'border-fg bg-fg text-bg' : 'border-border-strong'}" role="checkbox" aria-checked={a.done} aria-label="Done">{#if a.done}<Check class="size-4" />{/if}</span>
					</li>
				{/each}
			</ul>
		</section>
		<div class="flex items-center gap-2"><Input class="h-11 flex-1 text-lg" placeholder="Log a meal, weight or note" /><Button variant="ghost" size="icon-touch" aria-label="Speak"><Mic /></Button><Button size="icon-touch" aria-label="Send"><SendHorizontal /></Button></div>
	{:else if tab === 'calendar'}
		{@render title('November 2026', '23 Nov to 29 Nov · 20 sessions across 5 goals')}
		<section class="flex flex-col gap-3 rounded-sm border border-border bg-bg p-4" aria-label="Week">
			<div class="flex items-center justify-between">
				<Button variant="ghost" size="icon-touch" aria-label="Previous week"><ChevronLeft /></Button>
				<div class="flex gap-1 rounded-sm border border-border p-0.5">{#each ['Day', 'Week', 'Month'] as v (v)}<span class="rounded-sm px-3 py-1 text-sm {v === 'Week' ? 'bg-fg text-bg' : 'text-muted'}">{v}</span>{/each}</div>
				<Button variant="ghost" size="icon-touch" aria-label="Next week"><ChevronRight /></Button>
			</div>
			<div class="grid grid-cols-[24px_repeat(7,1fr)] gap-x-1">
				<span></span>
				{#each DAYS as [d, n], i (i)}
					<span class="flex flex-col items-center gap-0.5 text-sm {i < 3 ? 'text-muted' : ''}"><span class="text-xs text-muted">{d}</span><span class="flex size-7 items-center justify-center rounded-sm font-medium tabular-nums {i === 3 ? 'bg-fg text-bg' : ''}">{n}</span></span>
				{/each}
			</div>
			<div class="grid h-[300px] grid-cols-[24px_repeat(7,1fr)] gap-x-1">
				<div class="relative">{#each [6, 12, 18] as h (h)}<span class="absolute text-xs text-muted tabular-nums" style="top: {top(h)}%">{h}</span>{/each}</div>
				{#each DAYS as _, d (d)}
					<div class="relative border-l border-border">
						{#each WEEK.filter((b) => b.day === d) as b, j (j)}
							<span
								class="absolute inset-x-0.5 flex items-start justify-center overflow-hidden rounded-sm border-l-3 {b.busy ? 'border border-l-3 border-border border-l-border-strong bg-surface-2' : ''}"
								style="top: {top(b.from)}%; height: max(6px, {top(b.to) - top(b.from)}%); {b.slot ? `border-left-color: var(--cat-${b.slot}); background: color-mix(in oklab, var(--cat-${b.slot}) 55%, var(--bg))` : ''}"
							>{#if b.fixed}<Lock class="mt-0.5 size-3 text-muted" aria-label="Fixed" />{/if}</span>
						{/each}
					</div>
				{/each}
			</div>
			<p class="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted"><span class="flex items-center gap-1.5"><span class="h-2.5 w-1.5 rounded-sm" style="background: var(--cat-1)"></span>Goal sessions</span><span class="flex items-center gap-1.5"><span class="h-2.5 w-1.5 rounded-sm border border-border-strong bg-surface-2"></span>Busy</span><span class="flex items-center gap-1.5"><Lock class="size-3" />Fixed</span></p>
		</section>
		<section class="flex flex-col gap-2" aria-label="Thursday 26">
			<div class="flex items-baseline justify-between px-1"><h3 class="text-sm font-semibold">Thursday 26</h3><p class="text-sm text-muted">Work 09:00 to 17:00</p></div>
			<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
				{#each TODAY as a (a.title)}
					<li class="flex min-h-13 items-center gap-3 px-4 py-2"><span class="w-11 shrink-0 text-sm tabular-nums">{a.time}</span><span class="flex min-w-0 flex-1 flex-col border-l-2 pl-3" style="border-color: var(--cat-{a.slot})"><span class="truncate text-md">{a.title}</span><span class="truncate text-xs text-muted">{a.goal}</span></span></li>
				{/each}
			</ul>
		</section>
	{:else if tab === 'goals'}
		<div class="flex items-start justify-between gap-3">
			{@render title('Goals', '6 goals · 4 on track, 1 behind, 1 waiting for you')}
			<Button size="touch" class="shrink-0 gap-1.5"><Plus />New goal</Button>
		</div>
		<ul class="flex gap-2 overflow-x-auto pb-1">
			{#each ['All', 'Waiting', 'Behind', 'Learning', 'Fitness'] as f, i (f)}<li><button type="button" class="min-h-11 rounded-sm border px-3 text-sm {i === 0 ? 'border-fg bg-fg text-bg' : 'border-border bg-bg'}">{f}</button></li>{/each}
		</ul>
		<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
			{#each GOALS as g (g.title)}
				<li class="flex flex-col gap-2 px-4 py-3">
					<div class="flex items-start gap-2">
						<span class="mt-1.5 size-2 shrink-0 rounded-full" style="background: var(--cat-{g.slot})" aria-hidden="true"></span>
						<span class="flex min-w-0 flex-1 flex-col"><span class="text-md font-medium">{g.title}</span><span class="text-xs text-muted">{g.cat}</span></span>
						<StatusChip tone={g.tone} label={g.chip} />
					</div>
					{#if g.total}<div class="ml-4 h-1.5 overflow-hidden rounded-full bg-surface-2"><div class="h-full rounded-full bg-fg" style="width: {Math.round((g.done / g.total) * 100)}%"></div></div>{/if}
					<p class="ml-4 text-sm text-muted">{g.line}</p>
				</li>
			{/each}
		</ul>
	{:else}
		{@render title('Log', 'Today: 750 ml water, 1 meal, 1 workout')}
		<ul class="grid grid-cols-4 gap-2">
			{#each CHIPS as c (c.label)}<li><button type="button" class="flex min-h-16 w-full flex-col items-center justify-center gap-1 rounded-sm border border-border bg-bg text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50"><c.icon class="size-5" aria-hidden="true" />{c.label}</button></li>{/each}
		</ul>
		<div class="flex items-center gap-2"><Input class="h-11 flex-1 text-lg" placeholder="Say or type what you did" /><Button variant="ghost" size="icon-touch" aria-label="Speak"><Mic /></Button><Button size="icon-touch" aria-label="Send"><SendHorizontal /></Button></div>
		<section class="grid grid-cols-3 divide-x divide-border rounded-sm border border-border bg-bg" aria-label="Today's totals">
			{#each [['Water', '750', 'of 2,000 ml'], ['Calories', '540', 'of 2,200'], ['Active', '39', 'min']] as [k, v, u] (k)}
				<div class="flex flex-col px-3 py-3"><span class="text-xs text-muted">{k}</span><span class="text-xl font-semibold tabular-nums">{v}</span><span class="text-xs text-muted">{u}</span></div>
			{/each}
		</section>
		<section class="flex flex-col gap-2" aria-label="Entries">
			<h3 class="px-1 text-sm font-semibold">Today</h3>
			<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
				{#each LOG as e (e.time + e.text)}
					<li class="flex min-h-13 items-center gap-3 px-4 py-2">
						<span class="w-11 shrink-0 text-sm text-muted tabular-nums">{e.time}</span>
						<span class="flex min-w-0 flex-1 flex-col"><span class="truncate text-md">{e.text}</span><span class="text-xs text-muted">{e.kind}</span></span>
						<span class="shrink-0 text-sm tabular-nums">{e.detail}</span>
					</li>
				{/each}
			</ul>
		</section>
	{/if}
</div>
