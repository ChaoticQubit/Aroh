import { describe, expect, it } from 'vitest';
import { limitsView } from './limits';

const CHOICES = ['Move the date to Dec 15', 'Raise the daily minutes', 'Lower the target'];
const card = { cap: 20, today: '2026-11-23', target: '2026-12-05', finish: '2026-12-15', needed: 440, raise_to: 40, fits_minutes: 240 };

describe('limitsView', () => {
	it('draws the today, target and finish bar and gives each choice its number', () => {
		const v = limitsView('At 20 min/day you finish on Dec 15, 10 days after your target. What should change?', CHOICES, card);
		expect(v.headline).toBe('At 20 min/day you finish on Dec 15, 10 days after your target.');
		expect(v.bar).toEqual({ today: '23 Nov', target: '5 Dec', finish: '15 Dec', at: 54.5, pace: 'At 20 min' });
		expect(v.choices).toEqual([
			{ value: 'Move the date to Dec 15', kind: 'move', title: 'Move the date to Dec 15', detail: 'Keep 20 min a day' },
			{ value: 'Raise the daily minutes', kind: 'raise', title: 'Raise to 40 min a day', detail: 'Finishes by 5 Dec', minutes: 40 },
			{ value: 'Lower the target', kind: 'lower', title: 'Lower the target', detail: 'About 55% fits by 5 Dec at 20 min a day' }
		]);
	});

	it('keeps the server text and plain choices when the numbers are missing', () => {
		const v = limitsView('The plan does not fit in 20 minutes a day. What should change?', ['Raise the daily minutes', 'Lower the target'], null);
		expect(v.headline).toBe('The plan does not fit in 20 minutes a day.');
		expect(v.bar).toBeNull();
		expect(v.choices.map((c) => [c.kind, c.title, c.detail])).toEqual([
			['raise', 'Raise the daily minutes', 'Pick your new minutes a day'],
			['lower', 'Lower the target', 'Keep your minutes and your date']
		]);
	});
});
