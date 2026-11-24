<script lang="ts">
	import type { Snippet } from 'svelte';
	import Bell from '@lucide/svelte/icons/bell';
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import Settings from '@lucide/svelte/icons/settings';
	import { tabs, type Place } from './nav';
	import type { Back } from './back';

	/**
	 * Phone frame of the app: a top bar with the title (left on a tab root; centred after a back
	 * arrow to `back` below a tab, Q66), the Inbox bell with its unread count and the Settings gear
	 * (Q69), a scrolling body that is a size container, and the five-tab bar.
	 */
	let {
		title,
		tab,
		inboxCount = 0,
		back = null,
		onback,
		onnavigate,
		children
	}: { title: string; tab: Place; inboxCount?: number; back?: Back | null; onback?: () => void; onnavigate?: (to: Place) => void; children: Snippet } = $props();

	const icon = 'relative flex size-11 items-center justify-center rounded-sm outline-none hover:text-fg focus-visible:ring-3 focus-visible:ring-ring/50';
</script>

<div class="flex h-full min-h-0 flex-col bg-bg text-fg">
	<header class="box-content grid h-14 shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-1 border-b border-border px-4 pt-[env(safe-area-inset-top)]">
		{#if back}
			<button type="button" class="-ml-2 flex min-h-11 min-w-0 items-center gap-0.5 justify-self-start rounded-sm pr-2 text-md outline-none focus-visible:ring-3 focus-visible:ring-ring/50" aria-label="Back to {back.label}" onclick={() => onback?.()}>
				<ChevronLeft class="size-5 shrink-0" aria-hidden="true" /><span class="truncate">{back.label}</span>
			</button>
			<h1 class="max-w-44 truncate text-md font-semibold">{title}</h1>
		{:else}
			<h1 class="col-span-2 truncate text-lg font-semibold">{title}</h1>
		{/if}
		<div class="-mr-3 flex items-center justify-self-end">
			<button type="button" class="{icon} {tab === 'inbox' ? 'text-fg' : 'text-muted'}" aria-label="Inbox{inboxCount > 0 ? `, ${inboxCount} new` : ''}" aria-current={tab === 'inbox' ? 'page' : undefined} onclick={() => onnavigate?.('inbox')}>
				<Bell class="size-4" strokeWidth={tab === 'inbox' ? 2.25 : 1.75} />
				{#if inboxCount > 0}<span class="absolute top-1.5 right-1 min-w-4 rounded-sm bg-fg px-1 text-center text-xs leading-4 font-medium text-bg tabular-nums" aria-hidden="true">{inboxCount}</span>{/if}
			</button>
			<button type="button" class="{icon} {tab === 'settings' ? 'text-fg' : 'text-muted'}" aria-label="Settings" aria-current={tab === 'settings' ? 'page' : undefined} onclick={() => onnavigate?.('settings')}>
				<Settings class="size-4" strokeWidth={tab === 'settings' ? 2.25 : 1.75} />
			</button>
		</div>
	</header>
	<main class="@container min-h-0 flex-1 overflow-y-auto bg-surface">
		{@render children()}
	</main>
	<nav aria-label="Main" data-hide-typing class="flex shrink-0 border-t border-border bg-bg pb-[env(safe-area-inset-bottom)]">
		{#each tabs as t (t.id)}
			<button
				type="button"
				aria-current={tab === t.id ? 'page' : undefined}
				class="relative flex h-(--tabbar) flex-1 flex-col items-center justify-center gap-0.5 text-xs font-medium outline-none focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset {tab === t.id ? 'text-accent' : 'text-muted'}"
				onclick={() => onnavigate?.(t.id)}
			>
				<t.icon class="size-4" strokeWidth={tab === t.id ? 2.25 : 1.75} />
				{t.label}
			</button>
		{/each}
	</nav>
</div>
