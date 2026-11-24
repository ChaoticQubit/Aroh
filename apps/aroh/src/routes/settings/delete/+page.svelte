<script lang="ts">
	import { goto } from '$app/navigation';
	import { Button } from '$lib/ui/primitives/button';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';

	let busy = $state(false);
	let error = $state<string | null>(null);

	/** Deletes the account (15-day grace, Q76): the server disables it, this device signs out and erases its data, then onboarding opens. */
	async function remove() {
		if (busy) return;
		busy = true;
		error = null;
		try {
			await client.deleteAccount();
		} catch (e) {
			error = String(e);
			busy = false;
			return;
		}
		await data.run(async () => {
			await client.setSetting('setup_done', 'false');
		});
		busy = false;
	}
</script>

<div class="flex w-full max-w-2xl flex-col gap-6 p-4 @3xl:p-6">
	<div class="flex flex-col gap-3">
		<h1 class="text-2xl leading-tight font-semibold">Delete your account?</h1>
		<p class="text-lg text-muted">Your account is disabled now and deleted for good after 15 days.</p>
	</div>
	<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
		{#each [['Now', 'The account is disabled: agents stop and nothing syncs. This phone signs out.'], ['Within 15 days', 'Sign back in to keep the account.'], ['After 15 days', 'Everything on the server is deleted: goals, plans, logs, files, coaches and your Claude key. This cannot be undone.']] as [t, d] (t)}
			<li class="flex flex-col px-4 py-3"><span class="text-sm font-medium">{t}</span><span class="text-sm text-muted">{d}</span></li>
		{/each}
	</ul>
	{#if error}<p role="alert" class="text-sm text-status-missed">{error}</p>{/if}
	<div class="flex flex-col gap-3">
		<Button variant="destructive" class="h-12 text-lg" disabled={busy} onclick={remove}>{busy ? 'Deleting' : 'Delete account'}</Button>
		<Button variant="outline" class="h-12 text-lg" disabled={busy} onclick={() => void goto('/settings')}>Keep my account</Button>
	</div>
</div>
