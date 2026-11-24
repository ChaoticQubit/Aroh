/** One onboarding screen (Q72): `email` and `server` are sub-screens of sign in. */
export type OnboardingStep = 'welcome' | 'signin' | 'email' | 'server' | 'key' | 'consent' | 'permissions' | 'goal';

/** A device permission onboarding asks for, with its one-line reason. */
export type Permission = { id: 'calendar' | 'notifications' | 'location' | 'health'; title: string; why: string };

/** The permissions screen's rows in order; each one is optional. */
export const PERMISSIONS: readonly Permission[] = [
	{ id: 'calendar', title: 'Calendar', why: 'Plans go around your meetings. Aroh only reads busy times and never writes to your calendars.' },
	{ id: 'notifications', title: 'Notifications', why: 'A reminder before each session and a note when a plan changes.' },
	{ id: 'location', title: 'Location', why: 'Nudges near places you name, like the gym. Visits stay on this phone.' },
	{ id: 'health', title: 'Apple Health', why: 'Imports weight, workouts and steps so you log less by hand.' }
];

const NUMBERED: Partial<Record<OnboardingStep, number>> = { signin: 1, email: 1, server: 1, key: 2, consent: 2, permissions: 3, goal: 4 };

/**
 * Where onboarding opens, or null when the app shows: Welcome until `setup_done` is `true`;
 * after that only the consent screen, for a signed-in account the server reports without AI
 * consent (accounts made before Q75). `only` means finishing that one screen closes onboarding.
 */
export function onboardingStart(settings: Partial<Record<string, string>>, account: { signedIn: boolean; consentMissing: boolean }): { step: OnboardingStep; only: boolean } | null {
	if (settings.setup_done !== 'true') return { step: 'welcome', only: false };
	if (account.signedIn && account.consentMissing) return { step: 'consent', only: true };
	return null;
}

/** Whether no server is set: the Supabase URL, its anon key or the engine URL is missing or empty. */
export function needsServer(settings: Partial<Record<string, string>>): boolean {
	return !settings.supabase_url || !settings.supabase_anon_key || !settings.server_url;
}

/**
 * The screen after `step`, or null when onboarding is done. A build without server defaults
 * (`noServer`) goes from Welcome to the own-server form. Without an account the key and consent
 * screens are skipped; "Add later" on the key still leads to consent, so no run starts without it.
 */
export function nextStep(step: OnboardingStep, at: { signedIn: boolean; only: boolean; noServer?: boolean }): OnboardingStep | null {
	if (at.only) return null;
	switch (step) {
		case 'welcome':
			return at.noServer ? 'server' : 'signin';
		case 'server':
			return 'signin';
		case 'signin':
		case 'email':
			return at.signedIn ? 'key' : 'permissions';
		case 'key':
			return 'consent';
		case 'consent':
			return 'permissions';
		case 'permissions':
			return 'goal';
		case 'goal':
			return null;
	}
}

/** The "Step n of 4" counter for `step`, or null on Welcome. */
export function stepCount(step: OnboardingStep): { at: number; of: number } | null {
	const at = NUMBERED[step];
	return at === undefined ? null : { at, of: 4 };
}
