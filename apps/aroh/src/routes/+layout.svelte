<script lang="ts">
	import '../app.css';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { onMount, type Snippet } from 'svelte';
	import PhoneShell from '$lib/ui/shell/PhoneShell.svelte';
	import DesktopShell from '$lib/ui/shell/DesktopShell.svelte';
	import type { TabId } from '$lib/ui/shell/nav';
	import { data } from '$lib/app/data.svelte';
	import { notificationPermission } from '$lib/app/client';
	import { categoryList, inboxCount } from '$lib/app/views';

	let { children }: { children: Snippet } = $props();

	const TITLES: Record<TabId | 'settings', string> = { today: 'Today', calendar: 'Calendar', goals: 'Goals', inbox: 'Inbox', log: 'Log', settings: 'Settings' };
	const bare = $derived((page.data as { bare?: boolean }).bare === true);
	const tab = $derived((Object.keys(TITLES) as (TabId | 'settings')[]).find((t) => page.url.pathname.startsWith(`/${t}`)) ?? 'today');
	const title = $derived(page.url.pathname === '/goals/new' ? (page.url.searchParams.has('goal') ? 'Goal chat' : 'New goal') : TITLES[tab]);
	const count = $derived(data.snapshot ? inboxCount(data.snapshot) : 0);
	const categories = $derived(data.snapshot ? categoryList(data.snapshot) : []);
	let wide = $state(false);

	onMount(() => {
		if (bare) return;
		const query = matchMedia('(min-width: 768px)');
		const onWidth = () => (wide = query.matches);
		onWidth();
		query.addEventListener('change', onWidth);
		const stop = data.start();
		addEventListener('pointerdown', askPermission, { capture: true });
		return () => {
			query.removeEventListener('change', onWidth);
			removeEventListener('pointerdown', askPermission, { capture: true });
			stop();
		};
	});

	/** Asks for notification permission at the owner's first real tap, not at launch; then stops listening. */
	function askPermission(event: PointerEvent) {
		if (!event.isTrusted) return;
		removeEventListener('pointerdown', askPermission, { capture: true });
		void notificationPermission().catch(() => false);
	}

	/** Opens a tab or Settings. */
	function navigate(to: TabId | 'settings') {
		void goto(`/${to}`);
	}
</script>

{#snippet body()}
	{#if data.error}<p role="alert" class="px-4 pt-3 text-sm text-status-missed">{data.error}</p>{/if}
	{#key data.day}{@render children()}{/key}
{/snippet}

{#if bare}
	{@render children()}
{:else}
	<div class="fixed inset-0">
		{#if wide}
			<DesktopShell {title} {tab} inboxCount={count} {categories} onnavigate={navigate}>{@render body()}</DesktopShell>
		{:else}
			<PhoneShell {title} {tab} inboxCount={count} onnavigate={navigate}>{@render body()}</PhoneShell>
		{/if}
	</div>
{/if}
