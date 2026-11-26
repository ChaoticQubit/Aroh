import { describe, expect, it } from 'vitest';
import { edgeSpeed, holdMove, HOLD_MS, releaseOf, slotMinute, snapMinutes, zonedIso } from './gesture';

describe('calendar slot tap (B121)', () => {
	it('b121: a tap y offset snaps down to its 15 minute slot inside the shown hours', () => {
		expect(slotMinute(0, 48, 6, 22)).toBe(360);
		expect(slotMinute(11, 48, 6, 22)).toBe(360);
		expect(slotMinute(12, 48, 6, 22)).toBe(375);
		expect(slotMinute(48 * 3 + 47, 48, 6, 22)).toBe(9 * 60 + 45);
		expect(slotMinute(-20, 48, 6, 22)).toBe(360);
		expect(slotMinute(48 * 16, 48, 6, 22)).toBe(22 * 60 - 15);
	});

	it('b121: a day and minute become the instant of that wall time in the zone', () => {
		expect(zonedIso('2026-11-27', 14 * 60 + 15, 'Europe/Berlin')).toBe('2026-11-27T13:15:00.000Z');
		expect(zonedIso('2026-07-01', 9 * 60, 'Europe/Berlin')).toBe('2026-07-01T07:00:00.000Z');
		expect(zonedIso('2026-11-27', 0, 'UTC')).toBe('2026-11-27T00:00:00.000Z');
		expect(zonedIso('2026-11-27', 18 * 60 + 30, 'America/Los_Angeles')).toBe('2026-11-28T02:30:00.000Z');
	});
});

describe('calendar drag (B122)', () => {
	it('b122: a hold starts a drag after about 300 ms; finger jitter below 10 px keeps waiting, more is a scroll', () => {
		expect(HOLD_MS).toBe(300);
		expect(holdMove(6, 6)).toBe('wait');
		expect(holdMove(0, 9)).toBe('wait');
		expect(holdMove(0, 11)).toBe('scroll');
		expect(holdMove(8, 8)).toBe('scroll');
	});

	it('b122: a release is a tap that opens, a drop that saves, or a put back that does nothing', () => {
		expect(releaseOf({ live: false, held: false, travel: 0, delta: 0, days: 0 })).toBe('open');
		expect(releaseOf({ live: true, held: true, travel: 20, delta: 15, days: 0 })).toBe('drop');
		expect(releaseOf({ live: true, held: true, travel: 60, delta: 0, days: -1 })).toBe('drop');
		expect(releaseOf({ live: true, held: true, travel: 4, delta: 0, days: 0 })).toBe('none');
	});

	it('b121: a tap on the selected item still opens it, even with jitter that would snap 15 minutes', () => {
		expect(releaseOf({ live: true, held: false, travel: 0, delta: 0, days: 0 })).toBe('open');
		expect(releaseOf({ live: true, held: false, travel: 7, delta: 15, days: 0 })).toBe('open');
		expect(releaseOf({ live: true, held: false, travel: 30, delta: 30, days: 0 })).toBe('drop');
	});

	it('b122: a drag of dy px snaps to whole 15 minute steps', () => {
		expect(snapMinutes(0, 48)).toBe(0);
		expect(snapMinutes(5, 48)).toBe(0);
		expect(snapMinutes(7, 48)).toBe(15);
		expect(snapMinutes(-36, 48)).toBe(-45);
		expect(snapMinutes(96, 48)).toBe(120);
	});

	it('b122: a finger near the top or bottom edge scrolls that way, faster closer to the edge', () => {
		expect(edgeSpeed(400, 100, 800)).toBe(0);
		expect(edgeSpeed(100, 100, 800)).toBeLessThan(edgeSpeed(130, 100, 800));
		expect(edgeSpeed(130, 100, 800)).toBeLessThan(0);
		expect(edgeSpeed(780, 100, 800)).toBeGreaterThan(0);
		expect(edgeSpeed(800, 100, 800)).toBeGreaterThan(edgeSpeed(780, 100, 800));
		expect(Math.abs(edgeSpeed(0, 100, 800))).toBeLessThanOrEqual(16);
	});
});
