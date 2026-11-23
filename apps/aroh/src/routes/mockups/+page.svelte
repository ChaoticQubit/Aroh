<script lang="ts">
	import Moon from '@lucide/svelte/icons/moon';
	import Sun from '@lucide/svelte/icons/sun';
	import type { Component } from 'svelte';
	import { onMount } from 'svelte';
	import { Button } from '$lib/ui/primitives/button';
	import Segmented from '$lib/ui/components/Segmented.svelte';
	import Render from '$lib/ui/components/Render.svelte';
	import PersonaChat from '$lib/ui/components/agent/PersonaChat.svelte';
	import PhoneShell from '$lib/ui/shell/PhoneShell.svelte';
	import DesktopShell from '$lib/ui/shell/DesktopShell.svelte';
	import type { Place, TabId } from '$lib/ui/shell/nav';
	import TodayScreen from '$lib/ui/screens/TodayScreen.svelte';
	import CalendarScreen from '$lib/ui/screens/CalendarScreen.svelte';
	import GoalScreen from '$lib/ui/screens/GoalScreen.svelte';
	import NewGoalScreen from '$lib/ui/screens/NewGoalScreen.svelte';
	import InboxScreen from '$lib/ui/screens/InboxScreen.svelte';
	import SettingsScreen from '$lib/ui/screens/SettingsScreen.svelte';
	import GoalsScreen from '$lib/ui/screens/GoalsScreen.svelte';
	import LogScreen from '$lib/ui/screens/LogScreen.svelte';
	import { setClock } from '$lib/ui/clock';
	import ClockFrame from './ClockFrame.svelte';
	import GoalRedesign from './goal-redesign/GoalRedesign.svelte';
	import PlanView from './plan-view/PlanView.svelte';
	import LocationExplainer from '$lib/ui/components/location/LocationExplainer.svelte';
	import LockScreenNudge from '$lib/ui/components/location/LockScreenNudge.svelte';
	import PlacesScreen from '$lib/ui/screens/PlacesScreen.svelte';
	import RoutinesScreen from '$lib/ui/screens/RoutinesScreen.svelte';
	import LocationSettingsScreen from '$lib/ui/screens/LocationSettingsScreen.svelte';
	import LocationLogScreen from '$lib/ui/screens/LocationLogScreen.svelte';
	import { groceryPattern, gymPrompt, locationSettings, placeResults, places, routines, routinesHeader, routinesView, visits } from '$lib/ui/mockdata/location';
	import { gallery } from '$lib/ui/mockdata/gallery';
	import {
		plannedActions,
		sixpackAllDay,
		sixpackAllDayNew,
		categories,
		mockNow,
		mockZone,
		sixpackActions,
		sixpackChat,
		sixpackGoals,
		sixpackCoaches,
		sixpackHeader,
		sixpackInbox,
		sixpackInboxItems,
		sixpackLog,
		sixpackSettings,
		sixpackView,
		weightCheckIn
	} from '$lib/ui/mockdata/sixpack';

	setClock({ now: mockNow, zone: mockZone });

	type View = 'tokens' | 'inputs' | 'display' | 'planning' | 'agent' | 'screens-phone' | 'screens-desktop' | 'editing' | 'goal-redesign' | 'location' | 'plan-view';
	const VIEWS: { value: View; label: string }[] = [
		{ value: 'tokens', label: 'Tokens' },
		{ value: 'inputs', label: 'Inputs' },
		{ value: 'display', label: 'Display' },
		{ value: 'planning', label: 'Planning' },
		{ value: 'agent', label: 'Agent' },
		{ value: 'screens-phone', label: 'Screens (phone)' },
		{ value: 'screens-desktop', label: 'Screens (desktop)' },
		{ value: 'editing', label: 'Calendar editing' },
		{ value: 'goal-redesign', label: 'Goals redesign' },
		{ value: 'location', label: 'Location' },
		{ value: 'plan-view', label: 'Plan, back, dashboard' }
	];
	const keep = () => {};
	const EDIT = { onedit: keep, onlock: keep, ondelete: keep, onchoose: keep, inline: true };
	const PIANO = 'a-2026-11-26-2000-4';
	const CORE = 'a-2026-12-03-0700-2';
	const EDITING: { name: string; title: string; tab: TabId; now: string; screen: Component<any>; props: Record<string, unknown> }[] = [
		{ name: 'Today, new all-day event', title: 'Today', tab: 'today', now: mockNow, screen: TodayScreen, props: { actions: sixpackActions, allDay: sixpackAllDayNew, ...EDIT } },
		{ name: 'Today, Away with a Reminder', title: 'Today', tab: 'today', now: '2026-11-28T08:15:00Z', screen: TodayScreen, props: { actions: plannedActions, allDay: sixpackAllDay, ...EDIT } },
		{ name: 'Calendar day, which kind?', title: 'Calendar', tab: 'calendar', now: mockNow, screen: CalendarScreen, props: { actions: sixpackActions, categories, allDay: sixpackAllDayNew, initialView: 'day', day: '2026-11-27', ...EDIT } },
		{ name: 'Calendar day, all-day row', title: 'Calendar', tab: 'calendar', now: mockNow, screen: CalendarScreen, props: { actions: plannedActions, categories, allDay: sixpackAllDay, initialView: 'day', day: '2026-11-28', ...EDIT } },
		{ name: 'Calendar day, a Busy day', title: 'Calendar', tab: 'calendar', now: mockNow, screen: CalendarScreen, props: { actions: plannedActions, categories, allDay: sixpackAllDay, initialView: 'day', day: '2026-12-02', ...EDIT } },
		{ name: 'Calendar month, all-day bars', title: 'Calendar', tab: 'calendar', now: mockNow, screen: CalendarScreen, props: { actions: plannedActions, categories, allDay: sixpackAllDay, initialView: 'month', ...EDIT } },
		{ name: 'Edit sheet from Today', title: 'Today', tab: 'today', now: mockNow, screen: TodayScreen, props: { actions: plannedActions, allDay: sixpackAllDay, editing: PIANO, ...EDIT } },
		{ name: 'Delete confirm', title: 'Today', tab: 'today', now: mockNow, screen: TodayScreen, props: { actions: plannedActions, allDay: sixpackAllDay, editing: PIANO, confirming: true, ...EDIT } },
		{ name: 'Edit sheet from Calendar', title: 'Calendar', tab: 'calendar', now: mockNow, screen: CalendarScreen, props: { actions: plannedActions, categories, allDay: sixpackAllDay, initialView: 'day', day: '2026-12-03', editing: CORE, ...EDIT } },
		{ name: 'Drag to move, edge to resize', title: 'Calendar', tab: 'calendar', now: mockNow, screen: CalendarScreen, props: { actions: plannedActions.map((a) => (a.id === CORE ? { ...a, end: '2026-12-03T08:00:00Z' } : a)), categories, allDay: sixpackAllDay, initialView: 'day', day: '2026-12-03', selected: CORE, ...EDIT } }
	];
	const SCREENS: { name: string; title: string; tab: Place; screen: Component<any>; props: Record<string, unknown> }[] = [
		{ name: 'Today', title: 'Today', tab: 'today', screen: TodayScreen, props: { actions: plannedActions, checkIn: weightCheckIn, insight: sixpackInbox.insights[0], allDay: sixpackAllDay } },
		{ name: 'Calendar', title: 'Calendar', tab: 'calendar', screen: CalendarScreen, props: { actions: plannedActions, categories, allDay: sixpackAllDay } },
		{ name: 'Goals', title: 'Goals', tab: 'goals', screen: GoalsScreen, props: { goals: sixpackGoals, categories } },
		{ name: 'Goal', title: 'Goals', tab: 'goals', screen: GoalScreen, props: { goal: sixpackHeader, coaches: sixpackCoaches, view: sixpackView, versions: [{ value: 'derived', label: 'Derived' }, { value: 'p2', label: 'v2' }, { value: 'p1', label: 'v1' }], version: 'p2' } },
		{ name: 'New goal', title: 'New goal', tab: 'goals', screen: NewGoalScreen, props: { messages: sixpackChat } },
		{ name: 'Inbox', title: 'Inbox', tab: 'inbox', screen: InboxScreen, props: { ...sixpackInboxItems, runs: sixpackInbox.runs } },
		{ name: 'Log', title: 'Log', tab: 'log', screen: LogScreen, props: sixpackLog },
		{ name: 'Settings', title: 'Settings', tab: 'settings', screen: SettingsScreen, props: { settings: sixpackSettings } }
	];
	const NUDGE = { time: '18:04', date: 'Thursday 26 November', title: 'Strength session, due 18:00', body: 'Near your gym. Quick workout to stay on track?', detail: 'Six-pack by summer', actions: ['Mark done', 'Remind me in 1 hour', 'Skip today'] };
	const LOCATION: { name: string; title: string | null; tab: Place; now: string; screen: Component<any>; props: Record<string, unknown> }[] = [
		{ name: 'Before asking for Always', title: null, tab: 'today', now: mockNow, screen: LocationExplainer, props: {} },
		{ name: 'Inbox, is this your gym?', title: 'Inbox', tab: 'inbox', now: mockNow, screen: InboxScreen, props: { places: [gymPrompt], approvals: [], suggestions: [], insights: [], runs: [] } },
		{ name: 'Lock screen nudge', title: null, tab: 'today', now: '2026-11-26T18:04:00Z', screen: LockScreenNudge, props: NUDGE },
		{ name: 'Nudge, long press', title: null, tab: 'today', now: '2026-11-26T18:04:00Z', screen: LockScreenNudge, props: { ...NUDGE, expanded: true } },
		{ name: 'Inbox, learned routine', title: 'Inbox', tab: 'inbox', now: mockNow, screen: InboxScreen, props: { approvals: [], suggestions: [groceryPattern, ...sixpackInboxItems.suggestions], insights: [], runs: [] } },
		{ name: 'Places', title: 'Places', tab: 'settings', now: mockNow, screen: PlacesScreen, props: { places } },
		{ name: 'Places, add by map search', title: 'Places', tab: 'settings', now: mockNow, screen: PlacesScreen, props: { places, query: 'trader', results: placeResults } },
		{ name: 'Routines goal', title: 'Goals', tab: 'goals', now: mockNow, screen: RoutinesScreen, props: { goal: routinesHeader, view: routinesView, routines } },
		{ name: 'Settings, Location row', title: 'Settings', tab: 'settings', now: mockNow, screen: SettingsScreen, props: { settings: sixpackSettings, onlocation: keep } },
		{ name: 'Settings, Location', title: 'Location', tab: 'settings', now: mockNow, screen: LocationSettingsScreen, props: { settings: locationSettings } },
		{ name: 'Location log', title: 'Location log', tab: 'settings', now: '2026-11-26T18:05:00Z', screen: LocationLogScreen, props: { visits } }
	];
	const COLORS = ['bg', 'surface', 'surface-2', 'border', 'border-strong', 'fg', 'muted', 'accent', 'status-done', 'status-planned', 'status-warn', 'status-missed', 'status-skipped', 'series-1', 'series-2', 'series-3', 'series-4', 'series-5', 'series-6', 'cat-1', 'cat-2', 'cat-3', 'cat-4', 'cat-5', 'cat-6'];
	const TYPE = ['text-xs', 'text-sm', 'text-md', 'text-lg', 'text-xl'];
	const SPACE = ['space-1', 'space-2', 'space-3', 'space-4', 'space-5', 'space-6'];
	const RADIUS = ['radius-sm', 'radius-dialog'];
	const inboxCount = sixpackInbox.approvals.length + sixpackInbox.suggestions.length + sixpackInbox.insights.length;

	let view = $state<View>('tokens');
	let dark = $state(false);
	let solo = $state<number | null>(null);
	let values = $state<Record<string, string>>({});

	/** Reads `#view`, or `#view/frame/theme` for one full-screen frame used by simulator screenshots. */
	function readHash() {
		const [name, frame, theme] = location.hash.slice(1).split('/');
		if (VIEWS.some((v) => v.value === name)) view = name as View;
		solo = frame === undefined ? null : Number(frame);
		if (theme) dark = theme === 'dark';
	}

	/** Reads the current computed value of every listed token, for the swatch captions. */
	function readTokens() {
		const style = getComputedStyle(document.documentElement);
		values = Object.fromEntries([...COLORS, ...TYPE, ...SPACE, ...RADIUS].map((t) => [t, style.getPropertyValue(`--${t}`).trim()]));
	}

	onMount(() => {
		dark = matchMedia('(prefers-color-scheme: dark)').matches;
		readHash();
		readTokens();
		addEventListener('hashchange', readHash);
		return () => removeEventListener('hashchange', readHash);
	});

	$effect(() => {
		if (solo === null && location.hash.slice(1) !== view) history.replaceState(null, '', `#${view}`);
	});

	$effect(() => {
		document.documentElement.dataset.theme = dark ? 'dark' : 'light';
		readTokens();
	});
