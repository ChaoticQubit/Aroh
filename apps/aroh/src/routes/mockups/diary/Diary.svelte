<script lang="ts">
	import Lock from '@lucide/svelte/icons/lock';
	import Mic from '@lucide/svelte/icons/mic';
	import Send from '@lucide/svelte/icons/send';
	import Smartphone from '@lucide/svelte/icons/smartphone';
	import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
	import { Button } from '$lib/ui/primitives/button';
	import PhoneShell from '$lib/ui/shell/PhoneShell.svelte';
	import DesktopShell from '$lib/ui/shell/DesktopShell.svelte';
	import type { Place } from '$lib/ui/shell/nav';
	import type { Back } from '$lib/ui/shell/back';
	import SettingsList, { type SettingsSection } from '$lib/ui/components/SettingsList.svelte';
	import CalendarScreen from '$lib/ui/screens/CalendarScreen.svelte';
	import { categories, mockZone } from '$lib/ui/mockdata/sixpack';
	import ClockFrame from '../ClockFrame.svelte';
	import DiaryCard from '$lib/ui/diary/DiaryCard.svelte';
	import { NO_KEY } from '$lib/app/diary';
	import { DIARY_RISK, diarySettings } from '$lib/ui/diary/settings';
	import { diaryNow, today, todayUnstructured, tuesday, tuesdayActions, wednesday } from './data';

	/**
	 * The "Diary" mock view (Q85, pod 52): the Log tab with the composer and today's card, the calendar
	 * day with the card above its sessions, the full card, the unstructured state, the Settings toggle
	 * off and on with its risk text, and the Mac "available on iPhone" state. `solo` renders one frame
	 * full screen for screenshots.
	 */
	let { inboxCount, solo = null }: { inboxCount: number; solo?: number | null } = $props();

	type Body = 'log' | 'calendar' | 'full' | 'unstructured' | 'settings-off' | 'settings-on' | 'mac';
	type Frame = { name: string; title: string; tab: Place; back?: Back; body: Body };
	const FRAMES: Frame[] = [
		{ name: '1. Log, composer and today', title: 'Log', tab: 'log', body: 'log' },
		{ name: '2. Calendar day, card above sessions', title: 'Calendar', tab: 'calendar', body: 'calendar' },
		{ name: '3. Full card', title: 'Diary', tab: 'log', back: { label: 'Log', history: true, path: '/log' }, body: 'full' },
		{ name: '4. No Claude key on the engine', title: 'Log', tab: 'log', body: 'unstructured' },
		{ name: '5. Settings, agents cannot read', title: 'Diary', tab: 'settings', back: { label: 'Settings', history: true, path: '/settings' }, body: 'settings-off' },
		{ name: '6. Settings, agents can read', title: 'Diary', tab: 'settings', back: { label: 'Settings', history: true, path: '/settings' }, body: 'settings-on' },
		{ name: '7. Mac, diary is on iPhone', title: 'Log', tab: 'log', body: 'mac' }
	];
	const list = $derived(solo === null ? FRAMES : FRAMES.slice(solo, solo + 1));
	const keep = () => {};

	const RISK = DIARY_RISK;

	/** The Diary settings page with the agents toggle in state `on`. */
	function settings(on: boolean): SettingsSection[] {
		return diarySettings(on, keep);
	}
</script>

