import { describe, expect, it } from 'vitest';
import { agentDefaults, blankDraft, canCreate, defaultsValue, pickCategory, problem, quickPicks, startBody, withGuess, type NewGoalDraft } from './newgoal';

const draft = (patch: Partial<NewGoalDraft> = {}): NewGoalDraft => ({ ...blankDraft(), ...patch });

describe('new goal form', () => {
	it('needs a title before Create is enabled', () => {
		expect(canCreate(draft())).toBe(false);
		expect(problem(draft({ title: '   ' }))).toBe('Add a title to create the goal.');
		expect(canCreate(draft({ title: 'Finish reading Dune' }))).toBe(true);
		expect(problem(draft({ title: 'x'.repeat(201) }))).toBe('Keep the title under 200 characters.');
		expect(problem(draft({ title: 'Read', target: '31.12.2026' }))).toBe('Pick a target date.');
		expect(problem(draft({ title: 'Read', runLimit: 0 }))).toBe('The run time limit is 1 to 120 minutes.');
		expect(problem(draft({ title: 'Read', runLimit: 120 }))).toBeNull();
	});

	it('sends the form as the start goal body, leaving out empty fields', () => {
		const body = startBody(draft({ title: '  Read Dune ', description: '', target: '2026-12-31', categoryId: 'c1', autonomy: 'ask', reminders: false }), 'Europe/Berlin');
		expect(body).toEqual({ title: 'Read Dune', deadline: '2026-12-31', category_id: 'c1', autonomy: 'ask', reminders: false, use_calendars: true, timezone: 'Europe/Berlin' });
		const full = startBody(draft({ title: 'Run', description: ' Evenings ', runLimit: 10 }), 'UTC');
		expect(full).toMatchObject({ context: 'Evenings', run_limit_minutes: 10, autonomy: 'balanced' });
		expect(full).not.toHaveProperty('deadline');
		expect(full).not.toHaveProperty('category_id');
	});

	it('takes the category guess until the owner picks one', () => {
		const guessed = withGuess(draft({ title: 'Finish reading' }), 'learning-id');
		expect(guessed).toMatchObject({ categoryId: 'learning-id', categoryPicked: false });
		expect(withGuess(guessed, null).categoryId).toBeNull();
		const picked = pickCategory(guessed, 'garden-id');
		expect(withGuess(picked, 'learning-id')).toMatchObject({ categoryId: 'garden-id', categoryPicked: true });
	});

	it('offers quick target dates from today in the owner zone', () => {
		expect(quickPicks('2026-11-26')).toEqual([
			{ label: 'In 1 month', ymd: '2026-12-26' },
			{ label: 'In 3 months', ymd: '2027-02-26' },
			{ label: 'End of year', ymd: '2026-12-31' }
		]);
		expect(quickPicks('2027-01-31')[0]).toEqual({ label: 'In 1 month', ymd: '2027-02-28' });
		expect(quickPicks('2026-12-31')[2]).toEqual({ label: 'End of next year', ymd: '2027-12-31' });
	});

	it('reads the Settings agent defaults, falling back on a missing or bad value', () => {
		const base = { autonomy: 'balanced', reminders: true, useCalendars: true, runLimit: null };
		expect(agentDefaults(undefined)).toEqual(base);
		expect(agentDefaults('nope')).toEqual(base);
		expect(agentDefaults('{"autonomy":"wild","reminders":false,"use_calendars":false}')).toEqual(base);
		const ask = { autonomy: 'ask' as const, reminders: false, useCalendars: true, runLimit: 9 };
		expect(defaultsValue(ask)).toBe('{"autonomy":"ask","reminders":false,"use_calendars":true}');
		expect(agentDefaults(defaultsValue(ask))).toEqual({ ...ask, runLimit: null });
	});
});
