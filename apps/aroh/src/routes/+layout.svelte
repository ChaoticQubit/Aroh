<script lang="ts">
	import '../app.css';
	import { page } from '$app/state';
	import { afterNavigate, beforeNavigate, goto } from '$app/navigation';
	import { onMount, tick, type Snippet } from 'svelte';
	import PhoneShell from '$lib/ui/shell/PhoneShell.svelte';
	import DesktopShell from '$lib/ui/shell/DesktopShell.svelte';
	import type { Place } from '$lib/ui/shell/nav';
	import { backTarget, nextStack, screenTitle } from '$lib/ui/shell/back';
	import { data } from '$lib/app/data.svelte';
	import SetupScreen from '$lib/ui/screens/SetupScreen.svelte';
	import { healthConnect, notificationPermission, setSetting } from '$lib/app/client';
	import { syncCalendars } from '$lib/app/calendar';
	import { needsSetup, setupSteps, type SetupStep } from '$lib/app/setup';
	import { fitKeyboard } from '$lib/app/keyboard';
	import { categoryList, inboxCount } from '$lib/app/views';

	let { children }: { children: Snippet } = $props();

	const PLACES: Place[] = ['today', 'calendar', 'goals', 'dashboard', 'log', 'inbox', 'settings'];
	const bare = $derived((page.data as { bare?: boolean }).bare === true);
	const coaches = $derived(page.url.pathname.startsWith('/personas'));
	const tab = $derived(PLACES.find((t) => page.url.pathname.startsWith(`/${t}`)) ?? (coaches ? (page.url.searchParams.has('goal') ? 'goals' : 'settings') : 'today'));
	const title = $derived(screenTitle(page.url.pathname + page.url.search));
	let stack = $state<string[]>([]);
	const back = $derived(stack.length ? backTarget(stack) : null);
	const scrolls = new Map<string, number>();
	const scroller = () => document.querySelector<HTMLElement>('main');

	beforeNavigate(({ from }) => {
		const main = scroller();
		if (from && main) scrolls.set(from.url.pathname + from.url.search, main.scrollTop);
	});

	afterNavigate(async ({ to, type }) => {
		if (!to) return;
		const here = to.url.pathname + to.url.search;
		stack = nextStack(stack, here, type);
		await tick();
		const main = scroller();
		if (main) main.scrollTop = type === 'popstate' ? (scrolls.get(here) ?? 0) : 0;
	});

	/** Goes back (Q66): a history step to the previous screen, or the parent screen on a cold start. */
	function goBack() {
		if (!back) return;
		if (back.history) history.back();
		else void goto(back.path, { replaceState: true });
	}
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

	/**
	 * Asks the OS for one first-launch permission; Calendar access also imports the calendars.
	 * The server step asks nothing here: setup ends on it and opens Settings for the server.
	 */
	async function allow(id: SetupStep['id']) {
		if (id === 'notifications') await notificationPermission();
		else if (id === 'calendar') await data.run(async () => (await setSetting('calendars', 'true'), await syncCalendars()));
		else if (id === 'health') await healthConnect();
	}

	/** Saves that setup ran, so it never shows again, and opens Settings to sign in. */
	async function finishSetup() {
		await data.run(() => setSetting('setup_done', 'true'));
		await goto('/settings');
	}

	/** Opens a tab, the Inbox or Settings. */
	function navigate(to: Place) {
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
			<SetupScreen steps={setupSteps(data.snapshot?.settings ?? {})} onallow={allow} ondone={finishSetup} />
		{:else if wide}
			<DesktopShell {title} {tab} {back} onback={goBack} inboxCount={count} {categories} onnavigate={navigate}>{@render body()}</DesktopShell>
		{:else}
			<PhoneShell {title} {tab} {back} onback={goBack} inboxCount={count} onnavigate={navigate}>{@render body()}</PhoneShell>
		{/if}
	</div>
{/if}
