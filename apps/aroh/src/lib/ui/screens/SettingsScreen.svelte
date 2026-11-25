<script lang="ts">
	import SettingsList, { type SettingsSection } from '../components/SettingsList.svelte';
	import type { SettingsView } from './types';

	/**
	 * Settings root (Q72, Q76): one grouped list; heavy items open sub-pages through `onopen`
	 * (a path). Switches report through `onsetting` and `onhealth`; `facts` counts what Aroh knows,
	 * `consent` is the server's AI consent (null when unknown) and `version` the app version.
	 */
	let {
		settings,
		facts = 0,
		consent = null,
		version = '',
		onopen,
		onsetting,
		onhealth,
		onnotify,
		onconsent,
		onsignin,
		onsignout
	}: {
		settings: SettingsView;
		facts?: number;
		consent?: boolean | null;
		version?: string;
		onopen?: (path: string) => void;
		onsetting?: (key: 'calendars', value: string) => void;
		onhealth?: (on: boolean) => Promise<string>;
		onnotify?: () => Promise<string>;
		onconsent?: () => void;
		onsignin?: () => void;
		onsignout?: () => void;
	} = $props();

	let healthStatus = $state<string | null>(null);
	let notifyStatus = $state<string | null>(null);
	let health = $derived(settings.health);
	let calendars = $derived(settings.calendars);
	const chosen = $derived(settings.providers.find((p) => p.id === settings.providerId) ?? settings.providers[0]);
	const go = (path: string) => () => onopen?.(path);
	const money = (usd: number | null) => (usd === null ? 'None' : `$${usd}`);
	const host = (url: string) => url.replace(/^https?:\/\//, '').replace(/\/$/, '') || 'Not set';

	/** Connects or stops Apple Health through `onhealth` and shows its answer under the row. */
	async function toggleHealth(on: boolean) {
		health = on;
		healthStatus = on ? 'Connecting to Apple Health.' : null;
		healthStatus = onhealth ? await onhealth(on) : null;
	}

	/** Asks for notification permission through `onnotify` and shows the answer. */
	async function notify() {
		notifyStatus = onnotify ? await onnotify() : null;
	}

	const sections = $derived<SettingsSection[]>([
		{
			title: 'Account',
			rows: settings.account.email
				? [
						{ label: 'Signed in', value: settings.account.email },
						{ label: 'Sign out', onclick: () => onsignout?.() },
						{ label: 'Delete account', detail: 'Disabled at once, deleted after 15 days.', danger: true, go: true, onclick: go('/settings/delete') }
					]
				: [{ label: 'Sign in or create an account', detail: 'Agents and sync need an account.', go: true, onclick: () => onsignin?.() }]
		},
		{
			title: 'Agents and coaches',
			rows: [
				{ label: 'Agent defaults', detail: 'What new goals start with, and the time limit per run.', go: true, onclick: go('/settings/agents') },
				{ label: 'Coach library', detail: 'Every coach by team. Copy one to change it.', go: true, onclick: go('/personas') }
			]
		},
		{
			title: 'Claude key and budget',
			rows: [
				{ label: chosen.id === 'anthropic' ? 'Claude key' : `${chosen.name} key`, value: chosen.last4 ? `ends ${chosen.last4}` : chosen.keySet ? 'Saved' : 'Not set', go: true, onclick: go('/settings/key') },
				{ label: 'Model', value: settings.model, go: true, onclick: go('/settings/key') },
				{ label: 'Monthly budget', value: money(settings.budgetAll), go: true, onclick: go('/settings/key') }
			]
		},
		{
			title: 'Calendars and reminders',
			rows: [
				{ label: 'Calendars', detail: 'Read device calendars as busy time. Nothing is written back.', toggle: calendars, ontoggle: (on) => ((calendars = on), onsetting?.('calendars', String(on))) },
				{ label: 'Notifications', detail: notifyStatus ?? 'A reminder before each session.', value: 'Allow', onclick: notify },
				{ label: 'Apple Health', detail: healthStatus ?? 'Import weight, workouts and steps on iPhone.', toggle: health, ontoggle: (on) => void toggleHealth(on) }
			]
		},
		{ title: 'Location', rows: [{ label: 'Location', detail: 'Places, nudges, pause tracking and visit history.', go: true, onclick: go('/settings/location') }] },
		{ title: 'Diary', rows: [{ label: 'Diary', detail: 'Encryption, your keys and whether agents may read it.', go: true, onclick: go('/settings/diary') }] },
		{
			title: 'Privacy and what Aroh knows',
			rows: [
				{ label: 'What Aroh knows about you', value: facts === 1 ? '1 fact' : `${facts} facts`, go: true, onclick: go('/settings/profile') },
				consent === false
					? { label: 'Allow agents to use AI', detail: 'Agents do not run until you allow this.', value: 'Not allowed', go: true, onclick: () => onconsent?.() }
					: { label: 'AI data sharing', detail: 'Goal data goes to Anthropic with your own key when agents run.', value: consent ? 'Allowed' : undefined }
			]
		},
		{
			title: 'About',
			note: 'To change the server, sign out and choose Use my own server.',
			rows: [
				{ label: 'Version', value: version || undefined },
				{ label: 'Server', value: host(settings.serverUrl) }
			]
		}
	]);
</script>

<div class="flex w-full max-w-2xl flex-col p-4 @3xl:p-6">
	<SettingsList {sections} />
</div>
