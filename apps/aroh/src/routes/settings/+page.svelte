<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import SettingsScreen from '$lib/ui/screens/SettingsScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { engine, keyStatus } from '$lib/app/engine';
	import { settingsView } from '$lib/app/views';
	import { syncCalendars } from '$lib/app/calendar';
	import { getVersion } from '@tauri-apps/api/app';

	const s = $derived(data.snapshot);
	const facts = $derived(s?.facts.filter((f) => !f.blocked).length ?? 0);
	let email = $state<string | null>(null);
	let last4 = $state<Record<string, string | null>>({});
	let consent = $state<boolean | null>(engine.cachedConsent() ?? null);
	let version = $state('');

	/** Reads the sign-in state and, when signed in, the stored keys (vendor and last 4 only) and AI consent. */
	async function refresh() {
		const a = await client.account().catch(() => null);
		email = a?.signed_in ? (a.email ?? 'Signed in') : null;
		const keys = email ? await engine.keys() : null;
		last4 = keys?.ok && s ? keyStatus(keys.value, s.providers) : {};
		const c = email ? await engine.consent() : null;
		consent = c?.ok ? c.value : null;
	}

	onMount(() => {
		void refresh();
		void getVersion().then((v) => (version = v), () => (version = ''));
	});

	/** Connects Apple Health (permission, then import) or turns imports off; returns the status line. */
	async function onhealth(on: boolean): Promise<string> {
		try {
			if (!on) {
				await data.run(() => client.setSetting('health', 'false'));
				return 'Apple Health imports are off.';
			}
			return await client.healthConnect();
		} catch (e) {
			return String(e);
		} finally {
			await data.load();
		}
	}

	/** Asks for notification permission and says whether reminders can show. */
	async function onnotify(): Promise<string> {
		const granted = await client.notificationPermission().catch(() => false);
		return granted ? 'Allowed. Reminders show before each session.' : 'Not allowed. Turn notifications on for Aroh in iOS Settings.';
	}

	/** Opens onboarding again from its start (Q72: sign-in lives there, not in Settings). */
	function onsignin() {
		void data.run(() => client.setSetting('setup_done', 'false'));
	}

	/** Signs out and returns to onboarding, where signing in or choosing a server happens. */
	function onsignout() {
		void data.run(async () => (await client.signOut(), await client.setSetting('setup_done', 'false')));
	}

	/** Records AI consent from Settings, for accounts that skipped it. */
	async function onconsent() {
		const r = await engine.giveConsent();
		if (!r.ok) data.error = r.message;
		await refresh();
	}
</script>

{#if s}
	<SettingsScreen
		settings={settingsView(s, last4, email)}
		{facts}
		{consent}
		{version}
		onopen={(path) => void goto(path)}
		onsetting={(name, value) => data.run(async () => (await client.setSetting(name, value), value === 'true' && (await syncCalendars())))}
		{onhealth}
		{onnotify}
		{onconsent}
		{onsignin}
		{onsignout}
	/>
{/if}
