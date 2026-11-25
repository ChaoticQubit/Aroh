import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { diaryDay, diaryLoad, diaryStructure, doneOn, needsStructure, isSealed, NO_KEY, openProposals, withOpened, isUnavailable, pastDays, structureNote, toDayView, type RustDay } from './diary';
import type { ActionView } from '$lib/ui/contract';
import type { Proposal } from './client';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const rust: RustDay = {
	day: '2026-11-26',
	entries: [
		{ id: 'e2', at: '2026-11-26T13:10:00Z', text: 'Lunch was rice.', source: 'voice' },
		{ id: 'e1', at: '2026-11-26T07:45:00Z', text: 'Slept badly.', source: 'text' }
	],
	card: {
		v: 1,
		day: '2026-11-26',
		entry_ids: ['e1', 'e2'],
		summary: 'A tired start, a steady day.',
		mood: 4,
		energy: 2,
		highlights: ['Piano clicked'],
		food: 'Rice bowl',
		water: null,
		workout: 'Core, 30 min',
		notes_for_tomorrow: 'Stretch.',
		structured_at: '2026-11-26T21:21:00Z'
	},
	stale: false,
	locked: 0
};

const action = (id: string, start: string, status: ActionView['status']): ActionView => ({ id, title: `Session ${id}`, start, end: start, status, locked: false, category: 2, goalTitle: 'Six pack' });

describe('diary view model', () => {
	it('maps the Rust day to the card view, entries oldest first', () => {
		const v = toDayView(rust, []);
		expect(v.entries.map((e) => e.id)).toEqual(['e1', 'e2']);
		expect(v.structure).toMatchObject({ summary: 'A tired start, a steady day.', mood: 4, energy: 2, notesForTomorrow: 'Stretch.', water: null, structuredAt: '2026-11-26T21:21:00Z' });
	});

	it('shows no structure while the card is stale or missing', () => {
		expect(toDayView({ ...rust, stale: true }, []).structure).toBeNull();
		expect(toDayView({ ...rust, card: null, stale: true }, []).structure).toBeNull();
	});

	it('links only the done sessions of that local day', () => {
		const actions = [action('a1', '2026-11-26T07:00:00Z', 'done'), action('a2', '2026-11-26T20:00:00Z', 'planned'), action('a3', '2026-11-25T23:30:00Z', 'done')];
		const done = doneOn('2026-11-26', actions, 'Europe/Berlin');
		expect(done.map((d) => d.id)).toEqual(['a3', 'a1']);
		expect(done[1]).toEqual({ id: 'a1', title: 'Session a1', start: '2026-11-26T07:00:00Z', category: 2 });
		expect(doneOn('2026-11-26', actions, 'UTC').map((d) => d.id)).toEqual(['a1']);
	});

	it('words a structuring outcome for the owner', () => {
		expect(structureNote('ok')).toBeNull();
		expect(structureNote('empty')).toBeNull();
		expect(structureNote('no_key')).toBe('Add your Claude key in Settings to structure this day');
		expect(structureNote('the model answered 401')).toBe('Not structured: the model answered 401. Your words are saved.');
	});

	it('knows the Mac answer', () => {
		expect(isUnavailable('unavailable: the diary is on iPhone only')).toBe(true);
		expect(isUnavailable('locked: waiting')).toBe(false);
	});

	it('lists the past days before today, newest first', () => {
		expect(pastDays('2026-11-26', 3)).toEqual(['2026-11-25', '2026-11-24', '2026-11-23']);
	});

	it('loads today and past days and keeps plaintext out of storage', async () => {
		const call = vi.mocked(invoke);
		call.mockReset();
		call.mockImplementation(async (cmd: string) => {
			if (cmd === 'app_diary_day') return { view: rust, structure: 'no_key' };
			if (cmd === 'app_diary_days') return [{ ...rust, day: '2026-11-25', entries: [], card: null }];
			throw new Error(cmd);
		});
		const setItem = vi.spyOn(Storage.prototype, 'setItem');
		const log = vi.spyOn(console, 'log');
		const loaded = await diaryLoad('2026-11-26', 1, [], 'UTC');
		expect(loaded.kind).toBe('ready');
		if (loaded.kind !== 'ready') return;
		expect(loaded.today.entries).toHaveLength(2);
		expect(loaded.note).toBe('Add your Claude key in Settings to structure this day');
		expect(loaded.past).toEqual([]);
		expect(call).toHaveBeenCalledWith('app_diary_days', { days: ['2026-11-25'] });
		expect(setItem).not.toHaveBeenCalled();
		expect(log).not.toHaveBeenCalled();
	});

	it('reads one day with its note, or null when it has no entries', async () => {
		vi.mocked(invoke).mockReset().mockResolvedValueOnce({ view: rust, structure: 'no_key' }).mockResolvedValueOnce({ view: { ...rust, entries: [] }, structure: 'ok' });
		expect((await diaryDay('2026-11-26', [], 'UTC'))?.note).toBe(NO_KEY);
		expect(await diaryDay('2026-11-26', [], 'UTC')).toBeNull();
	});

	it('opens only sealed proposals and shows them in place', async () => {
		const plain = { id: 'p1', summary: 'Moved a session.', diff: { changes: [] } } as unknown as Proposal;
		const whole = { id: 'p2', summary: 'Changed your plan from your recent days.', diff: { sealed: { key_version: 1 } } } as unknown as Proposal;
		const parts = { id: 'p3', summary: 'Changed the card.', diff: { op: 'card', request: { sealed: { key_version: 1 } } } } as unknown as Proposal;
		expect([plain, whole, parts].map(isSealed)).toEqual([false, true, true]);
		vi.mocked(invoke).mockReset().mockImplementation(async (_cmd: string, args?: unknown) => {
			if ((args as { id: string }).id === 'p3') throw new Error('locked');
			return { summary: 'Moved the run after a bad night.', diff: { changes: [{ op: 'move', title: 'Run' }] } };
		});
		const opened = await openProposals([plain, whole, parts]);
		expect(Object.keys(opened)).toEqual(['p2']);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('app_diary_proposal', { id: 'p2' });
		const shown = withOpened([plain, whole, parts], opened);
		expect(shown[0]).toBe(plain);
		expect(shown[1]).toMatchObject({ summary: 'Moved the run after a bad night.', diff: { changes: [{ op: 'move', title: 'Run' }] } });
		expect(shown[2]).toBe(parts);
	});

	it('structures an unstructured today once a key is set', async () => {
		const stale = { ...rust, stale: true };
		vi.mocked(invoke).mockReset().mockResolvedValueOnce({ view: stale, structure: 'ok' }).mockResolvedValueOnce({ view: rust, structure: 'ok' });
		const before = await diaryLoad('2026-11-26', 0, [], 'UTC');
		expect(needsStructure(before)).toBe(true);
		const after = await diaryStructure('2026-11-26', 0, [], 'UTC');
		expect(vi.mocked(invoke)).toHaveBeenLastCalledWith('app_diary_structure', { day: '2026-11-26' });
		expect(needsStructure(after)).toBe(false);
		vi.mocked(invoke).mockReset().mockResolvedValueOnce({ view: stale, structure: 'no_key' });
		expect(needsStructure(await diaryLoad('2026-11-26', 0, [], 'UTC'))).toBe(false);
	});

	it('reports the Mac as unavailable', async () => {
		vi.mocked(invoke).mockReset().mockRejectedValue('unavailable: the diary is on iPhone only');
		expect((await diaryLoad('2026-11-26', 7, [], 'UTC')).kind).toBe('unavailable');
	});
});
