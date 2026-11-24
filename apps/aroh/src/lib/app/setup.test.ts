import { describe, expect, it } from 'vitest';
import { needsServer, needsSetup, SETUP_STEPS, setupSteps } from './setup';

describe('first-launch setup', () => {
	it('asks for notifications, Calendar and Health in that order, each with a one-line reason', () => {
		expect(SETUP_STEPS.map((s) => s.id)).toEqual(['notifications', 'calendar', 'health']);
		for (const step of SETUP_STEPS) {
			expect(step.reason).not.toContain('\n');
			expect(step.reason.length).toBeLessThanOrEqual(80);
			expect(step.allow.length).toBeGreaterThan(0);
		}
	});

	it('shows once: until setup_done is saved as true', () => {
		expect(needsSetup({})).toBe(true);
		expect(needsSetup({ setup_done: 'false' })).toBe(true);
		expect(needsSetup({ setup_done: 'true' })).toBe(false);
	});

	it('asks for an own server last when this build has no server defaults', () => {
		const hosted = { supabase_url: 'https://p.supabase.co', supabase_anon_key: 'sb_publishable_x', server_url: 'https://engine.example.com' };
		expect(needsServer(hosted)).toBe(false);
		expect(setupSteps(hosted)).toEqual(SETUP_STEPS);
		for (const missing of ['supabase_url', 'supabase_anon_key', 'server_url'] as const) {
			expect(needsServer({ ...hosted, [missing]: '' })).toBe(true);
			expect(needsServer({ ...hosted, [missing]: undefined })).toBe(true);
		}
		const steps = setupSteps({});
		expect(steps.map((s) => s.id)).toEqual(['notifications', 'calendar', 'health', 'server']);
		const server = steps[3];
		expect(server.allow).toBe('Use my own server');
		expect(server.reason).not.toContain('\n');
		expect(server.reason.length).toBeLessThanOrEqual(80);
	});
});
