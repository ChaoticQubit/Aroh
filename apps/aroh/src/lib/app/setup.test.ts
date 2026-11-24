import { describe, expect, it } from 'vitest';
import { needsServer, nextStep, onboardingStart, PERMISSIONS, stepCount } from './setup';

describe('onboarding (Q72, Q76)', () => {
	it('starts at Welcome until setup_done is saved as true', () => {
		expect(onboardingStart({}, { signedIn: false, consentMissing: false })).toEqual({ step: 'welcome', only: false });
		expect(onboardingStart({ setup_done: 'false' }, { signedIn: true, consentMissing: false })).toEqual({ step: 'welcome', only: false });
		expect(onboardingStart({ setup_done: 'true' }, { signedIn: false, consentMissing: false })).toBeNull();
	});

	it('asks an existing signed-in account for AI consent alone', () => {
		expect(onboardingStart({ setup_done: 'true' }, { signedIn: true, consentMissing: true })).toEqual({ step: 'consent', only: true });
		expect(nextStep('consent', { signedIn: true, only: true })).toBeNull();
	});

	it('walks sign in, key, consent, permissions and first goal; consent is never skipped when signed in', () => {
		const signed = { signedIn: true, only: false };
		expect(nextStep('welcome', signed)).toBe('signin');
		expect(nextStep('signin', signed)).toBe('key');
		expect(nextStep('email', signed)).toBe('key');
		expect(nextStep('server', signed)).toBe('signin');
		expect(nextStep('key', signed)).toBe('consent');
		expect(nextStep('consent', signed)).toBe('permissions');
		expect(nextStep('permissions', signed)).toBe('goal');
		expect(nextStep('goal', signed)).toBeNull();
	});

	it('opens on the own-server form when this build has no server defaults', () => {
		const hosted = { supabase_url: 'https://p.supabase.co', supabase_anon_key: 'sb_publishable_x', server_url: 'https://engine.example.com' };
		expect(needsServer(hosted)).toBe(false);
		for (const missing of ['supabase_url', 'supabase_anon_key', 'server_url'] as const) {
			expect(needsServer({ ...hosted, [missing]: '' })).toBe(true);
			expect(needsServer({ ...hosted, [missing]: undefined })).toBe(true);
		}
		expect(nextStep('welcome', { signedIn: false, only: false, noServer: true })).toBe('server');
		expect(nextStep('server', { signedIn: false, only: false, noServer: false })).toBe('signin');
	});

	it('without an account skips the key and consent steps', () => {
		expect(nextStep('signin', { signedIn: false, only: false })).toBe('permissions');
	});

	it('counts four numbered steps and lists Calendar, Notifications, Location and Apple Health', () => {
		expect(stepCount('welcome')).toBeNull();
		expect(stepCount('email')).toEqual({ at: 1, of: 4 });
		expect(stepCount('consent')).toEqual({ at: 2, of: 4 });
		expect(stepCount('goal')).toEqual({ at: 4, of: 4 });
		expect(PERMISSIONS.map((p) => p.id)).toEqual(['calendar', 'notifications', 'location', 'health']);
		for (const p of PERMISSIONS) expect(p.why).not.toContain('\n');
	});
});
