import type { SettingsSection } from '$lib/ui/components/SettingsList.svelte';

/** The approved "Good to know" lines under the Diary settings (Q85). */
export const DIARY_RISK = [
	'Your diary key lives in iCloud Keychain. If iCloud Keychain is off and this phone is lost, the key and every diary day are lost with it. Aroh cannot recover them.',
	'Food, water and workout numbers from your diary go to your trackers. Those numbers are stored without encryption, like any tracker entry.',
	'When a day is structured, its words go from this phone to Anthropic with your own Claude key.',
	'With agents reading on, the server holds a sealed copy of your diary key. That protects against a database leak, not against whoever runs the server.',
	'Session titles and coaching cards the agents write from your diary are stored like any other session.'
];

/**
 * The Diary settings sections with the agents toggle `on`, the Claude key row showing `last4`
 * (or that no key is set) and `ontoggle` for the switch. `busy` disables the switch while a
 * change runs.
 */
export function diarySettings(on: boolean, last4: string | null, ontoggle: (on: boolean) => void, busy = false): SettingsSection[] {
	return [
		{
			title: 'Encryption',
			rows: [
				{ label: 'Diary key', value: 'In iCloud Keychain', detail: 'Made on this phone. Our server and database only hold encrypted text.' },
				{ label: 'Claude key on this phone', value: last4 ? `ends ${last4}` : 'Not set', detail: 'A copy kept on this phone, so your days are structured from here and never through the Aroh server.' }
			]
		},
		{
			title: 'Agents',
			note: on
				? 'On: the server keeps a sealed copy of your diary key and reads summaries of your last 7 days for replans and cards. This protects against a database leak, not against the server operator. Turning it off deletes the copy and re-encrypts your diary with a new key.'
				: 'Off: no one but your devices can read your diary. Agents plan without it.',
			rows: [{ label: 'Let agents read my diary', detail: 'Summary, mood, energy and notes of the last 7 days. Never your exact words.', toggle: on, ontoggle, disabled: busy }]
		}
	];
}