</script>

<svelte:head><title>Aroh Mockups</title></svelte:head>

{#if solo !== null}
	<PlanView {inboxCount} {solo} />
{:else}
<div class="min-h-dvh bg-surface text-fg">
	<header class="sticky top-0 z-30 flex flex-wrap items-center gap-3 border-b border-border bg-bg px-4 py-2">
		<p class="text-sm font-semibold">Aroh mockups</p>
		<div class="order-last -mx-4 w-[calc(100%+2rem)] overflow-x-auto px-4 md:order-none md:mx-0 md:w-auto md:px-0">
			<Segmented label="View" compact bind:value={view} options={VIEWS} />
		</div>
		<Button variant="outline" size="icon-touch" class="ml-auto md:size-9" aria-label={dark ? 'Use light theme' : 'Use dark theme'} onclick={() => (dark = !dark)}>
			{#if dark}<Sun />{:else}<Moon />{/if}
		</Button>
	</header>

	<main class="p-4 md:p-6">
		{#if view === 'tokens'}
			<div class="flex max-w-5xl flex-col gap-8">
				<section class="flex flex-col gap-3" aria-label="Colours">
					<h2 class="text-lg font-semibold">Colours</h2>
					<ul class="grid grid-cols-2 gap-4 sm:grid-cols-4 md:grid-cols-8">
						{#each COLORS as c (c)}
							<li class="flex flex-col gap-1.5">
								<span class="h-10 rounded-sm border border-border" style="background: var(--{c})"></span>
								<span class="text-sm font-medium">--{c}</span>
								<span class="text-xs text-muted">{values[c] ?? ''}</span>
							</li>
						{/each}
					</ul>
				</section>
				<section class="flex flex-col gap-3" aria-label="Type scale">
					<h2 class="text-lg font-semibold">Type scale</h2>
					<ul class="flex flex-col divide-y divide-border rounded-md border border-border bg-bg">
						{#each TYPE as t (t)}
							<li class="flex items-baseline gap-4 px-4 py-3">
								<span class="w-36 shrink-0 text-xs text-muted">--{t} {values[t] ?? ''}</span>
								<span style="font-size: var(--{t})">Strength session at 18:00</span>
							</li>
						{/each}
					</ul>
				</section>
				<div class="grid gap-8 md:grid-cols-2">
					<section class="flex flex-col gap-3" aria-label="Spacing">
						<h2 class="text-lg font-semibold">Spacing</h2>
						<ul class="flex flex-col gap-2 rounded-md border border-border bg-bg p-4">
							{#each SPACE as s (s)}
								<li class="flex items-center gap-4"><span class="w-28 text-xs text-muted">--{s} {values[s] ?? ''}</span><span class="h-2 bg-fg" style="width: var(--{s})"></span></li>
							{/each}
						</ul>
					</section>
					<section class="flex flex-col gap-3" aria-label="Radius">
						<h2 class="text-lg font-semibold">Radius</h2>
						<ul class="flex gap-4 rounded-md border border-border bg-bg p-4">
							{#each RADIUS as r (r)}
								<li class="flex flex-col items-center gap-2"><span class="size-16 border border-border bg-surface-2" style="border-radius: var(--{r})"></span><span class="text-xs text-muted">--{r}</span></li>
							{/each}
						</ul>
					</section>
				</div>
			</div>
		{:else if view === 'inputs' || view === 'display' || view === 'planning' || view === 'agent'}
			<ul class="grid max-w-6xl grid-cols-[repeat(auto-fill,minmax(min(100%,340px),1fr))] items-start gap-4">
				{#each gallery[view] as item, i (i)}
					<li class="flex flex-col gap-2">
						<p class="flex items-center gap-2 text-xs text-muted"><span class="font-medium text-fg">{item.spec.kind}</span>{item.state}</p>
						<div class={view === 'inputs' ? 'rounded-md bg-bg p-4 ring-1 ring-border' : ''}><Render spec={item.spec} /></div>
					</li>
				{/each}
				{#if view === 'agent'}
					<li class="flex flex-col gap-2">
						<p class="flex items-center gap-2 text-xs text-muted"><span class="font-medium text-fg">PersonaChat</span>filled</p>
						<div class="h-[640px] overflow-hidden rounded-md bg-bg ring-1 ring-border"><PersonaChat messages={sixpackChat.slice(0, 4)} /></div>
					</li>
				{/if}
			</ul>
		{:else if view === 'screens-phone'}
			<ul class="flex gap-6 overflow-x-auto pb-4">
				{#each SCREENS as s (s.name)}
					<li class="flex shrink-0 flex-col items-center gap-2">
						<div class="box-content h-[844px] w-[390px] [transform:translateZ(0)] overflow-hidden rounded-md border border-border bg-bg">
							<PhoneShell title={s.title} tab={s.tab} {inboxCount}><s.screen {...s.props} /></PhoneShell>
						</div>
						<p class="text-sm font-medium">{s.name}</p>
					</li>
				{/each}
			</ul>
		{:else if view === 'editing'}
			<ul class="flex gap-6 overflow-x-auto pb-4">
				{#each EDITING as s (s.name)}
					<li class="flex shrink-0 flex-col items-center gap-2">
						<div class="box-content h-[844px] w-[390px] [transform:translateZ(0)] overflow-hidden rounded-md border border-border bg-bg">
							<ClockFrame now={s.now} zone={mockZone}>
								<PhoneShell title={s.title} tab={s.tab} {inboxCount}><s.screen {...s.props} /></PhoneShell>
							</ClockFrame>
						</div>
						<p class="text-sm font-medium">{s.name}</p>
					</li>
				{/each}
			</ul>
		{:else if view === 'goal-redesign'}
			<GoalRedesign {inboxCount} />
		{:else if view === 'plan-view'}
			<PlanView {inboxCount} />
		{:else if view === 'location'}
			<ul class="flex gap-6 overflow-x-auto pb-4">
				{#each LOCATION as s (s.name)}
					<li class="flex shrink-0 flex-col items-center gap-2">
						<div class="box-content h-[844px] w-[390px] [transform:translateZ(0)] overflow-hidden rounded-md border border-border bg-bg">
							<ClockFrame now={s.now} zone={mockZone}>
								{#if s.title}
									<PhoneShell title={s.title} tab={s.tab} {inboxCount}><s.screen {...s.props} /></PhoneShell>
								{:else}
									<s.screen {...s.props} />
								{/if}
							</ClockFrame>
						</div>
						<p class="text-sm font-medium">{s.name}</p>
					</li>
				{/each}
			</ul>
		{:else}
			<ul class="flex flex-col gap-8">
				{#each SCREENS as s (s.name)}
					<li class="flex flex-col gap-2">
						<p class="text-sm font-medium">{s.name}</p>
						<div class="overflow-x-auto pb-2">
							<div class="box-content h-[800px] w-[1280px] [transform:translateZ(0)] overflow-hidden rounded-md border border-border bg-bg">
								<DesktopShell title={s.title} tab={s.tab} {inboxCount} {categories}><s.screen {...s.props} /></DesktopShell>
							</div>
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</main>
</div>
{/if}
