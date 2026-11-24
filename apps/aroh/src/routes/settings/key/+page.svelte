<script lang="ts">
	import { onMount } from 'svelte';
	import KeyScreen from '$lib/ui/screens/KeyScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { engine, keyStatus } from '$lib/app/engine';
	import { saveAndTestKey } from '$lib/app/keys';
	import { settingsView } from '$lib/app/views';

	const s = $derived(data.snapshot);
	let last4 = $state<Record<string, string | null>>({});

	/** Reads the stored keys: vendor and last 4 only. */
	async function refresh() {
		const keys = await engine.keys();
		last4 = keys.ok && s ? keyStatus(keys.value, s.providers) : {};
	}

	onMount(() => void refresh());

	/** Saves and tests the key for `vendor`, then reloads the key list; returns the status line. */
	async function ontest(vendor: string, key: string): Promise<string> {
		const r = await saveAndTestKey(s?.providers ?? [], vendor, key);
		await data.load();
		await refresh();
		return r.text;
	}

	/** Saves the per-goal budget as a device setting and the all-goals budget as a row. */
	function onbudget(scope: 'all' | 'goal', usd: number | null) {
		void data.run(() => (scope === 'all' ? client.saveBudget(usd) : client.setSetting('goal_budget_usd', usd === null ? '' : String(usd))));
	}
</script>

{#if s}
	<KeyScreen settings={settingsView(s, last4)} onprovider={(vendor, model) => data.run(() => client.saveProvider(vendor, model))} {ontest} {onbudget} />
{/if}
