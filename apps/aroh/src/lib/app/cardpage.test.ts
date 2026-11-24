import { describe, expect, it } from 'vitest';
import type { Action, Snapshot } from './client';
import { cardMeta, fillDate, logOf, pageCard } from './cardpage';

const action: Action = {
	id: 'a1',
	goal_id: 'g1',
	series_id: 's1',
	occurrence: null,
	title: 'Leg day',
	notes: null,
	start_at: '2026-12-10T17:00:00Z',
	end_at: '2026-12-10T18:00:00Z',
	status: 'planned',
	locked: false,
	done_at: null,
	card: null
};
const snapshot = {
	categories: [],
	goals: [{ id: 'g1', category_id: null, title: 'Six pack', context: null, deadline: null, status: 'active', autonomy: 'ask', created_at: '2026-11-23T12:00:00Z', reminders: true, use_calendars: true, run_limit_minutes: null, ordered: false }],
	actions: []
} as unknown as Snapshot;

describe('card page', () => {
	it('builds the header in the zone with the goal week', () => {
		expect(cardMeta(snapshot, action, 'Europe/Berlin')).toMatchObject({ title: 'Leg day', goal: 'Six pack', date: '2026-12-10', start: '18:00', minutes: 60, week: 'Week 3' });
	});

	it('starts a blank editable card and an empty log', () => {
		expect(pageCard(action).blocks).toEqual([{ kind: 'steps', title: 'Steps', items: [{ text: '', minutes: null }] }]);
		expect(logOf(null)).toEqual({ sets: [], checked: [], effort: null, note: null });
	});

	it('fills an outline a week ahead, never in the past', () => {
		expect(fillDate(action, 'UTC', '2026-11-25')).toBe('2026-12-03');
		expect(fillDate(action, 'UTC', '2026-12-08')).toBe('2026-12-08');
	});
});
