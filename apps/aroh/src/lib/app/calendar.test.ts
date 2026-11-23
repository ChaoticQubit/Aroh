import { describe, expect, it, vi } from 'vitest';
import { allDayViews, busyViews, connect, decisionsFrom, IGNORE, KEEP, mergeSuggestions, refreshSuggestions, triageCards, triageKinds, triageOf, type CalendarDeps, type SuggestionDeps } from './calendar';
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
			],
			days: [{ event: 'e5', kind: 'reminder' }]
		},
		refs: [
			{ key: 'e1', event_id: 'standup', calendar_id: 'work', title: 'Standup', recurring: true },
			{ key: 'e5', event_id: 'thanks', calendar_id: 'hol', title: 'Thanksgiving', recurring: false }
		],
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

function block(id: string, title: string, start_at: string, end_at: string, all_day: boolean, day_kind: string | null = null, event_id = id) {
	return { id, calendar_id: 'work', event_id, title, start_at, end_at, all_day, group_name: null, day_kind };
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

		const fresh = deps([event('a', { rrule: 'FREQ=DAILY' }), event('t', { all_day: true, start_at: '2026-11-23T23:00:00Z', end_at: '2026-11-24T23:00:00Z' })]);
		expect(await connect(fresh, 0)).toEqual({ kept: 1, sent: 2, run: 'r1' });
		expect(fresh.calls).toEqual(['read', 'import', 'runs', 'triage']);
		expect(fresh.triage).toHaveBeenCalledWith([
			{ event_id: 'a', calendar_id: 'work', calendar: 'Work', title: 'a', start_at: '2026-11-23T08:00:00Z', end_at: '2026-11-23T08:30:00Z', rrule: 'FREQ=DAILY', all_day: false },
			{ event_id: 't', calendar_id: 'work', calendar: 'Work', title: 't', start_at: '2026-11-23T23:00:00Z', end_at: '2026-11-24T23:00:00Z', rrule: null, all_day: true }
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

	it('shows busy blocks as neutral views with their group, all-day ones only as chips', () => {
		const s = { busy: [block('b1', 'Standup', '2026-11-23T08:00:00Z', '2026-11-23T08:30:00Z', false), block('b2', 'Trip', '2026-11-23T23:00:00Z', '2026-11-24T23:00:00Z', true)] } as unknown as Snapshot;
		expect(busyViews(s)).toEqual([{ id: 'b1', title: 'Standup', start: '2026-11-23T08:00:00Z', end: '2026-11-23T08:30:00Z', group: 'Busy' }]);
		expect(busyViews({} as Snapshot)).toEqual([]);
	});

	it('maps the proposed day kinds of a decide node to event ids', () => {
		expect(triageKinds(waiting)).toEqual({ thanks: 'reminder' });
		expect(triageKinds(undefined)).toEqual({});
		const unknown = { ...waiting, data: { ...waiting.data, sorted: { groups: [], days: [{ event: 'e9', kind: 'busy' }, { event: 'e1', kind: 'holiday' }] } } };
		expect(triageKinds(unknown)).toEqual({});
	});

	it('turns all-day blocks into inclusive local days with their kind and suggestion', () => {
		const s = {
			busy: [
				block('one', 'Birthday', '2026-11-27T23:00:00Z', '2026-11-28T23:00:00Z', true, 'reminder', 'bday'),
				block('three', 'Vacation', '2026-11-26T23:00:00Z', '2026-11-29T23:00:00Z', true, null, 'trip'),
				block('late', 'Fair', '2026-11-30T23:00:00Z', '2026-12-01T22:59:59Z', true, 'busy', 'fair'),
				block('timed', 'Standup', '2026-11-23T08:00:00Z', '2026-11-23T08:30:00Z', false)
			],
			settings: { day_suggestions: JSON.stringify({ trip: 'away', bday: 'busy' }) }
		} as unknown as Snapshot;
		expect(allDayViews(s, 'Europe/Berlin')).toEqual([
			{ id: 'three', title: 'Vacation', start: '2026-11-27', end: '2026-11-29', kind: null, suggested: 'away' },
			{ id: 'one', title: 'Birthday', start: '2026-11-28', end: '2026-11-28', kind: 'reminder', suggested: null },
			{ id: 'late', title: 'Fair', start: '2026-12-01', end: '2026-12-01', kind: 'busy', suggested: null }
		]);
		expect(allDayViews({} as Snapshot, 'UTC')).toEqual([]);
	});

	it('keeps a stored suggestion after the run ended and drops it once a kind is set', () => {
		const s = { busy: [block('three', 'Vacation', '2026-11-26T23:00:00Z', '2026-11-29T23:00:00Z', true, null, 'trip')], settings: { day_suggestions: '{"trip":"away"}' } } as unknown as Snapshot;
		expect(allDayViews(s, 'UTC')[0].suggested).toBe('away');
		s.busy![0].day_kind = 'busy';
		expect(allDayViews(s, 'UTC')[0].suggested).toBeNull();
		s.settings.day_suggestions = 'not json';
		s.busy![0].day_kind = null;
		expect(allDayViews(s, 'UTC')[0].suggested).toBeNull();
	});

	it('merges new picks into the stored suggestions and prunes confirmed or gone events', () => {
		const s = {
			busy: [
				block('b1', 'Vacation', '2026-11-26T23:00:00Z', '2026-11-29T23:00:00Z', true, null, 'trip'),
				block('b2', 'Fair', '2026-11-30T23:00:00Z', '2026-12-01T23:00:00Z', true, 'busy', 'fair'),
				block('b3', 'Move', '2026-12-02T23:00:00Z', '2026-12-03T23:00:00Z', true, null, 'move')
			],
			settings: {}
		} as unknown as Snapshot;
		expect(mergeSuggestions({ trip: 'busy', fair: 'busy', gone: 'away' }, { trip: 'away', move: 'busy' }, s)).toEqual({ trip: 'away', move: 'busy' });
	});

	it('stores the picks of a waiting run and answers a run with no group so it ends', async () => {
		const s = { busy: [block('b1', 'Thanksgiving', '2026-11-25T23:00:00Z', '2026-11-26T23:00:00Z', true, null, 'thanks')], settings: {} } as unknown as Snapshot;
		const calls: string[] = [];
		const empty = { ...waiting, data: { ...waiting.data, sorted: { groups: [], days: [{ event: 'e5', kind: 'reminder' }] } } };
		const make = (w: Waiting): SuggestionDeps => ({
			open: async () => ({ runId: 'r1', waiting: w }),
			save: async (value) => void calls.push(`save ${value}`),
			answer: async (runId, value) => (calls.push(`answer ${runId} ${JSON.stringify(value)}`), { ok: true as const, value: null }),
			finish: async (runId) => void calls.push(`finish ${runId}`)
		});
		expect(await refreshSuggestions(s, make(empty))).toBeNull();
		expect(calls).toEqual(['save {"thanks":"reminder"}', 'answer r1 []', 'finish r1']);
		calls.length = 0;
		const open = await refreshSuggestions(s, make(waiting));
		expect(open?.runId).toBe('r1');
		expect(calls, 'a run with groups waits for the owner').toEqual(['save {"thanks":"reminder"}']);
		calls.length = 0;
		const saved = { ...s, settings: { day_suggestions: '{"thanks":"reminder"}' } } as unknown as Snapshot;
		await refreshSuggestions(saved, { ...make(waiting), open: async () => null });
		expect(calls, 'nothing changed, nothing saved').toEqual([]);
	});
});
