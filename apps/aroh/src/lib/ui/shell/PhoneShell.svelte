<script lang="ts">
	import type { Snippet } from 'svelte';
	import Settings from '@lucide/svelte/icons/settings';
	import { tabs, type TabId } from './nav';

	/**
	 * Phone frame of the app: top bar with the screen title and the gear that opens Settings,
	 * a scrolling body, and the five-tab bar. Fills its container; the body is a size container.
	 */
	let {
		title,
		tab,
		inboxCount = 0,
		onnavigate,
		children
	}: { title: string; tab: TabId | 'settings'; inboxCount?: number; onnavigate?: (to: TabId | 'settings') => void; children: Snippet } = $props();
</script>

<div class="flex h-full min-h-0 flex-col bg-bg text-fg">
	<header class="box-content flex h-14 shrink-0 items-center justify-between border-b border-border px-4 pt-[env(safe-area-inset-top)]">
		<h1 class="text-lg font-semibold">{title}</h1>
		<button type="button" class="-mr-3 flex size-11 items-center justify-center rounded-sm text-muted outline-none hover:text-fg focus-visible:ring-3 focus-visible:ring-ring/50" aria-label="Settings" aria-current={tab === 'settings' ? 'page' : undefined} onclick={() => onnavigate?.('settings')}>
			<Settings class="size-4" />
		</button>
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
				{#if t.id === 'inbox' && inboxCount > 0}
					<span class="absolute top-2 left-1/2 ml-2 min-w-4 rounded-sm border border-border bg-bg px-1 text-center text-xs leading-4 font-medium text-fg tabular-nums" aria-label="{inboxCount} new">{inboxCount}</span>
				{/if}
			</button>
		{/each}
	</nav>
</div>
