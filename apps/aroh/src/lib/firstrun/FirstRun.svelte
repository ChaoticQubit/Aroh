<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { ensureNotificationPermission } from '../calendar/api';

	const DONE_KEY = 'aroh.firstrun.done';
	const SYNC_SKIP_KEY = 'aroh.firstrun.sync-skipped';

	let dismissed = $state(localStorage.getItem(DONE_KEY) === '1');
	let keyReady = $state<boolean | null>(null);
	let syncReady = $state<boolean | null>(null);
	let syncSkipped = $state(localStorage.getItem(SYNC_SKIP_KEY) === '1');
	let notifyReady = $state<boolean | null>(null);
	let notifyMessage = $state('');

	const allDone = $derived(keyReady === true && notifyReady === true && (syncReady === true || syncSkipped));
	const loaded = $derived(keyReady !== null && syncReady !== null && notifyReady !== null);

	/** Reads the three setup states without prompting the owner for anything. */
	async function refresh() {
		const [key, sync, notify] = await Promise.all([
			invoke<{ present: boolean }>('claude_key_status').then((s) => s.present).catch(() => false),
			invoke<{ signed_in: boolean }>('sync_status').then((s) => s.signed_in).catch(() => false),
			invoke<boolean | null>('plugin:notification|is_permission_granted').then((g) => g === true).catch(() => false)
		]);
		keyReady = key;
		syncReady = sync;
		notifyReady = notify;
	}

	/** Asks the OS for notification permission and records the outcome. */
	async function allowNotifications() {
		notifyMessage = '';
		try {
			notifyReady = await ensureNotificationPermission();
			if (!notifyReady) notifyMessage = 'Notifications are off. Turn them on for Aroh in iOS Settings.';
		} catch (e) {
			notifyMessage = String(e);
		}
	}

	function skipSync() {
		syncSkipped = true;
		localStorage.setItem(SYNC_SKIP_KEY, '1');
	}

	function dismiss() {
		dismissed = true;
		localStorage.setItem(DONE_KEY, '1');
	}

	$effect(() => {
		if (dismissed) return;
		refresh();
		const onVisible = () => document.visibilityState === 'visible' && refresh();
		document.addEventListener('visibilitychange', onVisible);
		return () => document.removeEventListener('visibilitychange', onVisible);
	});

	$effect(() => {
		if (allDone && !dismissed) dismiss();
	});
</script>

{#if !dismissed && loaded && !allDone}
	<section class="firstrun" aria-labelledby="firstrun-title">
		<h2 id="firstrun-title">Set up Aroh</h2>
		<p class="lede">Three steps before your first goal.</p>
		<ol>
			<li class:done={keyReady}>
				<span class="mark" aria-hidden="true">{keyReady ? '✓' : '1'}</span>
				<div class="body">
					<strong>Claude API key</strong>
					<span>Plans your goals. Stored in the keychain.</span>
				</div>
				{#if !keyReady}<a class="btn primary" href="/settings#claude-key">Add key</a>{/if}
			</li>
			<li class:done={syncReady || syncSkipped}>
				<span class="mark" aria-hidden="true">{syncReady ? '✓' : syncSkipped ? '–' : '2'}</span>
				<div class="body">
					<strong>Sync <em>optional</em></strong>
					<span>{syncSkipped && !syncReady ? 'Skipped. Set it up later in Settings.' : 'Sign in to Supabase to back up and share across devices.'}</span>
				</div>
				{#if !syncReady && !syncSkipped}
					<div class="pair">
						<a class="btn" href="/settings#sync">Sign in</a>
						<button type="button" class="btn ghost" onclick={skipSync}>Skip</button>
					</div>
				{/if}
			</li>
			<li class:done={notifyReady}>
				<span class="mark" aria-hidden="true">{notifyReady ? '✓' : '3'}</span>
				<div class="body">
					<strong>Reminders</strong>
					<span>{notifyMessage || 'Get a nudge when an action starts or is due.'}</span>
				</div>
				{#if !notifyReady}<button type="button" class="btn primary" onclick={allowNotifications}>Allow</button>{/if}
			</li>
		</ol>
		<button type="button" class="later" onclick={dismiss}>Hide setup</button>
	</section>
{/if}

<style>
	.firstrun {
		margin-bottom: var(--space-4, 16px);
		padding: var(--space-4, 16px);
		background: var(--surface-raised);
		border: 1px solid var(--line);
		border-radius: 14px;
	}
	h2 {
		margin: 0;
		font-size: 1.0625rem;
	}
	.lede {
		margin: 4px 0 12px;
		color: var(--ink-muted);
		font-size: 0.875rem;
	}
	ol {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 12px;
	}
	li {
		display: grid;
		grid-template-columns: 28px 1fr;
		gap: 8px 12px;
		align-items: center;
	}
	.mark {
		width: 28px;
		height: 28px;
		border-radius: 50%;
		display: grid;
		place-items: center;
		font-size: 0.8125rem;
		font-weight: 600;
		border: 1px solid var(--line);
	}
	li.done .mark {
		background: var(--accent);
		color: var(--accent-ink);
		border-color: var(--accent);
	}
	li.done .body strong {
		color: var(--ink-muted);
	}
	.body {
		display: grid;
		gap: 2px;
		font-size: 0.875rem;
	}
	.body span {
		color: var(--ink-muted);
	}
	em {
		font-style: normal;
		font-weight: 400;
		color: var(--ink-muted);
	}
	.btn,
	.pair {
		grid-column: 2;
	}
	.pair {
		display: flex;
		gap: 8px;
	}
	.btn {
		min-height: 44px;
		padding: 0 16px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		justify-self: start;
		border-radius: 10px;
		border: 1px solid var(--line);
		background: transparent;
		color: inherit;
		font: inherit;
		font-size: 0.9375rem;
		text-decoration: none;
	}
	.btn.primary {
		background: var(--accent);
		color: var(--accent-ink);
		border-color: var(--accent);
	}
	.btn.ghost {
		border-color: transparent;
		color: var(--ink-muted);
	}
	.later {
		margin-top: 12px;
		min-height: 44px;
		padding: 0;
		background: none;
		border: none;
		color: var(--ink-muted);
		font: inherit;
		font-size: 0.8125rem;
	}
</style>
