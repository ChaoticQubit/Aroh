import { describe, expect, it } from 'vitest';
import { daysUntil, formatDuration, formatNumber, formatTime, formatYmd, minuteOfDay, shiftDay, shiftMonth, mondayOf, daysBetween, formatDate, snap } from './format';

describe('format', () => {
	it('formats numbers with units', () => {
		expect(formatNumber(82.4, 'kg')).toBe('82.4 kg');
		expect(formatNumber(2400, 'kcal')).toBe('2,400 kcal');
		expect(formatNumber(3)).toBe('3');
		expect(formatNumber(4, 'count')).toBe('4');
	});
	it('formats durations', () => {
		expect(formatDuration(45)).toBe('45 min');
		expect(formatDuration(90)).toBe('1 h 30 min');
		expect(formatDuration(120)).toBe('2 h');
	});
	it('formats a time in a zone', () => {
		expect(formatTime('2026-11-21T18:00:00Z', 'UTC')).toBe('18:00');
	});
	it('gives the local date and minute of day in a zone', () => {
		expect(formatYmd('2026-11-21T23:30:00Z', 'Asia/Kolkata')).toBe('2026-11-22');
		expect(minuteOfDay('2026-11-21T18:15:00Z', 'UTC')).toBe(18 * 60 + 15);
		expect(minuteOfDay('2026-11-21T00:05:00Z', 'UTC')).toBe(5);
		expect(shiftDay('2026-11-30', 2)).toBe('2026-12-02');
		expect(shiftDay('2026-03-01', -1)).toBe('2026-02-28');
		expect(shiftMonth('2026-11-26', 1)).toBe('2026-12-01');
		expect(shiftMonth('2027-01-15', -2)).toBe('2026-11-01');
		expect(mondayOf('2026-11-26')).toBe('2026-11-23');
		expect(mondayOf('2026-11-23')).toBe('2026-11-23');
		expect(daysBetween('2026-11-23', '2026-11-20')).toBe(-3);
		expect(formatDate('2026-11-26', { weekday: 'short' })).toBe('Thu');
	});
	it('snaps slider and step input to bounds and step precision', () => {
		expect(snap(82.40000001, 0.1, 30, 250)).toBe(82.4);
		expect(snap(82.4 + 0.1, 0.1, 30, 250)).toBe(82.5);
		expect(snap(10, 0.1, 30, 250)).toBe(30);
		expect(snap(300, 1, 0, 16)).toBe(16);
		expect(snap(5.6, 1, 0, 16)).toBe(6);
		expect(() => snap(Number.NaN, 1)).toThrow(RangeError);
		expect(() => snap(1, 0)).toThrow(RangeError);
	});
	it('counts whole days to a deadline', () => {
		expect(daysUntil('2027-02-01T00:00:00Z', '2026-11-21T00:00:00Z')).toBe(72);
	});
});
