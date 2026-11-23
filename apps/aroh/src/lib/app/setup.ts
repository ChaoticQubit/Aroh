/** One first-launch permission step: what it asks for and the one-line reason shown with it. */
export type SetupStep = { id: 'notifications' | 'calendar' | 'health'; title: string; reason: string; allow: string };

/** The first-launch steps in order. Each can be skipped; sign-in follows the last one. */
export const SETUP_STEPS: readonly SetupStep[] = [
	{ id: 'notifications', title: 'Reminders', reason: 'Aroh reminds you when a planned action is due.', allow: 'Allow notifications' },
	{ id: 'calendar', title: 'Your calendar', reason: 'Aroh plans around the events already on your calendar.', allow: 'Allow calendar access' },
	{ id: 'health', title: 'Apple Health', reason: 'Aroh reads steps, workouts and weight to track fitness goals.', allow: 'Connect Apple Health' }
];

/** Whether the first-launch setup still has to show: until `setup_done` is saved as `true`. */
export function needsSetup(settings: Partial<Record<string, string>>): boolean {
	return settings.setup_done !== 'true';
}
