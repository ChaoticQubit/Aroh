import { describe, expect, it } from 'vitest';
import { busyStrip, columns, occurrences, planBlocks, scaleFor, summarize, summaryLine, weeks, type StripBlock } from './strip';

const item = (start: string, minutes: number, rrule: string | null = null, title = 'Read') => ({ title, start, minutes, rrule });

describe('occurrences', () => {
	it('keeps a one-off item as one session in local wall time', () => {
		expect(occurrences(item('2026-11-26T18:00:00', 20), 90)).toEqual([{ date: '2026-11-26', start: 1080, end: 1100 }]);
	});

	it('expands a daily rule with a count', () => {
		const out = occurrences(item('2026-11-26T18:00', 20, 'FREQ=DAILY;COUNT=3'), 90);
		expect(out?.map((o) => o.date)).toEqual(['2026-11-26', '2026-11-27', '2026-11-28']);
	});

	it('expands a weekly rule on chosen days until a date', () => {
		const out = occurrences(item('2026-11-23T07:00', 45, 'FREQ=WEEKLY;BYDAY=MO,WE,FR;UNTIL=20261204T235959Z'), 90);
		expect(out?.map((o) => o.date)).toEqual(['2026-11-23', '2026-11-25', '2026-11-27', '2026-11-30', '2026-12-02', '2026-12-04']);
	});

	it('honours an interval and caps an open rule at the horizon', () => {
		const out = occurrences(item('2026-11-26T18:00', 20, 'FREQ=DAILY;INTERVAL=2'), 10);
		expect(out?.map((o) => o.date)).toEqual(['2026-11-26', '2026-11-28', '2026-11-30', '2026-12-02', '2026-12-04']);
	});

	it('gives null for a rule it cannot expand, so it never draws as one lone block', () => {
		expect(occurrences(item('2026-11-26T18:00', 20, 'FREQ=MONTHLY;BYMONTHDAY=1'), 90)).toBeNull();
		expect(occurrences(item('2026-11-26T18:00', 20, 'FREQ=WEEKLY;BYSETPOS=1'), 90)).toBeNull();
	});

	it('clips a session that runs past midnight to its day', () => {
		expect(occurrences(item('2026-11-26T23:40', 40), 90)).toEqual([{ date: '2026-11-26', start: 1420, end: 1440 }]);
	});
});

describe('planBlocks', () => {
	it('numbers blocks by item and marks only one-off items fixable', () => {
		const { blocks, unexpanded } = planBlocks([item('2026-11-26T18:00', 20), item('2026-11-27T18:00', 20, 'FREQ=DAILY;COUNT=2'), item('2026-11-27T07:00', 20, 'FREQ=YEARLY')], 90);
		expect(blocks.map((b) => [b.item, b.date, b.fixable])).toEqual([
			[0, '2026-11-26', true],
			[1, '2026-11-27', false],
			[1, '2026-11-28', false]
		]);
		expect(unexpanded).toEqual([2]);
	});
});

describe('busyStrip', () => {
	const busy = (id: string, start: string, end: string, extra: Record<string, unknown> = {}) => ({ id, calendar_id: 'c', event_id: id, title: id, start_at: start, end_at: end, all_day: false, group_name: 'Work', day_kind: null, ...extra });
	const action = (id: string, goal: string, start: string, end: string, locked: boolean) => ({ id, goal_id: goal, series_id: null, occurrence: null, title: id, notes: null, start_at: start, end_at: end, status: 'planned' as const, locked, done_at: null });

	it('places timed events in the plan zone and splits one across midnight', () => {
		const out = busyStrip([busy('late', '2026-11-26T22:00:00Z', '2026-11-27T01:00:00Z')], [], 'g', 'Europe/Berlin');
		expect(out.blocks.map((b) => [b.date, b.start, b.end])).toEqual([
			['2026-11-26', 1380, 1440],
			['2026-11-27', 0, 120]
		]);
	});

	it('turns busy and away all-day events into busy days and ignores reminders', () => {
		const out = busyStrip(
			[busy('trip', '2026-12-01T00:00:00Z', '2026-12-03T00:00:00Z', { all_day: true, day_kind: 'away' }), busy('bday', '2026-12-05T00:00:00Z', '2026-12-06T00:00:00Z', { all_day: true, day_kind: 'reminder' })],
			[],
			'g',
			'UTC'
		);
		expect(out.busyDays).toEqual([
			{ date: '2026-12-01', title: 'trip' },
			{ date: '2026-12-02', title: 'trip' }
		]);
		expect(out.blocks).toEqual([]);
	});

	it('shows locked actions as fixed, other goals as busy and skips this goal and finished ones', () => {
		const out = busyStrip(
			[],
			[
				action('gym', 'other', '2026-11-27T07:00:00Z', '2026-11-27T08:00:00Z', true),
				action('piano', 'other', '2026-11-27T19:00:00Z', '2026-11-27T19:30:00Z', false),
				action('mine', 'g', '2026-11-27T18:00:00Z', '2026-11-27T18:20:00Z', false),
				{ ...action('old', 'other', '2026-11-27T10:00:00Z', '2026-11-27T11:00:00Z', false), status: 'done' as const }
			],
			'g',
			'UTC'
		);
		expect(out.blocks.map((b) => [b.title, b.kind])).toEqual([
			['gym', 'fixed'],
			['piano', 'busy']
		]);
	});
});

