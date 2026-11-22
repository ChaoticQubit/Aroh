import { describe, expect, it, vi } from 'vitest';
import { busyViews, connect, decisionsFrom, IGNORE, KEEP, triageCards, triageOf, type CalendarDeps } from './calendar';
import type { CalendarImport, DeviceEvent, Snapshot } from './client';
import type { RunSummary, Waiting } from './engine';

const waiting: Waiting = {
	node: 'decide',
	ask: 'question',
	prompt: 'Keep these groups as busy time?',
	data: {
		sorted: {
			groups: [
				{ name: 'Work meetings', kind: 'work', keep: true, reason: 'Meetings.', events: ['e1', 'e2', 'e3', 'e4'] },
				{ name: 'Noise', kind: 'noise', keep: false, reason: 'Holidays.', events: ['e5'] }
			]
		},
		events: [
			{ key: 'e1', calendar: 'Work', title: 'Standup', start: '2026-11-23T09:00:00', minutes: 30, rrule: 'FREQ=DAILY' },
			{ key: 'e2', calendar: 'Work', title: 'Review', start: '2026-11-23T13:00:00', minutes: 60, rrule: null },
			{ key: 'e3', calendar: 'Work', title: '1:1', start: '2026-11-24T10:00:00', minutes: 30, rrule: null },
			{ key: 'e4', calendar: 'Work', title: 'Planning', start: '2026-11-25T10:00:00', minutes: 30, rrule: null },
			{ key: 'e5', calendar: 'Holidays', title: 'Thanksgiving', start: '2026-11-26T00:00:00', minutes: 60, rrule: null }
		]
	}
};

function event(id: string, extra: Partial<DeviceEvent> = {}): DeviceEvent {
	return { calendar_id: 'work', calendar: 'Work', event_id: id, title: id, start_at: '2026-11-23T08:00:00Z', end_at: '2026-11-23T08:30:00Z', all_day: false, rrule: null, ...extra };
}

function deps(unmatched: DeviceEvent[], runs: RunSummary[] = []): CalendarDeps & { calls: string[] } {
	const calls: string[] = [];
	const report: CalendarImport = { kept: 1, ignored: 0, removed: 0, unmatched };
	return {
		calls,
		read: vi.fn(async () => (calls.push('read'), [event('a'), event('b')])),
		importEvents: vi.fn(async () => (calls.push('import'), report)),
		runs: vi.fn(async () => (calls.push('runs'), { ok: true as const, value: runs })),
		triage: vi.fn(async () => (calls.push('triage'), { ok: true as const, value: { goal_id: null, run_id: 'r1' } }))
	};
}

describe('calendar triage', () => {
	it('reads the groups of a decide node and nothing else', () => {
		expect(triageOf(waiting)?.groups.length).toBe(2);
		expect(triageOf({ ...waiting, node: 'approve' })).toBeNull();
	});

	it('makes one question card per group with count, sample titles and the suggestion preselected', () => {
		const cards = triageCards(waiting);
		expect(cards.map((c) => c.id)).toEqual(['Work meetings', 'Noise']);
		const spec = cards[0].spec;
		if (spec.kind !== 'question_card' || spec.input.kind !== 'choice') throw new Error('not a choice card');
		expect(spec.question).toBe('Work meetings: 4 events, like Standup, Review and 1:1. Keep them as busy time?');
		expect(spec.input.options).toEqual([KEEP, IGNORE]);
		expect(spec.input.value).toEqual([KEEP]);
		const noise = cards[1].spec;
		expect(noise.kind === 'question_card' && noise.input.kind === 'choice' && noise.input.value).toEqual([IGNORE]);
		expect(noise.kind === 'question_card' && noise.question).toBe('Noise: 1 event, like Thanksgiving. Keep them as busy time?');
	});

	it('posts decisions only once every group is answered, untouched cards taking the suggestion', () => {
		expect(decisionsFrom(waiting, { 'Work meetings': [IGNORE] })).toBeNull();
		expect(decisionsFrom(waiting, { 'Work meetings': [IGNORE], Noise: undefined })).toEqual([
			{ group: 'Work meetings', keep: false },
			{ group: 'Noise', keep: false }
		]);
		expect(decisionsFrom(waiting, { 'Work meetings': undefined, Noise: [KEEP] })).toEqual([
			{ group: 'Work meetings', keep: true },
			{ group: 'Noise', keep: true }
		]);
	});

	it('imports, then triages only unmatched events and only with no open triage', async () => {
		const none = deps([]);
		expect(await connect(none, 0)).toEqual({ kept: 1, sent: 0, run: null });
		expect(none.calls).toEqual(['read', 'import']);

		const fresh = deps([event('a', { rrule: 'FREQ=DAILY' }), event('t', { all_day: true })]);
		expect(await connect(fresh, 0)).toEqual({ kept: 1, sent: 1, run: 'r1' });
		expect(fresh.calls).toEqual(['read', 'import', 'runs', 'triage']);
		expect(fresh.triage).toHaveBeenCalledWith([
			{ event_id: 'a', calendar_id: 'work', calendar: 'Work', title: 'a', start_at: '2026-11-23T08:00:00Z', end_at: '2026-11-23T08:30:00Z', rrule: 'FREQ=DAILY' }
		]);
		const read = fresh.read as ReturnType<typeof vi.fn>;
		expect(read.mock.calls[0]).toEqual(['1969-12-31T00:00:00.000Z', '1970-01-15T00:00:00.000Z']);

		const open: RunSummary = { run_id: 'r0', goal_id: null, workflow: 'calendar-triage', state: 'waiting_human', created_at: '', updated_at: '' };
		const busy = deps([event('a')], [open]);
		expect(await connect(busy, 0)).toEqual({ kept: 1, sent: 0, run: 'r0' });
		expect(busy.calls).not.toContain('triage');
		const done = deps([event('a')], [{ ...open, state: 'done' }]);
		expect((await connect(done, 0)).run).toBe('r1');
	});

	it('shows busy blocks as neutral views with their group', () => {
		const s = { busy: [{ id: 'b1', calendar_id: 'work', event_id: 'e', title: 'Standup', start_at: '2026-11-23T08:00:00Z', end_at: '2026-11-23T08:30:00Z', all_day: false, group_name: null }] } as unknown as Snapshot;
		expect(busyViews(s)).toEqual([{ id: 'b1', title: 'Standup', start: '2026-11-23T08:00:00Z', end: '2026-11-23T08:30:00Z', group: 'Busy' }]);
		expect(busyViews({} as Snapshot)).toEqual([]);
	});
});
