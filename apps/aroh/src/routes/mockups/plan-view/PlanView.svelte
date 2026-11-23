<script lang="ts">
	import PhoneShell from '$lib/ui/shell/PhoneShell.svelte';
	import PlacesScreen from '$lib/ui/screens/PlacesScreen.svelte';
	import PlanCard, { type PlanContext } from '$lib/ui/components/plan/PlanCard.svelte';
	import CantFitCard from '$lib/ui/components/plan/CantFitCard.svelte';
	import type { Place } from '$lib/ui/shell/nav';
	import type { Back } from '$lib/ui/shell/back';
	import type { BusyBlock, Action } from '$lib/app/client';
	import type { ChatMessage } from '$lib/ui/contract';
	import { places } from '$lib/ui/mockdata/location';
	import { shiftDay } from '$lib/ui/format';
	import Dashboard from './Dashboard.svelte';

	/**
	 * The "Plan, back, dashboard" view on the real components: the plan card (Q65) and the can't-fit
	 * card (Q64) inside the phone shell with its back bar and Inbox bell (Q66, Q69), a settings
	 * sub-page with the back bar, and the Dashboard mock for pod 34. `solo` renders one frame full screen.
	 */
	let { inboxCount, solo = null }: { inboxCount: number; solo?: number | null } = $props();

	const at = (d: string, hm: string) => `${d}T${hm}:00Z`;
	const busy: BusyBlock[] = [];
	const actions: Action[] = [];
	for (let d = '2026-11-23'; d <= '2026-12-20'; d = shiftDay(d, 1)) {
		const wd = new Date(`${d}T12:00:00Z`).getUTCDay();
		if (d === '2026-12-02') continue;
		if (wd >= 1 && wd <= 5) busy.push({ id: `w${d}`, calendar_id: 'c', event_id: `w${d}`, title: 'Work', start_at: at(d, '09:00'), end_at: at(d, '17:00'), all_day: false, group_name: 'Work calendar', day_kind: null });
		if (wd === 1 || wd === 3 || wd === 5) actions.push({ id: `g${d}`, goal_id: 'six', series_id: null, occurrence: null, title: 'Strength session', notes: null, start_at: at(d, '07:00'), end_at: at(d, '08:00'), status: 'planned', locked: true, done_at: null });
	}
	busy.push({ id: 'conf', calendar_id: 'c', event_id: 'conf', title: 'Conference', start_at: '2026-12-02T00:00:00Z', end_at: '2026-12-03T00:00:00Z', all_day: true, group_name: 'Work calendar', day_kind: 'busy' });
	busy.push({ id: 'sam', calendar_id: 'c', event_id: 'sam', title: 'Dinner with Sam', start_at: at('2026-11-26', '18:00'), end_at: at('2026-11-26', '20:00'), all_day: false, group_name: 'Home calendar', day_kind: null });
	const context: PlanContext = { busy, actions, goalId: 'read', deadline: '2026-12-31', knownUntil: '2026-12-10' };
	const items = [
		{ title: 'Read chapter 6', start: '2026-11-26T20:30', minutes: 20, rrule: null },
		{ title: 'Read a chapter', start: '2026-11-27T18:00', minutes: 20, rrule: 'FREQ=DAILY;UNTIL=20261201' },
		{ title: 'Read a chapter', start: '2026-12-03T18:00', minutes: 20, rrule: 'FREQ=DAILY;COUNT=13' }
	];
	const cant: Extract<ChatMessage, { type: 'questions' }> = {
		type: 'questions',
		persona: 'Aroh',
		node: 'limit_choice',
		card: { cap: 20, today: '2026-11-26', target: '2026-12-05', finish: '2026-12-15', needed: 380, raise_to: 45, fits_minutes: 180 },
		prompt: 'This plan does not fit your daily minutes by the date. Pick what should change.',
		items: [
			{ id: 'limit_choice', text: 'At 20 min/day you finish on Dec 15, 10 days after your target. What should change?', input: { kind: 'choice', label: 'Your answer', options: ['Move the date to Dec 15', 'Raise the daily minutes', 'Lower the target'], multi: false, value: [] } },
			{ id: 'new_minutes', text: 'If you raise them, how many minutes a day?', input: { kind: 'number_input', label: 'Your answer', unit: 'min', value: 45, step: 5, min: 21, max: 960 } }
		],
		values: { limit_choice: 'Raise the daily minutes', new_minutes: '45' },
		skipped: [],
		missing: [],
		status: '',
		submitted: false,
		sending: false,
		error: null
	};

	type Frame = { name: string; title: string; tab: Place; back?: Back; body: 'plan' | 'cant' | 'places' | 'dash'; props?: Record<string, unknown> };
	const GOALS: Back = { label: 'Goals', history: true, path: '/goals' };
	const FRAMES: Frame[] = [
		{ name: '1. Plan ready, this week', title: 'Goal', tab: 'goals', back: GOALS, body: 'plan' },
		{ name: '2. Does not fit by your date', title: 'Goal', tab: 'goals', back: GOALS, body: 'cant' },
		{ name: '3. Settings sub-page with back', title: 'Places', tab: 'settings', back: { label: 'Settings', history: true, path: '/settings' }, body: 'places' },
		{ name: '4. Dashboard (pod 34 mock)', title: 'Dashboard', tab: 'dashboard', body: 'dash' },
		{ name: '5. Dashboard, Behind by category', title: 'Dashboard', tab: 'dashboard', body: 'dash', props: { filter: 'Behind', group: 'category' } }
	];
	const list = $derived(solo === null ? FRAMES : FRAMES.slice(solo, solo + 1));
</script>

{#snippet frame(f: Frame)}
	<PhoneShell title={f.title} tab={f.tab} back={f.back ?? null} {inboxCount}>
		{#if f.body === 'plan' || f.body === 'cant'}
			<div class="flex flex-col gap-3 p-4">
				<header class="flex flex-col gap-1">
					<h2 class="text-xl font-semibold">Finish reading Harbor Lights</h2>
					<p class="flex items-center gap-2 text-sm text-muted"><span class="size-2 rounded-full" style="background: var(--cat-1)" aria-hidden="true"></span>Learning · by {f.body === 'cant' ? '5 Dec' : '31 Dec'}</p>
				</header>
				{#if f.body === 'plan'}
					<PlanCard title="One chapter an evening from chapter 6." plan={{ items, timezone: 'UTC' }} {context} slot={1} />
				{:else}
					<CantFitCard group={cant} />
				{/if}
			</div>
		{:else if f.body === 'places'}
			<PlacesScreen {places} />
		{:else}
			<Dashboard {...f.props} />
		{/if}
	</PhoneShell>
{/snippet}

{#if solo !== null}
	<div class="h-dvh w-full overflow-hidden bg-bg">{@render frame(list[0])}</div>
{:else}
	<ul class="flex gap-6 overflow-x-auto pb-4">
		{#each list as f (f.name)}
			<li class="flex shrink-0 flex-col items-center gap-2">
				<div class="box-content h-[844px] w-[390px] [transform:translateZ(0)] overflow-hidden rounded-md border border-border bg-bg">{@render frame(f)}</div>
				<p class="text-sm font-medium">{f.name}</p>
			</li>
		{/each}
	</ul>
{/if}
