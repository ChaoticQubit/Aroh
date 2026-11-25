<script lang="ts">
	import { onMount } from 'svelte';
	import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
	import SettingsList from '$lib/ui/components/SettingsList.svelte';
	import { DIARY_RISK, diarySettings } from '$lib/ui/diary/settings';
	import { data } from '$lib/app/data.svelte';
	import { engine, keyStatus } from '$lib/app/engine';
	import { diaryShare, diaryShareStatus, isUnavailable } from '$lib/app/diary';

	let on = $state(false);
	let busy = $state(false);
	let last4 = $state<string | null>(null);
	let message = $state<string | null>(null);
	let unavailable = $state(false);

	onMount(async () => {
		try {
			on = await diaryShareStatus();
		} catch (e) {
			unavailable = isUnavailable(e);
			if (!unavailable) message = String(e);
		}
		const keys = await engine.keys();
		if (keys.ok && data.snapshot) last4 = keyStatus(keys.value, data.snapshot.providers).anthropic ?? null;
	});

	/** Turns agent reading on or off; turning it off rotates the diary key on this phone. */
	async function ontoggle(next: boolean) {
		busy = true;
		message = next ? null : 'Turning off: re-encrypting your diary with a new key…';
		try {
			on = await diaryShare(next);
			message = null;
		} catch (e) {
			message = String(e).startsWith('the engine answered 409') ? 'Your diary moved to a newer key on another device. Sync, then try again.' : String(e);
			on = await diaryShareStatus().then((s) => s, () => on);
		} finally {
			busy = false;
		}
	}
</script>

<div class="flex flex-col gap-6 p-4">
	{#if unavailable}
		<p class="text-sm text-muted">The diary and its settings are on iPhone.</p>
	{:else}
		<SettingsList sections={diarySettings(on, last4, (next) => void ontoggle(next), busy)} />
		{#if message}<p class="px-1 text-sm text-muted" role="status">{message}</p>{/if}
	{/if}
	<section class="flex flex-col gap-2" aria-label="Before you turn it on">
		<h2 class="flex items-center gap-1.5 px-1 text-sm font-semibold"><TriangleAlert class="size-4 text-status-warn" aria-hidden="true" />Good to know</h2>
		<ul class="flex flex-col gap-2 rounded-sm border border-border bg-bg p-4 text-sm">
			{#each DIARY_RISK as r (r)}<li class="flex gap-2"><span aria-hidden="true" class="text-muted">·</span><span>{r}</span></li>{/each}
		</ul>
	</section>
</div>