describe('weeks', () => {
	it('lists every Monday from the first week to the last', () => {
		expect(weeks('2026-11-26', '2026-12-15')).toEqual(['2026-11-23', '2026-11-30', '2026-12-07', '2026-12-14']);
		expect(weeks('2026-11-26', '2026-11-26')).toEqual(['2026-11-23']);
	});
});

const block = (date: string, start: number, end: number, kind: StripBlock['kind'] = 'plan'): StripBlock => ({ id: `${date}-${start}`, kind, title: 't', date, start, end });

describe('scaleFor', () => {
	it('defaults to 06:00 to 22:00 and widens to whole hours around early or late blocks', () => {
		expect(scaleFor([block('2026-11-26', 1080, 1100)])).toEqual({ startHour: 6, endHour: 22 });
		expect(scaleFor([block('2026-11-26', 330, 350), block('2026-11-27', 1350, 1380)])).toEqual({ startHour: 5, endHour: 23 });
	});
});

describe('columns', () => {
	it('places blocks per day with a minimum height and marks past, busy and finish days', () => {
		const cols = columns('2026-11-23', [block('2026-11-26', 1080, 1100), block('2026-11-27', 540, 1020, 'busy')], [{ date: '2026-11-28', title: 'Trip' }], { startHour: 6, endHour: 22 }, { hourPx: 12, minPx: 12 }, '2026-11-26', '2026-11-27');
		expect(cols).toHaveLength(7);
		expect(cols.map((c) => c.past)).toEqual([true, true, true, false, false, false, false]);
		expect(cols[3].blocks[0]).toMatchObject({ y: 144, h: 12 });
		expect(cols[4].blocks[0]).toMatchObject({ y: 36, h: 96 });
		expect(cols[4].finish).toBe(true);
		expect(cols[5].busy).toEqual({ date: '2026-11-28', title: 'Trip' });
	});

	it('keeps a minimum-height block at the bottom edge inside the column', () => {
		const cols = columns('2026-11-23', [block('2026-11-23', 1315, 1320)], [], { startHour: 6, endHour: 22 }, { hourPx: 12, minPx: 12 }, '2026-11-23', '2026-11-23');
		expect(cols[0].blocks[0].y + cols[0].blocks[0].h).toBeLessThanOrEqual(16 * 12);
	});
});

describe('summarize', () => {
	it('reads minutes, cadence, the usual time and the finish from the sessions', () => {
		const days = ['2026-11-26', '2026-11-27', '2026-11-28', '2026-11-29'];
		const blocks = days.map((d, i) => ({ ...block(d, i === 1 ? 1230 : 1080, (i === 1 ? 1230 : 1080) + 20), item: i, fixable: true }));
		const s = summarize(blocks);
		expect(s).toEqual({ minutes: '20 min', cadence: 'daily', time: '18:00', finish: '2026-11-29', sessions: 4 });
		expect(summaryLine(s, '2026-11-23')).toBe('20 min daily · 18:00 · finishes 29 Nov');
	});

	it('says how many days a week and a range of minutes when they vary', () => {
		const blocks = ['2026-11-23', '2026-11-25', '2026-11-27', '2026-11-30', '2026-12-02', '2026-12-04'].map((d, i) => ({ ...block(d, 420, 420 + (i % 2 ? 45 : 30)), item: 0, fixable: false }));
		expect(summaryLine(summarize(blocks), '2026-11-23')).toBe('30-45 min, 3 days a week · 07:00 · finishes 4 Dec');
	});

	it('says one day a week, not one days (gate 4 D6)', () => {
		const blocks = ['2026-11-23', '2026-11-30', '2026-12-07'].map((d) => ({ ...block(d, 720, 750), item: 0, fixable: false }));
		expect(summarize(blocks).cadence).toBe('1 day a week');
	});

	it('adds the year to a finish in another year than today (gate 4 D6)', () => {
		const blocks = ['2027-11-19', '2027-11-26'].map((d) => ({ ...block(d, 720, 750), item: 0, fixable: false }));
		expect(summaryLine(summarize(blocks), '2026-11-23')).toBe('30 min, 1 day a week · 12:00 · finishes 26 Nov 2027');
		expect(summaryLine(summarize(blocks), '2027-01-02')).toBe('30 min, 1 day a week · 12:00 · finishes 26 Nov');
	});

	it('has no finish line without sessions', () => {
		expect(summarize([])).toEqual({ minutes: null, cadence: null, time: null, finish: null, sessions: 0 });
		expect(summaryLine(summarize([]), '2026-11-23')).toBe('No sessions');
	});
});
