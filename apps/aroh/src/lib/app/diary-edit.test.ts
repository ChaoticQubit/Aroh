import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { diaryDayOf, diaryDeleteEntry, diaryEditCard, diaryEditEntry, diaryMoveEntry, diaryRedo, draftError, draftOf, needsStructure, redoNeedsConfirm, toCardEdit, toDayView, type DiaryLoad, type RustDay } from './diary';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const card = {
	v: 1,
	day: '2026-11-26',
	entry_ids: ['e1'],
	summary: 'A tired start.',
	mood: 2,
	energy: 3,
	highlights: ['Piano clicked'],
	food: 'Rice bowl',
	water: null,
	workout: null,
	notes_for_tomorrow: null,
	structured_at: '2026-11-26T21:21:00Z'
};
const rust: RustDay = { day: '2026-11-26', entries: [{ id: 'e1', at: '2026-11-26T07:45:00Z', text: 'Slept badly.', source: 'text' }], card, stale: false, locked: 0 };
const done = [{ id: 'a1', title: 'Run', start: '2026-11-26T08:00:00Z', category: 2 as const }];

describe('diary card edits (B118, B120)', () => {
	it('keeps an edited card on screen when a new entry made it stale, and never auto-structures it', () => {
		const edited = { ...rust, stale: true, card: { ...card, edited_at: '2026-11-26T22:00:00Z' } };
		const view = toDayView(edited, []);
		expect(view.structure?.editedAt).toBe('2026-11-26T22:00:00Z');
		expect(view.stale).toBe(true);
		const load: DiaryLoad = { kind: 'ready', today: view, past: [], note: null };
		expect(needsStructure(load)).toBe(false);
		expect(redoNeedsConfirm(view)).toBe(true);
		expect(toDayView({ ...rust, stale: true }, []).structure).toBeNull();
		expect(redoNeedsConfirm(toDayView(rust, []))).toBe(false);
	});

	it('turns a draft into the edit payload: trimmed, cleared lines null, blank highlights dropped', () => {
		const draft = draftOf(toDayView(rust, []).structure!);
		expect(draft).toEqual({ summary: 'A tired start.', mood: 2, energy: 3, highlights: ['Piano clicked'], food: 'Rice bowl', water: '', workout: '', notesForTomorrow: '' });
		const edit = toCardEdit({ ...draft, summary: '  Better  ', highlights: ['Piano clicked', '  ', ' Walk '], food: '  ', notesForTomorrow: ' Sleep. ' });
		expect(edit).toEqual({ summary: 'Better', mood: 2, energy: 3, highlights: ['Piano clicked', 'Walk'], food: null, water: null, workout: null, notes_for_tomorrow: 'Sleep.' });
	});

	it('checks a draft against the card bounds before saving', () => {
		const draft = draftOf(toDayView(rust, []).structure!);
		expect(draftError(draft)).toBeNull();
		expect(draftError({ ...draft, summary: '  ' })).toBe('Write a summary.');
		expect(draftError({ ...draft, mood: 0 })).toBe('Pick a mood from 1 to 5.');
		expect(draftError({ ...draft, energy: 6 })).toBe('Pick an energy from 1 to 5.');
		expect(draftError({ ...draft, highlights: Array(11).fill('h') })).toBe('At most 10 highlights.');
		expect(draftError({ ...draft, highlights: ['h'.repeat(301)] })).toBe('Keep each highlight under 300 characters.');
		expect(draftError({ ...draft, water: 'w'.repeat(301) })).toBe('Keep food, water and workout under 300 characters each.');
		expect(draftError({ ...draft, summary: 's'.repeat(1001) })).toBe('Keep the summary under 1000 characters.');
		expect(draftError({ ...draft, notesForTomorrow: 'n'.repeat(1001) })).toBe('Keep the notes for tomorrow under 1000 characters.');
	});

	it('sends Redo summary with force and shows the outcome note, keeping the done sessions', async () => {
		vi.mocked(invoke).mockReset().mockResolvedValueOnce({ view: rust, structure: 'The engine is busy' });
		const r = await diaryRedo('2026-11-26', done);
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('app_diary_structure', { day: '2026-11-26', force: true });
		expect(r).toMatchObject({ ok: true, note: 'The engine is busy. Your words are saved.' });
		expect(r.ok && r.view.done).toEqual(done);
	});

	it('saves card and entry edits through their commands and reports failures in plain words', async () => {
		vi.mocked(invoke).mockReset().mockResolvedValue({ view: rust, structure: 'ok' });
		const draft = draftOf(toDayView(rust, []).structure!);
		expect(await diaryEditCard('2026-11-26', { ...draft, mood: 5 }, [])).toMatchObject({ ok: true, note: null });
		expect(vi.mocked(invoke)).toHaveBeenLastCalledWith('app_diary_edit_card', { day: '2026-11-26', edit: { ...toCardEdit(draft), mood: 5 } });
		await diaryEditEntry('2026-11-26', 'e1', 'Slept fine.', []);
		expect(vi.mocked(invoke)).toHaveBeenLastCalledWith('app_diary_edit_entry', { day: '2026-11-26', id: 'e1', text: 'Slept fine.' });
		await diaryDeleteEntry('2026-11-26', 'e1', []);
		expect(vi.mocked(invoke)).toHaveBeenLastCalledWith('app_diary_delete_entry', { day: '2026-11-26', id: 'e1' });
		vi.mocked(invoke).mockReset().mockRejectedValueOnce('invalid mood: must be 1 to 5');
		expect(await diaryEditCard('2026-11-26', draft, [])).toEqual({ ok: false, message: 'invalid mood: must be 1 to 5' });
	});
});

describe('diary day boundary and moves (B127)', () => {
	it('b127: an entry written before 04:00 local time belongs to the day before', () => {
		expect(diaryDayOf('2026-11-26T06:30:00Z', 'America/New_York')).toBe('2026-11-25');
		expect(diaryDayOf('2026-11-26T08:59:00Z', 'America/New_York')).toBe('2026-11-25');
		expect(diaryDayOf('2026-11-26T09:00:00Z', 'America/New_York')).toBe('2026-11-26');
		expect(diaryDayOf('2026-11-26T05:00:00Z', 'America/New_York')).toBe('2026-11-25');
		expect(diaryDayOf('2026-11-26T04:59:00Z', 'America/New_York')).toBe('2026-11-25');
		expect(diaryDayOf('2026-11-26T12:00:00Z', 'Asia/Kolkata')).toBe('2026-11-26');
		expect(diaryDayOf('2026-11-25T22:00:00Z', 'Asia/Kolkata')).toBe('2026-11-25');
	});

	it('b127: moves an entry to another day through its command', async () => {
		vi.mocked(invoke).mockReset().mockResolvedValue({ view: rust, structure: 'ok' });
		expect(await diaryMoveEntry('2026-11-26', 'e1', '2026-11-25', [])).toMatchObject({ ok: true });
		expect(vi.mocked(invoke)).toHaveBeenLastCalledWith('app_diary_move_entry', { day: '2026-11-26', id: 'e1', to: '2026-11-25' });
	});
});
