import { describe, expect, it } from 'vitest';
import { actionStyle, logState } from './status';

describe('action status style', () => {
	it('draws done struck with a green Done label and no category tint, the same everywhere', () => {
		expect(actionStyle('done')).toEqual({ title: 'text-muted line-through', label: 'Done', labelClass: 'text-status-done', tint: 0 });
		expect(actionStyle('planned')).toEqual({ title: 'text-fg', label: null, labelClass: '', tint: 8 });
	});

	it('keeps missed red and struck, skipped grey, both untinted', () => {
		expect(actionStyle('missed')).toMatchObject({ title: 'text-status-missed line-through', label: 'Missed', labelClass: 'text-status-missed', tint: 0 });
		expect(actionStyle('skipped')).toMatchObject({ title: 'text-muted line-through', label: 'Skipped', labelClass: 'text-status-skipped', tint: 0 });
	});
});

describe('log state (Q78)', () => {
	const zone = 'Europe/Berlin';
	const run = { status: 'planned' as const, start: '2026-11-24T06:00:00Z', end: '2026-11-24T06:30:00Z' };

	it('says nothing before the end, Not logged yet until the local day ends, then Missed', () => {
		expect(logState(run, '2026-11-24T06:10:00Z', zone)).toBeNull();
		expect(logState(run, '2026-11-24T06:31:00Z', zone)).toEqual({ tone: 'warn', label: 'Not logged yet' });
		expect(logState(run, '2026-11-24T22:59:00Z', zone)).toEqual({ tone: 'warn', label: 'Not logged yet' });
		expect(logState(run, '2026-11-24T23:00:00Z', zone)).toEqual({ tone: 'missed', label: 'Missed' });
	});

	it('keeps an action ending at midnight Not logged yet for the grace', () => {
		const late = { status: 'planned' as const, start: '2026-11-24T22:30:00Z', end: '2026-11-24T23:00:00Z' };
		expect(logState(late, '2026-11-24T23:10:00Z', zone)?.label).toBe('Not logged yet');
		expect(logState(late, '2026-11-24T23:15:00Z', zone)?.label).toBe('Missed');
	});

	it('names every logged status, which stays editable', () => {
		expect(logState({ ...run, status: 'done' }, '2026-11-24T06:10:00Z', zone)).toEqual({ tone: 'done', label: 'Done' });
		expect(logState({ ...run, status: 'skipped' }, '2026-11-24T06:10:00Z', zone)).toEqual({ tone: 'skipped', label: 'Skipped' });
		expect(logState({ ...run, status: 'missed' }, '2026-11-25T06:10:00Z', zone)).toEqual({ tone: 'missed', label: 'Missed' });
	});
});
