<script lang="ts">
	import '../app.css';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import type { Snippet } from 'svelte';
	import { onNotificationRoute } from '$lib/log/api';
	import ReplanWatcher from '$lib/replan/ReplanWatcher.svelte';

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
	<div class="statusbar" aria-hidden="true"></div>
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
		position: fixed;
		inset: 0;
		display: grid;
		grid-template-rows: env(safe-area-inset-top) minmax(0, 1fr) auto;
		background: var(--surface);
	}

	.statusbar {
		grid-row: 1;
		background: var(--surface);
	}

	main {
		grid-row: 2;
		min-height: 0;
		padding: var(--space-3) max(var(--space-3), env(safe-area-inset-right)) var(--space-4)
			max(var(--space-3), env(safe-area-inset-left));
		overflow-y: auto;
		overscroll-behavior-y: contain;
		-webkit-overflow-scrolling: touch;
	}

	nav {
		grid-row: 3;
		display: flex;
		min-height: calc(var(--tabbar) + env(safe-area-inset-bottom));
		background: var(--surface-raised);
		border-top: 1px solid var(--line);
		padding: 0 env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left);
	}

	.brand {
		display: none;
	}

	a {
		flex: 1;
		min-height: var(--tabbar);
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
			grid-template-columns: var(--sidebar) minmax(0, 1fr);
		}

		.statusbar {
			display: none;
		}

		main {
			grid-row: auto;
			grid-column: 2;
			padding: var(--space-5);
		}

		nav {
			grid-row: auto;
			grid-column: 1;
			min-height: 0;
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
