import { describe, expect, it } from 'vitest';
import { sixpackActions, sixpackEntries, mockNow } from './sixpack';

describe('sixpack sample data', () => {
	it('has exactly one missed action, the Wednesday strength session', () => {
		const missed = sixpackActions.filter((a) => a.status === 'missed');
		expect(missed.map((a) => a.start)).toEqual(['2026-11-25T18:00:00Z']);
	});
	it('marks actions before now as done and after now as planned', () => {
		for (const a of sixpackActions) {
			if (a.status === 'missed') continue;
			expect(a.status).toBe(Date.parse(a.end) <= Date.parse(mockNow) ? 'done' : 'planned');
		}
	});
	it('ends the weight series at 82.4 kg on the mock date', () => {
		const last = sixpackEntries.weight.at(-1);
		expect(sixpackEntries.weight).toHaveLength(21);
		expect(last).toEqual({ at: '2026-11-26T07:00:00.000Z', value: 82.4 });
	});
});