{#snippet composer()}
	<form class="flex flex-col gap-2 rounded-sm border border-border bg-bg p-3" onsubmit={(e) => e.preventDefault()}>
		<label for="diary-draft" class="text-sm font-semibold">How is your day going?</label>
		<textarea id="diary-draft" rows="2" placeholder="Write or speak. Meals, water and workouts become tracker numbers." class="min-h-16 w-full resize-none rounded-sm border border-input bg-transparent px-3 py-2 text-lg outline-none placeholder:text-muted focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"></textarea>
		<div class="flex items-center gap-1">
			<p class="flex flex-1 items-center gap-1 text-xs text-muted"><Lock class="size-3" aria-hidden="true" />Encrypted on this phone before it syncs</p>
			<Button variant="ghost" size="icon-touch" type="button" aria-label="Speak a diary entry"><Mic /></Button>
			<Button size="icon-touch" type="submit" aria-label="Add to diary" disabled><Send /></Button>
		</div>
	</form>
{/snippet}

{#snippet logTab(day: typeof today, past: (typeof today)[])}
	<div class="flex flex-col gap-4 p-4">
		<header class="flex flex-col gap-1">
			<h2 class="text-2xl leading-tight font-semibold">Log</h2>
			<p class="text-sm text-muted">Today: {day.entries.length} diary entries.</p>
		</header>
		{@render composer()}
		<DiaryCard {day} zone={mockZone} title="Today" note={NO_KEY} />
		{#if past.length}
			<section class="flex flex-col gap-2" aria-label="Past days">
				<h3 class="px-1 text-sm font-semibold">Past days</h3>
				{#each past as p (p.day)}<DiaryCard day={p} mode="compact" zone={mockZone} />{/each}
			</section>
		{/if}
	</div>
{/snippet}

{#snippet body(f: Frame)}
	{#if f.body === 'log'}
		{@render logTab(today, [wednesday, tuesday])}
	{:else if f.body === 'unstructured'}
		{@render logTab(todayUnstructured, [wednesday])}
	{:else if f.body === 'calendar'}
		<CalendarScreen actions={tuesdayActions} {categories} initialView="day" day="2026-11-24">
			{#snippet dayTop()}<DiaryCard day={tuesday} mode="compact" zone={mockZone} />{/snippet}
		</CalendarScreen>
	{:else if f.body === 'full'}
		<div class="p-4"><DiaryCard day={today} zone={mockZone} title="Thursday 26 November" note={NO_KEY} /></div>
	{:else if f.body === 'settings-off' || f.body === 'settings-on'}
		<div class="flex flex-col gap-6 p-4">
			<SettingsList sections={settings(f.body === 'settings-on')} />
			<section class="flex flex-col gap-2" aria-label="Before you turn it on">
				<h2 class="flex items-center gap-1.5 px-1 text-sm font-semibold"><TriangleAlert class="size-4 text-status-warn" aria-hidden="true" />Good to know</h2>
				<ul class="flex flex-col gap-2 rounded-sm border border-border bg-bg p-4 text-sm">
					{#each RISK as r (r)}<li class="flex gap-2"><span aria-hidden="true" class="text-muted">·</span><span>{r}</span></li>{/each}
				</ul>
			</section>
		</div>
	{:else}
		<div class="flex flex-col gap-4 p-6">
			<h2 class="text-2xl leading-tight font-semibold">Log</h2>
			<section class="flex max-w-lg flex-col items-start gap-3 rounded-sm border border-border bg-bg p-5" aria-label="Diary">
				<Smartphone class="size-6 text-muted" aria-hidden="true" />
				<h3 class="text-lg font-semibold">Your diary is available on iPhone</h3>
				<p class="text-sm text-muted">The diary is end-to-end encrypted with a key that lives in your iPhone's iCloud Keychain. Write and read it in Aroh on your iPhone. Trackers and check-ins still work here.</p>
			</section>
		</div>
	{/if}
{/snippet}

{#snippet frame(f: Frame)}
	<ClockFrame now={diaryNow} zone={mockZone}>
		{#if f.body === 'mac'}
			<DesktopShell title={f.title} tab={f.tab} {inboxCount} {categories}>{@render body(f)}</DesktopShell>
		{:else}
			<PhoneShell title={f.title} tab={f.tab} back={f.back ?? null} {inboxCount}>{@render body(f)}</PhoneShell>
		{/if}
	</ClockFrame>
{/snippet}

{#if solo !== null}
	<div class="h-dvh w-full overflow-hidden bg-bg">{@render frame(list[0])}</div>
{:else}
	<ul class="flex gap-6 overflow-x-auto pb-4">
		{#each list as f (f.name)}
			<li class="flex shrink-0 flex-col items-center gap-2">
				<div class="box-content [transform:translateZ(0)] overflow-hidden rounded-md border border-border bg-bg {f.body === 'mac' ? 'h-[800px] w-[1280px]' : 'h-[844px] w-[390px]'}">{@render frame(f)}</div>
				<p class="text-sm font-medium">{f.name}</p>
			</li>
		{/each}
	</ul>
{/if}
