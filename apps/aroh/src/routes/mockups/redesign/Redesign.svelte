<script lang="ts">
	import Check from '@lucide/svelte/icons/check';
	import PhoneShell from '$lib/ui/shell/PhoneShell.svelte';
	import type { Place } from '$lib/ui/shell/nav';
	import type { Back } from '$lib/ui/shell/back';
	import Dashboard from '../plan-view/Dashboard.svelte';
	import Onboarding from './Onboarding.svelte';
	import SettingsPages from './SettingsPages.svelte';
	import GoalPage from './GoalPage.svelte';
	import TabBodies from './TabBodies.svelte';
	import { halfGoal, readingGoal } from './data';

	/**
	 * The Q72 redesign mockup for owner approval: onboarding, regrouped Settings, the goal page in
	 * collapsible cards (default and long data), the back bar on three screen kinds, and the five tab
	 * screens. `solo` renders one frame full screen for simulator screenshots.
	 */
	let { inboxCount, solo = null }: { inboxCount: number; solo?: number | null } = $props();

	type Body = 'onboarding' | 'settings' | 'key' | 'goal' | 'goal-long' | 'action' | 'today' | 'calendar' | 'goals' | 'dashboard' | 'log';
	type Frame = { name: string; body: Body; title?: string; tab?: Place; back?: Back; step?: 'welcome' | 'signin' | 'server' | 'key' | 'permissions' | 'goal' };
	const GOALS: Back = { label: 'Goals', history: true, path: '/goals' };
	const FRAMES: Frame[] = [
		{ name: '1a. Welcome', body: 'onboarding', step: 'welcome' },
		{ name: '1b. Sign in', body: 'onboarding', step: 'signin' },
		{ name: '1c. Use my own server', body: 'onboarding', step: 'server' },
		{ name: '1d. Claude key', body: 'onboarding', step: 'key' },
		{ name: '1e. Permissions', body: 'onboarding', step: 'permissions' },
		{ name: '1f. First goal', body: 'onboarding', step: 'goal' },
		{ name: '2a. Settings', body: 'settings', title: 'Settings', tab: 'settings' },
		{ name: '2b. Settings sub-page, back bar', body: 'key', title: 'Claude key and budget', tab: 'settings', back: { label: 'Settings', history: true, path: '/settings' } },
		{ name: '3a. Goal, default open and closed', body: 'goal', title: 'Goal', tab: 'goals', back: GOALS },
		{ name: '3b. Goal, long data, all open', body: 'goal-long', title: 'Goal', tab: 'goals', back: GOALS },
		{ name: '4. Action page, back bar', body: 'action', title: 'Action', tab: 'today', back: { label: 'Today', history: true, path: '/today' } },
		{ name: '5a. Today', body: 'today', title: 'Today', tab: 'today' },
		{ name: '5b. Calendar', body: 'calendar', title: 'Calendar', tab: 'calendar' },
		{ name: '5c. Goals', body: 'goals', title: 'Goals', tab: 'goals' },
		{ name: '5d. Dashboard', body: 'dashboard', title: 'Dashboard', tab: 'dashboard' },
		{ name: '5e. Log', body: 'log', title: 'Log', tab: 'log' }
	];
	const list = $derived(solo === null ? FRAMES : FRAMES.slice(solo, solo + 1));
	const SETS = [
		{ n: 1, reps: 8, kg: '60', done: true },
		{ n: 2, reps: 8, kg: '60', done: true },
		{ n: 3, reps: 8, kg: '62.5', done: false }
	];
</script>

{#snippet action()}
	<div class="flex flex-col gap-3 p-4">
		<header class="flex flex-col gap-1">
			<h2 class="text-2xl leading-tight font-semibold">Leg day</h2>
			<p class="flex items-center gap-2 text-sm text-muted"><span class="size-2 rounded-full" style="background: var(--cat-2)" aria-hidden="true"></span>Thu 26 Nov · 18:30 to 19:15 · Lisbon half marathon</p>
		</header>
		<p class="rounded-sm border border-border bg-bg p-4 text-sm text-muted">Placeholder body: the action page itself is pod 37's. This frame only shows the back bar returning to Today.</p>
		<div class="rounded-sm border border-border bg-bg">
			<p class="border-b border-border px-4 py-2.5 text-md font-semibold">Back squat</p>
			<ul class="flex flex-col divide-y divide-border">
				{#each SETS as s (s.n)}
					<li class="flex min-h-12 items-center gap-3 px-4 text-sm tabular-nums"><span class="w-10 text-muted">Set {s.n}</span><span class="flex-1">{s.reps} reps · {s.kg} kg</span><span class="flex size-6 items-center justify-center rounded-sm border {s.done ? 'border-fg bg-fg text-bg' : 'border-border-strong'}" aria-hidden="true">{#if s.done}<Check class="size-4" />{/if}</span></li>
				{/each}
			</ul>
		</div>
	</div>
{/snippet}

{#snippet frame(f: Frame)}
	{#if f.body === 'onboarding' && f.step}
		<Onboarding step={f.step} />
	{:else}
		<PhoneShell title={f.title ?? ''} tab={f.tab ?? 'today'} back={f.back ?? null} {inboxCount}>
			{#if f.body === 'settings'}
				<SettingsPages page="root" />
			{:else if f.body === 'key'}
				<SettingsPages page="key" />
			{:else if f.body === 'goal'}
				<GoalPage goal={readingGoal} />
			{:else if f.body === 'goal-long'}
				<GoalPage goal={halfGoal} expand />
			{:else if f.body === 'action'}
				{@render action()}
			{:else if f.body === 'dashboard'}
				<Dashboard />
			{:else if f.body === 'today' || f.body === 'calendar' || f.body === 'goals' || f.body === 'log'}
				<TabBodies tab={f.body} />
			{/if}
		</PhoneShell>
	{/if}
{/snippet}

{#if solo !== null}
	<div class="h-dvh w-full overflow-hidden bg-bg">{@render frame(list[0])}</div>
{:else}
	<ul class="flex gap-6 overflow-x-auto pb-4">
		{#each list as f (f.name)}
			<li class="flex shrink-0 flex-col items-center gap-2">
				<div class="box-content h-[852px] w-[393px] [transform:translateZ(0)] overflow-hidden rounded-md border border-border bg-bg">{@render frame(f)}</div>
				<p class="text-sm font-medium">{f.name}</p>
			</li>
		{/each}
	</ul>
{/if}
