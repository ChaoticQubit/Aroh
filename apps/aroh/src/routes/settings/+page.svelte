<script lang="ts">
	import SettingsScreen from '$lib/ui/screens/SettingsScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { engine } from '$lib/app/engine';
	import { settingsView } from '$lib/app/views';

	const s = $derived(data.snapshot);

	/** Hands the typed key to the engine and tests the connection; the app keeps no key. */
	async function ontest(providerId: string, key: string): Promise<string> {
		if (key) {
			const saved = await engine.saveKey(providerId, key);
			if (!saved.ok) return saved.message;
		}
		const tested = await engine.testConnection(providerId);
		return tested.ok ? tested.value : tested.message;
	}

	/** Saves the per-goal budget as a device setting and the all-goals budget as a row. */
	function onbudget(scope: 'all' | 'goal', usd: number | null) {
		void data.run(() => (scope === 'all' ? client.saveBudget(usd) : client.setSetting('goal_budget_usd', usd === null ? '' : String(usd))));
	}
</script>

{#if s}
	<SettingsScreen
		settings={settingsView(s)}
		onprovider={(vendor, model) => data.run(() => client.saveProvider(vendor, model))}
		{ontest}
		onsetting={(name, value) => data.run(() => client.setSetting(name, value))}
		{onbudget}
	/>
{/if}
