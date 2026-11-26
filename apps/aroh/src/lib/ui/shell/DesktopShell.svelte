<script lang="ts">
	import type { Snippet } from 'svelte';
	import Bell from '@lucide/svelte/icons/bell';
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import Settings from '@lucide/svelte/icons/settings';
	import Logo from '../brand/Logo.svelte';
	import type { CategorySlot } from '$lib/ui/contract';
	import { tabs, type Place } from './nav';
	import type { Back } from './back';

	/**
	 * Desktop frame of the app: left sidebar with the six tabs, a category filter with colour dots,
	 * Inbox with its unread count and Settings, then a titled main area with a back arrow to `back`
	 * on screens below a tab (`onback`). The main area is a size container.
	 */
	let {
		title,
		tab,
		inboxCount = 0,
		categories,
		actions,
		back = null,
		onback,
		onnavigate,
		children
	}: {
		title: string;
		tab: Place;
		back?: Back | null;
		onback?: () => void;
		inboxCount?: number;
		categories: { name: string; slot: CategorySlot }[];
		actions?: Snippet;
		onnavigate?: (to: Place) => void;
		children: Snippet;
	} = $props();

	const id = $props.id();
	let hidden = $state<string[]>([]);
	const item = 'relative flex h-8 w-full items-center gap-2.5 rounded-sm px-3 text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50';
</script>

<div class="flex h-full min-h-0 bg-bg text-fg">
	<aside class="flex w-[calc(var(--sidebar)+env(safe-area-inset-left))] shrink-0 flex-col gap-6 border-r border-border bg-bg p-2 pt-[max(0.5rem,env(safe-area-inset-top))] pb-[max(0.5rem,env(safe-area-inset-bottom))] pl-[max(0.5rem,env(safe-area-inset-left))]">
		<p class="flex items-center gap-2 px-3 pt-2 text-sm font-semibold"><Logo size={20} label="" />Aroh</p>
		<nav aria-label="Main" class="flex flex-col gap-0.5">
			{#each tabs as t (t.id)}
				<button type="button" aria-current={tab === t.id ? 'page' : undefined} class="{item} {tab === t.id ? 'bg-surface-2 font-medium text-fg before:absolute before:inset-y-1.5 before:left-0 before:w-0.5 before:bg-accent' : 'text-muted hover:bg-surface-2 hover:text-fg'}" onclick={() => onnavigate?.(t.id)}>
					<t.icon class="size-4" />
					<span class="flex-1 text-left">{t.label}</span>
				</button>
			{/each}
		</nav>
		<div class="flex flex-col gap-0.5" role="group" aria-labelledby={id}>
			<p {id} class="px-3 pb-1 text-xs font-medium text-muted">Categories</p>
			{#each categories as c (c.name)}
				{@const on = !hidden.includes(c.name)}
				<button type="button" aria-pressed={on} class="{item} text-fg hover:bg-surface-2" onclick={() => (hidden = on ? [...hidden, c.name] : hidden.filter((h) => h !== c.name))}>
					<span class="size-2 rounded-full {on ? '' : 'border'}" style={on ? `background: var(--cat-${c.slot})` : `border-color: var(--cat-${c.slot})`}></span>
					<span class="flex-1 text-left {on ? '' : 'text-muted'}">{c.name}</span>
				</button>
			{/each}
		</div>
		<div class="mt-auto flex flex-col gap-0.5">
			<button type="button" aria-current={tab === 'inbox' ? 'page' : undefined} class="{item} {tab === 'inbox' ? 'bg-surface-2 font-medium text-fg before:absolute before:inset-y-1.5 before:left-0 before:w-0.5 before:bg-accent' : 'text-muted hover:bg-surface-2 hover:text-fg'}" onclick={() => onnavigate?.('inbox')}>
				<Bell class="size-4" /><span class="flex-1 text-left">Inbox</span>
				{#if inboxCount > 0}<span class="rounded-sm border border-border bg-bg px-1.5 text-xs leading-4 font-medium text-fg tabular-nums">{inboxCount}</span>{/if}
			</button>
			<button type="button" aria-current={tab === 'settings' ? 'page' : undefined} class="{item} {tab === 'settings' ? 'bg-surface-2 font-medium text-fg before:absolute before:inset-y-1.5 before:left-0 before:w-0.5 before:bg-accent' : 'text-muted hover:bg-surface-2 hover:text-fg'}" onclick={() => onnavigate?.('settings')}>
				<Settings class="size-4" />Settings
			</button>
		</div>
	</aside>
	<div class="flex min-w-0 flex-1 flex-col pb-[env(safe-area-inset-bottom)]">
		<header class="box-content flex h-12 shrink-0 items-center justify-between gap-4 border-b border-border pt-[env(safe-area-inset-top)] pr-[max(1.5rem,env(safe-area-inset-right))] pl-6">
			<div class="flex min-w-0 items-center gap-2">
				{#if back}
					<button type="button" class="-ml-3 flex h-8 items-center gap-0.5 rounded-sm pr-2 pl-1 text-sm text-muted outline-none hover:text-fg focus-visible:ring-3 focus-visible:ring-ring/50" aria-label="Back to {back.label}" onclick={() => onback?.()}><ChevronLeft class="size-4" aria-hidden="true" />{back.label}</button>
				{/if}
				<h1 class="truncate text-lg font-semibold">{title}</h1>
			</div>
			{#if actions}<div class="flex items-center gap-2">{@render actions()}</div>{/if}
		</header>
		<main class="@container min-h-0 flex-1 overflow-y-auto bg-surface pr-[env(safe-area-inset-right)]">
			{@render children()}
		</main>
	</div>
</div>
