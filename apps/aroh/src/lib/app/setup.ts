/** One first-launch permission step: what it asks for and the one-line reason shown with it. */
export type SetupStep = { id: 'notifications' | 'calendar' | 'health' | 'server'; title: string; reason: string; allow: string };

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

/** The step a build without server defaults ends on: it opens Settings for the owner's own server. */
export const SERVER_STEP: SetupStep = { id: 'server', title: 'Your server', reason: 'This build has no server set. Enter your own in Settings.', allow: 'Use my own server' };

/** Whether no server is set: the Supabase URL, its anon key or the engine URL is missing or empty. */
export function needsServer(settings: Partial<Record<string, string>>): boolean {
	return !settings.supabase_url || !settings.supabase_anon_key || !settings.server_url;
}

/** The first-launch steps for these settings: the permission steps, then the server step when none is set. */
export function setupSteps(settings: Partial<Record<string, string>>): readonly SetupStep[] {
	return needsServer(settings) ? [...SETUP_STEPS, SERVER_STEP] : SETUP_STEPS;
}
