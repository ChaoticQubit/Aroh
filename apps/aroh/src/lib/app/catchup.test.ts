import { describe, expect, it } from 'vitest';
import { askCatchUp, offerOf } from './catchup';

const add = (title: string, start_at: string) => ({ title, start_at });

describe('catch-up offer (Q78)', () => {
	it('asks the server only when an action that already ended is newly done', () => {
		const end = '2026-11-24T06:30:00Z';
		expect(askCatchUp('missed', 'done', end, '2026-11-25T08:00:00Z')).toBe(true);
		expect(askCatchUp('planned', 'done', end, '2026-11-25T08:00:00Z')).toBe(true);
		expect(askCatchUp('done', 'done', end, '2026-11-25T08:00:00Z')).toBe(false);
		expect(askCatchUp('missed', 'skipped', end, '2026-11-25T08:00:00Z')).toBe(false);
		expect(askCatchUp('planned', 'done', end, '2026-11-24T06:20:00Z')).toBe(false);
	});

	it('offers nothing when the miss replan added nothing', () => {
		expect(offerOf('run-1', { added: [], moved: 2 }, 'Europe/Berlin')).toBeNull();
	});

	it('names what one tap removes, and what it puts back', () => {
		const one = offerOf('run-1', { added: [add('Core workout (catch-up)', '2026-11-24T17:00:00Z')], moved: 0 }, 'Europe/Berlin');
		expect(one).toEqual({
			runId: 'run-1',
			text: 'A catch-up was added because this was missed. Remove it?',
			detail: 'Removes Core workout (catch-up), Tue 24 Nov 18:00.'
		});
		const two = offerOf('run-1', { added: [add('Run', '2026-11-24T17:00:00Z'), add('Run', '2026-11-26T17:00:00Z')], moved: 1 }, 'Europe/Berlin');
		expect(two?.detail).toBe('Removes Run, Tue 24 Nov 18:00 and Run, Thu 26 Nov 18:00, and puts back 1 moved action.');
	});
});
