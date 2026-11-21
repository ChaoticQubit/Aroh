<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import TrackerSettings from '$lib/log/TrackerSettings.svelte';

	type KeyStatus = { present: boolean; last4: string | null };

	let status = $state<KeyStatus | null>(null);
	let draft = $state('');
	let message = $state('');
	let busy = $state(false);

	async function run(task: () => Promise<KeyStatus>, done: string) {
		busy = true;
		message = '';
		try {
			status = await task();
			message = done;
		} catch (err) {
			message = String(err);
		} finally {
			busy = false;
		}
	}

	function refresh() {
		return run(() => invoke<KeyStatus>('claude_key_status'), '');
	}

	function save(event: SubmitEvent) {
		event.preventDefault();
		const key = draft;
		draft = '';
		return run(() => invoke<KeyStatus>('save_claude_key', { key }), 'Key saved to the keychain.');
	}

	function clear() {
		return run(() => invoke<KeyStatus>('clear_claude_key'), 'Key removed.');
	}

	type SyncView = {
		configured: boolean;
		signed_in: boolean;
		email: string | null;
		last_sync_at: number | null;
		last_error: string | null;
	};

	let sync = $state<SyncView | null>(null);
	let syncUrl = $state('');
	let syncKey = $state('');
	let syncEmail = $state('');
	let syncPassword = $state('');
	let syncMessage = $state('');
	let syncBusy = $state(false);

	async function runSync(task: () => Promise<SyncView>, done: string) {
		syncBusy = true;
		syncMessage = '';
		try {
			sync = await task();
			if (done) syncMessage = done;
		} catch (err) {
			syncMessage = String(err);
			sync = await invoke<SyncView>('sync_status').catch(() => sync);
		} finally {
			syncBusy = false;
		}
	}

	function configureSync(event: SubmitEvent) {
		event.preventDefault();
		const url = syncUrl.trim();
		const anonKey = syncKey.trim();
		syncKey = '';
		return runSync(() => invoke<SyncView>('sync_configure', { url, anonKey }), 'Sync project saved.');
	}

	function signIn(event: SubmitEvent) {
		event.preventDefault();
		const email = syncEmail.trim();
		const password = syncPassword;
		syncPassword = '';
		return runSync(() => invoke<SyncView>('sync_sign_in', { email, password }), 'Signed in.');
	}

	function signOut() {
		return runSync(() => invoke<SyncView>('sync_sign_out'), 'Signed out.');
	}

	function syncNow() {
		return runSync(async () => {
			const report = await invoke<{ pushed: number; pulled: number }>('sync_now');
			syncMessage = `Pushed ${report.pushed}, pulled ${report.pulled}.`;
			return invoke<SyncView>('sync_status');
		}, '');
	}

	function when(ms: number | null) {
		return ms === null ? 'Never' : new Date(ms).toLocaleString();
	}

	$effect(() => {
		refresh();
		runSync(() => invoke<SyncView>('sync_status'), '');
	});
</script>

<h1>Settings</h1>

<section aria-labelledby="claude-key">
	<h2 id="claude-key">Claude API key</h2>
	<p class="status" aria-live="polite">
		{#if status === null}
			Checking keychain...
		{:else if status.present}
			Saved, ending in <code>{status.last4}</code>.
		{:else}
			No key saved.
		{/if}
	</p>
	<form onsubmit={save}>
		<label for="key">New key</label>
		<input id="key" type="password" autocomplete="off" spellcheck="false" bind:value={draft} placeholder="sk-ant-..." />
		<div class="row">
			<button type="submit" class="primary" disabled={busy || draft.trim() === ''}>Save</button>
			<button type="button" onclick={clear} disabled={busy || !status?.present}>Clear</button>
		</div>
	</form>
	{#if message}<p class="message" role="status">{message}</p>{/if}
</section>

<section aria-labelledby="sync">
	<h2 id="sync">Sync</h2>
	{#if sync === null}
		<p class="status" aria-live="polite">Checking sync...</p>
	{:else if !sync.configured}
		<p class="status">Connect a Supabase project to sync across devices.</p>
		<form onsubmit={configureSync}>
			<label for="sync-url">Supabase URL</label>
			<input id="sync-url" type="url" inputmode="url" autocomplete="off" autocapitalize="off" spellcheck="false" bind:value={syncUrl} placeholder="https://xyz.supabase.co" />
			<label for="sync-key">Publishable key</label>
			<input id="sync-key" type="password" autocomplete="off" spellcheck="false" bind:value={syncKey} placeholder="sb_publishable_..." />
			<div class="row">
				<button type="submit" class="primary" disabled={syncBusy || syncUrl.trim() === '' || syncKey.trim() === ''}>Connect</button>
			</div>
		</form>
	{:else if !sync.signed_in}
		<p class="status">Sign in to start syncing.</p>
		<form onsubmit={signIn}>
			<label for="sync-email">Email</label>
			<input id="sync-email" type="email" inputmode="email" autocomplete="username" autocapitalize="off" spellcheck="false" bind:value={syncEmail} />
			<label for="sync-password">Password</label>
			<input id="sync-password" type="password" autocomplete="current-password" bind:value={syncPassword} />
			<div class="row">
				<button type="submit" class="primary" disabled={syncBusy || syncEmail.trim() === '' || syncPassword === ''}>Sign in</button>
			</div>
		</form>
	{:else}
		<dl class="facts" aria-live="polite">
			<dt>Signed in as</dt>
			<dd>{sync.email ?? 'Unknown'}</dd>
			<dt>Last sync</dt>
			<dd>{when(sync.last_sync_at)}</dd>
			{#if sync.last_error}
				<dt>Last error</dt>
				<dd class="error">{sync.last_error}</dd>
			{/if}
		</dl>
		<div class="row">
			<button type="button" class="primary" onclick={syncNow} disabled={syncBusy}>{syncBusy ? 'Syncing...' : 'Sync now'}</button>
			<button type="button" onclick={signOut} disabled={syncBusy}>Sign out</button>
		</div>
	{/if}
	{#if syncMessage}<p class="message" role="status">{syncMessage}</p>{/if}
</section>

<TrackerSettings />

<style>
	section {
		max-width: 440px;
		background: var(--surface-raised);
		border: 1px solid var(--line);
		border-radius: var(--radius);
		padding: var(--space-4);
	}

	section + section {
		margin-top: var(--space-4);
	}

	.facts {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: var(--space-1) var(--space-3);
		margin: 0 0 var(--space-3);
		font-size: 0.875rem;
	}

	.facts dt {
		color: var(--ink-muted);
	}

	.facts dd {
		margin: 0;
		overflow-wrap: anywhere;
	}

	.facts .error {
		color: var(--danger);
	}

	h2 {
		font-size: 1rem;
		font-weight: 600;
		margin: 0 0 var(--space-2);
	}

	.status,
	.message {
		color: var(--ink-muted);
		margin: 0 0 var(--space-3);
	}

	.message {
		margin: var(--space-3) 0 0;
	}

	form {
		display: grid;
		gap: var(--space-2);
	}

	label {
		font-size: 0.875rem;
	}

	input {
		min-height: 44px;
		padding: 0 var(--space-3);
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--surface);
	}

	.row {
		display: flex;
		gap: var(--space-2);
	}

	button {
		min-height: 44px;
		padding: 0 var(--space-3);
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--surface);
		cursor: pointer;
	}

	button.primary {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--accent-ink);
	}

	button:disabled {
		opacity: 0.5;
		cursor: default;
	}

	input:focus-visible,
	button:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}
</style>
