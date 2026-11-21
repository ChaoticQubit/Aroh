<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';

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

	$effect(() => {
		refresh();
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

<style>
	section {
		max-width: 440px;
		background: var(--surface-raised);
		border: 1px solid var(--line);
		border-radius: var(--radius);
		padding: var(--space-4);
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
