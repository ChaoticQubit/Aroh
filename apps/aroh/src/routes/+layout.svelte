<script lang="ts">
	import '../app.css';
	import { page } from '$app/state';
	import { afterNavigate, beforeNavigate, goto } from '$app/navigation';
	import { onMount, tick, type Snippet } from 'svelte';
	import { prefetchOnPress } from '$lib/app/prefetch';
	import { listen } from '@tauri-apps/api/event';
	import PhoneShell from '$lib/ui/shell/PhoneShell.svelte';
	import DesktopShell from '$lib/ui/shell/DesktopShell.svelte';
	import favicon from '$lib/ui/brand/aroh-icon.svg';
	import type { Place } from '$lib/ui/shell/nav';
	import { backTarget, nextStack, screenTitle } from '$lib/ui/shell/back';
	import { data } from '$lib/app/data.svelte';
	import OnboardingScreen, { type OnboardingActions } from '$lib/ui/screens/OnboardingScreen.svelte';
	import * as client from '$lib/app/client';
	import { syncCalendars } from '$lib/app/calendar';
	import { engine } from '$lib/app/engine';
	import { saveAndTestKey } from '$lib/app/keys';
	import * as location from '$lib/app/location';
	import { needsServer, onboardingStart } from '$lib/app/setup';
	import { fitKeyboard } from '$lib/app/keyboard';
	import { categoryList, inboxCount } from '$lib/app/views';
	import { pushTarget } from '$lib/app/push';

	let { children }: { children: Snippet } = $props();

	const PLACES: Place[] = ['today', 'calendar', 'goals', 'dashboard', 'log', 'inbox', 'settings'];
	const bare = $derived((page.data as { bare?: boolean }).bare === true);
	const coaches = $derived(page.url.pathname.startsWith('/personas') || page.url.pathname.startsWith('/plugins'));
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
	let account = $state<{ signedIn: boolean; consentMissing: boolean } | null>(null);
	const onboarding = $derived(data.snapshot && account ? onboardingStart(data.snapshot.settings, account) : null);
	const setupDone = $derived(data.snapshot ? data.snapshot.settings.setup_done === 'true' : null);
	$effect(() => {
		if (bare || setupDone === null) return;
		if (!setupDone) account = null;
		void readAccount();
	});
	const apple = typeof navigator !== 'undefined' && /iPhone|iPad/.test(navigator.userAgent);
	let wide = $state(false);

	onMount(() => {
		if (bare) return;
		const query = matchMedia('(min-width: 768px)');
		const onWidth = () => (wide = query.matches);
		onWidth();
		query.addEventListener('change', onWidth);
		const stop = data.start();
		const unfit = fitKeyboard(window);
		void openPushTap();
		const unlisten = listen('aroh://push-tap', () => void openPushTap());
		return () => {
			query.removeEventListener('change', onWidth);
			stop();
			unfit();
			void unlisten.then((off) => off());
		};
	});

	/** Opens the screen of the last tapped run or proposal push (Q83), if one waits. */
	async function openPushTap() {
		const to = pushTarget(await client.takePushTap().catch(() => null));
		if (to) await goto(to);
	}

	/**
	 * Reads the sign-in state and, when signed in, whether the server lacks the owner's AI consent
	 * (Q75). A server without account endpoints or no connection counts as not missing.
	 */
	async function readAccount() {
		const a = await client.account().catch(() => null);
		const signedIn = a?.signed_in === true;
		const consent = signedIn ? await engine.consent() : null;
		account = { signedIn, consentMissing: consent?.ok === true && !consent.value };
	}

	/** Turns a rejected call into its message, or null when it resolved. */
	async function attempt(call: () => Promise<unknown>): Promise<string | null> {
		try {
			await call();
			return null;
		} catch (e) {
			return String(e);
		}
	}

	const actions: OnboardingActions = {
		onapple: () => attempt(client.signInApple),
		onemail: async (email, password, create) => {
			try {
				const a = await client.signIn(email, password, create);
				return a.check_email ? { checkEmail: a.email ?? email } : null;
			} catch (e) {
				return String(e);
			}
		},
		onserver: (supabaseUrl, anonKey, serverUrl) =>
			attempt(async () => {
				await client.setSetting('supabase_url', supabaseUrl);
				await client.setSetting('supabase_anon_key', anonKey);
				await client.setSetting('server_url', serverUrl);
			}),
		onkey: async (key) => {
			const r = await saveAndTestKey(data.snapshot?.providers ?? [], 'anthropic', key);
			await data.load();
			return r.ok ? null : r.text;
		},
		onconsent: async () => {
			const r = await engine.giveConsent();
			return r.ok ? null : r.message;
		},
		onallow: async (id) => {
			try {
				if (id === 'notifications') return await client.notificationPermission();
				if (id === 'calendar') await data.run(async () => (await client.setSetting('calendars', 'true'), await syncCalendars()));
				else if (id === 'location') await location.enable();
				else await client.healthConnect();
				return true;
			} catch (e) {
				data.error = String(e);
				return false;
			}
		},
		ondone: (goal) => void finishOnboarding(goal)
	};

	/** Saves that onboarding ran, so it never shows again, and opens the first goal's form or Today. */
	async function finishOnboarding(goal: string | null) {
		await data.run(() => client.setSetting('setup_done', 'true'));
		await readAccount();
		await goto(goal ? `/goals/new?title=${encodeURIComponent(goal)}` : '/today');
	}

	/** Opens a tab, the Inbox or Settings. */
	function navigate(to: Place) {
		void goto(`/${to}`);
	}
</script>

<svelte:head><link rel="icon" type="image/svg+xml" href={favicon} /></svelte:head>
<svelte:document onpointerdown={prefetchOnPress} />

{#snippet body()}
	{#if data.error}<p role="alert" class="px-4 pt-3 text-sm text-status-missed">{data.error}</p>{/if}
	{#key data.day + data.zone}{@render children()}{/key}
{/snippet}

{#if bare}
	{@render children()}
{:else}
	<div class="fixed inset-x-0 top-0" style="height: var(--app-h, 100%)">
		{#if onboarding}
			<OnboardingScreen start={onboarding} signedIn={account?.signedIn} {apple} noServer={needsServer(data.snapshot?.settings ?? {})} {actions} />
		{:else if wide}
			<DesktopShell {title} {tab} {back} onback={goBack} inboxCount={count} {categories} onnavigate={navigate}>{@render body()}</DesktopShell>
		{:else}
			<PhoneShell {title} {tab} {back} onback={goBack} inboxCount={count} onnavigate={navigate}>{@render body()}</PhoneShell>
		{/if}
	</div>
{/if}
