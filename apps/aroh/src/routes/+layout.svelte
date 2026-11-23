<script lang="ts">
	import '../app.css';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { onMount, type Snippet } from 'svelte';
	import PhoneShell from '$lib/ui/shell/PhoneShell.svelte';
	import DesktopShell from '$lib/ui/shell/DesktopShell.svelte';
	import type { TabId } from '$lib/ui/shell/nav';
	import { data } from '$lib/app/data.svelte';
	import SetupScreen from '$lib/ui/screens/SetupScreen.svelte';
	import { healthConnect, notificationPermission, setSetting } from '$lib/app/client';
	import { syncCalendars } from '$lib/app/calendar';
	import { needsSetup, SETUP_STEPS, type SetupStep } from '$lib/app/setup';
	import { fitKeyboard } from '$lib/app/keyboard';
	import { categoryList, inboxCount } from '$lib/app/views';

	let { children }: { children: Snippet } = $props();

	const TITLES: Record<TabId | 'settings', string> = { today: 'Today', calendar: 'Calendar', goals: 'Goals', inbox: 'Inbox', log: 'Log', settings: 'Settings' };
	const SUBTITLES: Record<string, string> = { '/settings/location': 'Location', '/settings/location/log': 'Location log', '/settings/places': 'Places', '/settings/profile': 'What Aroh knows' };
	const bare = $derived((page.data as { bare?: boolean }).bare === true);
	const coaches = $derived(page.url.pathname.startsWith('/personas'));
	const tab = $derived((Object.keys(TITLES) as (TabId | 'settings')[]).find((t) => page.url.pathname.startsWith(`/${t}`)) ?? (coaches ? (page.url.searchParams.has('goal') ? 'goals' : 'settings') : 'today'));
	const title = $derived(coaches ? 'Coaches' : page.url.pathname === '/goals/new' ? 'New goal' : /^\/goals\/[^/]+\/chat$/.test(page.url.pathname) ? 'Goal chat' : (SUBTITLES[page.url.pathname] ?? TITLES[tab]));
	const count = $derived(data.snapshot ? inboxCount(data.snapshot) : 0);
	const categories = $derived(data.snapshot ? categoryList(data.snapshot) : []);
	const setup = $derived(data.snapshot ? needsSetup(data.snapshot.settings) : false);
	let wide = $state(false);

	onMount(() => {
		if (bare) return;
		const query = matchMedia('(min-width: 768px)');
		const onWidth = () => (wide = query.matches);
		onWidth();
		query.addEventListener('change', onWidth);
		const stop = data.start();
		const unfit = fitKeyboard(window);
		return () => {
			query.removeEventListener('change', onWidth);
			stop();
			unfit();
		};
	});

	/** Asks the OS for one first-launch permission; Calendar access also imports the calendars. */
	async function allow(id: SetupStep['id']) {
		if (id === 'notifications') await notificationPermission();
		else if (id === 'calendar') await data.run(async () => (await setSetting('calendars', 'true'), await syncCalendars()));
		else await healthConnect();
	}

	/** Saves that setup ran, so it never shows again, and opens Settings to sign in. */
	async function finishSetup() {
		await data.run(() => setSetting('setup_done', 'true'));
		await goto('/settings');
	}

	/** Opens a tab or Settings. */
	function navigate(to: TabId | 'settings') {
		void goto(`/${to}`);
	}
</script>

{#snippet body()}
	{#if data.error}<p role="alert" class="px-4 pt-3 text-sm text-status-missed">{data.error}</p>{/if}
	{#key data.day + data.zone}{@render children()}{/key}
{/snippet}

{#if bare}
	{@render children()}
{:else}
	<div class="fixed inset-x-0 top-0" style="height: var(--app-h, 100%)">
		{#if setup}
			<SetupScreen steps={SETUP_STEPS} onallow={allow} ondone={finishSetup} />
		{:else if wide}
			<DesktopShell {title} {tab} inboxCount={count} {categories} onnavigate={navigate}>{@render body()}</DesktopShell>
		{:else}
			<PhoneShell {title} {tab} inboxCount={count} onnavigate={navigate}>{@render body()}</PhoneShell>
		{/if}
	</div>
{/if}
