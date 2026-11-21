<script lang="ts">
	import '../app.css';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import type { Snippet } from 'svelte';
	import { onNotificationRoute } from '../lib/log/api';
	import ReplanWatcher from '../lib/replan/ReplanWatcher.svelte';

	let { children }: { children: Snippet } = $props();

	const tabs = [
		{ href: '/today', label: 'Today' },
		{ href: '/calendar', label: 'Calendar' },
		{ href: '/goals/new', label: 'New goal' },
		{ href: '/log', label: 'Log' },
		{ href: '/settings', label: 'Settings' }
	];

	const current = $derived(page.url.pathname);

	$effect(() => {
		const listening = onNotificationRoute((route) => goto(route));
		return () => {
			listening.then((l) => l?.unregister());
		};
	});
</script>

<div class="shell">
	<nav aria-label="Main">
		<span class="brand">Aroh</span>
		{#each tabs as tab (tab.href)}
			<a href={tab.href} aria-current={current.startsWith(tab.href) ? 'page' : undefined}>{tab.label}</a>
		{/each}
	</nav>
	<main>
		{@render children()}
	</main>
	<ReplanWatcher />
</div>

<style>
	.shell {
		min-height: 100vh;
		display: grid;
		grid-template-rows: 1fr var(--tabbar);
	}

	main {
		grid-row: 1;
		padding: var(--space-4) var(--space-3);
		padding-top: max(var(--space-4), env(safe-area-inset-top));
		overflow: auto;
	}

	nav {
		grid-row: 2;
		position: sticky;
		bottom: 0;
		display: flex;
		background: var(--surface-raised);
		border-top: 1px solid var(--line);
		padding-bottom: env(safe-area-inset-bottom);
	}

	.brand {
		display: none;
	}

	a {
		flex: 1;
		min-height: 44px;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 0.8125rem;
		color: var(--ink-muted);
		text-decoration: none;
		transition: color 150ms;
	}

	a[aria-current='page'] {
		color: var(--accent);
		font-weight: 600;
	}

	a:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: -2px;
	}

	@media (min-width: 768px) {
		.shell {
			grid-template-rows: none;
			grid-template-columns: var(--sidebar) 1fr;
		}

		main {
			grid-row: auto;
			grid-column: 2;
			padding: var(--space-5);
		}

		nav {
			grid-row: auto;
			grid-column: 1;
			position: sticky;
			top: 0;
			height: 100vh;
			flex-direction: column;
			gap: var(--space-1);
			border-top: none;
			border-right: 1px solid var(--line);
			padding: var(--space-4) var(--space-2);
		}

		.brand {
			display: block;
			font-weight: 600;
			padding: 0 var(--space-3) var(--space-3);
		}

		a {
			flex: none;
			justify-content: flex-start;
			min-height: 36px;
			padding: 0 var(--space-3);
			border-radius: var(--radius);
			font-size: 0.9375rem;
		}

		a[aria-current='page'] {
			background: var(--surface);
		}
	}
</style>
