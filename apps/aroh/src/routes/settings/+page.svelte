<script lang="ts">
	import { onMount } from 'svelte';
	import SettingsScreen from '$lib/ui/screens/SettingsScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { catalog } from '$lib/app/catalog';
	import { engine, keyStatus } from '$lib/app/engine';
	import { settingsView } from '$lib/app/views';

	const s = $derived(data.snapshot);
	let email = $state<string | null>(null);
	let last4 = $state<Record<string, string | null>>({});

	/** Reads the sign-in state and, when signed in, the stored keys (vendor and last 4 only). */
	async function refresh() {
		const a = await client.account().catch(() => null);
		email = a?.signed_in ? (a.email ?? 'Signed in') : null;
		const keys = email ? await engine.keys() : null;
		last4 = keys?.ok && s ? keyStatus(keys.value, s.providers) : {};
	}

	onMount(() => void refresh());

	/** The vendor's provider row, created with its first model when the owner never picked it. */
	async function providerRow(vendor: string): Promise<client.ProviderSetting | null> {
		const row = s?.providers.find((p) => p.vendor === vendor);
		if (row) return row;
		const model = catalog.find((c) => c.id === vendor)?.models[0];
		return model ? client.saveProvider(vendor, model) : null;
	}

	/** Hands the typed key to the engine, then runs the engine's key test; the app keeps no key. */
	async function ontest(vendor: string, key: string): Promise<string> {
		const row = await providerRow(vendor).catch(() => null);
		if (!row) return 'Pick a provider first.';
		if (key) {
			const saved = await engine.saveKey(vendor, key);
			if (!saved.ok) return saved.message;
		}
		const tested = await engine.testConnection(row.id);
		await refresh();
		return tested.ok ? tested.value : tested.message;
	}

	/** Signs in or creates the account, then reloads the account state; returns the status line. */
	async function onsignin(address: string, password: string, create: boolean): Promise<string> {
		try {
			await client.signIn(address, password, create);
			await refresh();
			return 'Signed in. This device syncs with your account.';
		} catch (e) {
			return String(e);
		}
	}

	/** Saves the per-goal budget as a device setting and the all-goals budget as a row. */
	function onbudget(scope: 'all' | 'goal', usd: number | null) {
		void data.run(() => (scope === 'all' ? client.saveBudget(usd) : client.setSetting('goal_budget_usd', usd === null ? '' : String(usd))));
	}
</script>

{#if s}
	<SettingsScreen
		settings={settingsView(s, last4, email)}
		onprovider={(vendor, model) => data.run(() => client.saveProvider(vendor, model))}
		{ontest}
		onsetting={(name, value) => data.run(() => client.setSetting(name, value))}
		{onbudget}
		{onsignin}
		onsignout={() => data.run(async () => (await client.signOut(), await refresh()))}
	/>
{/if}
