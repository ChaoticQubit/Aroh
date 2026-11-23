import { describe, expect, it } from 'vitest';
import { needsSetup, SETUP_STEPS } from './setup';

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
});
