import { describe, expect, it } from 'vitest';
import { daysUntil, deadlineState, dayLabels, formatDuration, formatNumber, formatTime, formatYmd, minuteOfDay, shiftDay, shiftMonth, mondayOf, daysBetween, formatDate, sliderRange, snap, stepNumber, typedNumber } from './format';

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
	it('a typed number is never cut to the slider bounds; the slider grows to hold it (build 2: 315 became 100)', () => {
		expect(typedNumber(315, 1)).toBe(315);
		expect(typedNumber(82.40000001, 0.1)).toBe(82.4);
		expect(sliderRange(0, 100, 315)).toEqual({ min: 0, max: 315 });
		expect(sliderRange(10, 100, 4)).toEqual({ min: 4, max: 100 });
		expect(sliderRange(0, 100, 40)).toEqual({ min: 0, max: 100 });
		expect(sliderRange(0, 100, null)).toEqual({ min: 0, max: 100 });
		expect(sliderRange(0, undefined, 5)).toBeNull();
		expect(sliderRange(undefined, 30, 5)).toBeNull();
		expect(() => sliderRange(10, 5, null)).toThrow(RangeError);
	});
	it('step buttons go past the top of the range and stop at its floor', () => {
		expect(stepNumber(315, 1, 1, 0)).toBe(316);
		expect(stepNumber(100, 1, 1, 0)).toBe(101);
		expect(stepNumber(0, -1, 1, 0)).toBe(0);
		expect(stepNumber(null, 1, 1, 0)).toBe(1);
		expect(stepNumber(null, -0.1, 0.1, undefined)).toBe(-0.1);
		expect(stepNumber(-3, -1, 1, 0)).toBe(-3);
	});
	it('counts whole days to a deadline', () => {
		expect(daysUntil('2027-02-01T00:00:00Z', '2026-11-21T00:00:00Z')).toBe(72);
	});
	it('reads Due today on the due date until its end, and passed only from the next day (B83)', () => {
		const end = '2026-11-25T04:59:59.000Z';
		expect(deadlineState(end, '2026-11-24T05:00:00Z')).toEqual({ state: 'today' });
		expect(deadlineState(end, '2026-11-25T04:58:00Z')).toEqual({ state: 'today' });
		expect(deadlineState(end, '2026-11-25T05:00:00Z')).toEqual({ state: 'passed' });
		expect(deadlineState(end, '2026-11-23T17:00:00Z')).toEqual({ state: 'left', days: 1 });
		expect(deadlineState(end, '2026-11-14T12:00:00Z')).toEqual({ state: 'left', days: 10 });
	});
});

describe('dayLabels (gate 4 D2)', () => {
	it('labels nothing when every action is today, so the Today list shows times alone', () => {
		expect(dayLabels(['2026-11-23T11:00:00Z', '2026-11-23T15:00:00Z'], 'Europe/Berlin', '2026-11-23')).toEqual([null, null]);
	});

	it('labels every action with its day when the list spans days, today as Today', () => {
		expect(dayLabels(['2026-11-23T11:00:00Z', '2026-11-24T11:00:00Z', '2026-11-23T23:30:00Z'], 'Europe/Berlin', '2026-11-23')).toEqual(['Today', 'Tue 24', 'Tue 24']);
	});
});
