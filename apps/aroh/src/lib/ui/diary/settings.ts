import type { SettingsSection } from '$lib/ui/components/SettingsList.svelte';

/** The "Good to know" lines under the Diary settings (Q85, Q94). */
export const DIARY_RISK = [
	'Your diary key lives in iCloud Keychain. If iCloud Keychain is off and this phone is lost, the key and every diary day are lost with it. Aroh cannot recover them.',
	'Food, water and workout numbers from your diary go to your trackers. Those numbers are stored without encryption, like any tracker entry.',
	'When a day is structured, the Aroh engine opens its words in memory and sends them to your AI provider with your key. They are never stored or logged unencrypted.',
	'The server always holds a sealed copy of your diary key, so it can structure your days. That protects against a database leak, not against whoever runs the server.',
	'Session titles and coaching cards the agents write from your diary are stored like any other session.'
];

/**
 * The Diary settings sections with the agents toggle `on` and `ontoggle` for the switch.
 * `busy` disables the switch while a change runs.
 */
export function diarySettings(on: boolean, ontoggle: (on: boolean) => void, busy = false): SettingsSection[] {
	return [
		{
			title: 'Encryption',
			rows: [{ label: 'Diary key', value: 'In iCloud Keychain', detail: 'Made on this phone. The database only holds encrypted text; the engine keeps a sealed copy to structure your days.' }]
		},
		{
			title: 'Agents',
			note: on
				? 'On: agents read summaries of your last 7 days for replans and cards. This protects against a database leak, not against the server operator.'
				: 'Off: agents plan without your diary. The engine still structures your days with its sealed copy of your key.',
			rows: [{ label: 'Let agents read my diary', detail: 'Summary, mood, energy and notes of the last 7 days. Never your exact words.', toggle: on, ontoggle, disabled: busy }]
		}
	];
}
