import { invoke } from '@tauri-apps/api/core';
import type { ActionView, CategorySlot } from '$lib/ui/contract';
import type { Proposal } from './client';
import { formatYmd, minuteOfDay, shiftDay } from '$lib/ui/format';

/** One verbatim diary entry: the owner's words, when they were logged and how. */
export type DiaryEntryView = { id: string; at: string; text: string; source: 'text' | 'voice' };

/** What the structuring model returned for one day, as the card shows it. */
export type DiaryStructure = {
	summary: string;
	mood: number;
	energy: number;
	highlights: string[];
	food: string | null;
	water: string | null;
	workout: string | null;
	notesForTomorrow: string | null;
	structuredAt: string;
	editedAt?: string | null;
};

/** One session done that day, linked from the card. */
export type DiaryDone = { id: string; title: string; start: string; category: CategorySlot };

/**
 * One diary day: verbatim entries, the structure (null until structured) and the day's done sessions.
 * `stale` is true when an owner-edited card does not cover every entry yet.
 */
export type DiaryDayView = { day: string; entries: DiaryEntryView[]; structure: DiaryStructure | null; done: DiaryDone[]; stale?: boolean };

/** The card as Rust opens it from the sealed `diary_days` row. */
export type RustCard = {
	v: number;
	day: string;
	entry_ids: string[];
	summary: string;
	mood: number;
	energy: number;
	highlights: string[];
	food: string | null;
	water: string | null;
	workout: string | null;
	notes_for_tomorrow: string | null;
	structured_at: string;
	edited_at?: string | null;
};

/** One day as `app_diary_day` returns it: entries, card, `stale` and the locked row count. */
export type RustDay = { day: string; entries: DiaryEntryView[]; card: RustCard | null; stale: boolean; locked: number };

/** What the diary screens show: the Mac answer, or today, the past days and a structuring note. */
export type DiaryLoad = { kind: 'unavailable' } | { kind: 'error'; message: string } | { kind: 'ready'; today: DiaryDayView; past: DiaryDayView[]; note: string | null };

/** The no-key line, as approved in the mockup. */
export const NO_KEY = 'Add your Claude key in Settings to structure this day';

/** True for the error every diary command returns off iPhone. */
export function isUnavailable(error: unknown): boolean {
	return String(error).startsWith('unavailable');
}

/** The done sessions among `actions` whose start falls on local `day` in `zone`, by start time. */
export function doneOn(day: string, actions: ActionView[], zone: string): DiaryDone[] {
	return actions
		.filter((a) => a.status === 'done' && formatYmd(a.start, zone) === day)
		.sort((a, b) => a.start.localeCompare(b.start))
		.map((a) => ({ id: a.id, title: a.title, start: a.start, category: a.category }));
}

/**
 * Maps a Rust day to the card view. A stale card shows as not structured, unless the owner edited it:
 * that card stays on screen with `stale` set, since only a confirmed Redo summary replaces it.
 */
export function toDayView(day: RustDay, done: DiaryDone[]): DiaryDayView {
	const edited = !!day.card?.edited_at;
	const c = day.stale && !edited ? null : day.card;
	const entries = [...day.entries].sort((a, b) => a.at.localeCompare(b.at) || a.id.localeCompare(b.id));
	const structure = c
		? { summary: c.summary, mood: c.mood, energy: c.energy, highlights: c.highlights, food: c.food, water: c.water, workout: c.workout, notesForTomorrow: c.notes_for_tomorrow, structuredAt: c.structured_at, editedAt: c.edited_at ?? null }
		: null;
	return { day: day.day, entries, structure, done, stale: day.stale && !!c };
}

/** The card's editable fields as the edit form holds them; cleared lines are empty strings. */
export type CardDraft = { summary: string; mood: number; energy: number; highlights: string[]; food: string; water: string; workout: string; notesForTomorrow: string };

/** The body of `app_diary_edit_card`, matching `aroh_diary::CardEdit`. */
export type CardEdit = { summary: string; mood: number; energy: number; highlights: string[]; food: string | null; water: string | null; workout: string | null; notes_for_tomorrow: string | null };

/** The edit form's starting values for structure `s`. */
export function draftOf(s: DiaryStructure): CardDraft {
	return { summary: s.summary, mood: s.mood, energy: s.energy, highlights: [...s.highlights], food: s.food ?? '', water: s.water ?? '', workout: s.workout ?? '', notesForTomorrow: s.notesForTomorrow ?? '' };
}

/** The edit payload for `draft`: text trimmed, a cleared line null, blank highlights dropped. */
export function toCardEdit(draft: CardDraft): CardEdit {
	const line = (s: string) => s.trim() || null;
	return {
		summary: draft.summary.trim(),
		mood: draft.mood,
		energy: draft.energy,
		highlights: draft.highlights.map((h) => h.trim()).filter(Boolean),
		food: line(draft.food),
		water: line(draft.water),
		workout: line(draft.workout),
		notes_for_tomorrow: line(draft.notesForTomorrow)
	};
}

const SUMMARY_MAX = 1000;
const LINE_MAX = 300;
const HIGHLIGHTS_MAX = 10;

/** The first card bound `draft` breaks, as a sentence for the form; null when it can be saved. Rust checks the same bounds. */
export function draftError(draft: CardDraft): string | null {
	const e = toCardEdit(draft);
	const scale = (n: number) => Number.isInteger(n) && n >= 1 && n <= 5;
	const len = (s: string | null) => [...(s ?? '')].length;
	if (!e.summary) return 'Write a summary.';
	if (len(e.summary) > SUMMARY_MAX) return 'Keep the summary under 1000 characters.';
	if (!scale(e.mood)) return 'Pick a mood from 1 to 5.';
	if (!scale(e.energy)) return 'Pick an energy from 1 to 5.';
	if (e.highlights.length > HIGHLIGHTS_MAX) return 'At most 10 highlights.';
	if (e.highlights.some((h) => len(h) > LINE_MAX)) return 'Keep each highlight under 300 characters.';
	if ([e.food, e.water, e.workout].some((l) => len(l) > LINE_MAX)) return 'Keep food, water and workout under 300 characters each.';
	if (len(e.notes_for_tomorrow) > SUMMARY_MAX) return 'Keep the notes for tomorrow under 1000 characters.';
	return null;
}

/** The "Your words" toggle label with the entry count (B119). */
export function wordsLabel(count: number): string {
	return `Your words · ${count} ${count === 1 ? 'entry' : 'entries'}`;
}

/** True when Redo summary would replace the owner's own edits, so it asks first. */
export function redoNeedsConfirm(view: DiaryDayView): boolean {
	return !!view.structure?.editedAt;
}

/** A day command's result for the card: the fresh day and its note, or a plain failure. */
export type DayResult = { ok: true; view: DiaryDayView; note: string | null } | { ok: false; message: string };

/** Runs day command `cmd` with `args` and maps its answer, keeping the day's `done` sessions. */
async function dayCall(cmd: string, args: Record<string, unknown>, done: DiaryDone[]): Promise<DayResult> {
	try {
		const answer = await invoke<DayAnswer>(cmd, args);
		return { ok: true, view: toDayView(answer.view, done), note: structureNote(answer.structure) };
	} catch (e) {
		return { ok: false, message: String(e) };
	}
}

/** Redo summary (B118): has the engine structure `day` again even when its card is fresh. */
export function diaryRedo(day: string, done: DiaryDone[]): Promise<DayResult> {
	return dayCall('app_diary_structure', { day, force: true }, done);
}

/** Saves the owner's edit of the card of `day` (B120), resealed on the phone. */
export function diaryEditCard(day: string, draft: CardDraft, done: DiaryDone[]): Promise<DayResult> {
	return dayCall('app_diary_edit_card', { day, edit: toCardEdit(draft) }, done);
}

/** Replaces the words of entry `id` of `day`. */
export function diaryEditEntry(day: string, id: string, text: string, done: DiaryDone[]): Promise<DayResult> {
	return dayCall('app_diary_edit_entry', { day, id, text }, done);
}

/** Moves entry `id` of `day` to local day `to` (B127); both days are structured again. */
export function diaryMoveEntry(day: string, id: string, to: string, done: DiaryDone[]): Promise<DayResult> {
	return dayCall('app_diary_move_entry', { day, id, to }, done);
}

/** Local hour a diary day starts at (B127): words written after midnight but before it belong to the day before. */
export const DIARY_DAY_START_HOUR = 4;

/** The diary day an entry written at `now` in `zone` belongs to: the calendar day, or the one before it before 04:00. */
export function diaryDayOf(now: string, zone: string): string {
	const day = formatYmd(now, zone);
	return minuteOfDay(now, zone) < DIARY_DAY_START_HOUR * 60 ? shiftDay(day, -1) : day;
}

/** Deletes entry `id` of `day`; the last one takes the card with it. */
export function diaryDeleteEntry(day: string, id: string, done: DiaryDone[]): Promise<DayResult> {
	return dayCall('app_diary_delete_entry', { day, id }, done);
}

/** The owner-facing line for a structuring outcome; null when there is nothing to say. */
export function structureNote(outcome: string): string | null {
	if (outcome === 'ok' || outcome === 'empty') return null;
	if (outcome === 'no_key') return NO_KEY;
	const trimmed = outcome.trim();
	const sentence = /[.!?]$/.test(trimmed) ? trimmed : `${trimmed}.`;
	return sentence.endsWith('Your words are saved.') ? sentence : `${sentence} Your words are saved.`;
}

/** The `count` local days before `today`, newest first. */
export function pastDays(today: string, count: number): string[] {
	return Array.from({ length: count }, (_, i) => shiftDay(today, -(i + 1)));
}

type DayAnswer = { view: RustDay; structure: string };

/** Turns a command answer into the load result, with done sessions from `actions`. */
function ready(answer: DayAnswer, past: RustDay[], actions: ActionView[], zone: string): DiaryLoad {
	const today = toDayView(answer.view, doneOn(answer.view.day, actions, zone));
	const days = past.filter((d) => d.entries.length).map((d) => toDayView(d, doneOn(d.day, actions, zone)));
	return { kind: 'ready', today, past: days, note: structureNote(answer.structure) };
}

/** Loads local `today` and the `pastCount` days before it. Plaintext stays in memory only. */
export async function diaryLoad(today: string, pastCount: number, actions: ActionView[], zone: string): Promise<DiaryLoad> {
	try {
		const answer = await invoke<DayAnswer>('app_diary_day', { day: today });
		const past = pastCount ? await invoke<RustDay[]>('app_diary_days', { days: pastDays(today, pastCount) }) : [];
		return ready(answer, past, actions, zone);
	} catch (e) {
		return isUnavailable(e) ? { kind: 'unavailable' } : { kind: 'error', message: String(e) };
	}
}

/** True when `load` shows today written but not structured while a key is set on the engine, so a structure call is due. */
export function needsStructure(load: DiaryLoad): boolean {
	return load.kind === 'ready' && load.today.entries.length > 0 && !load.today.structure && load.note === null;
}

/** Has the engine structure local `today` (one model call there), then returns the fresh load. */
export async function diaryStructure(today: string, pastCount: number, actions: ActionView[], zone: string): Promise<DiaryLoad> {
	try {
		const answer = await invoke<DayAnswer>('app_diary_structure', { day: today });
		const past = pastCount ? await invoke<RustDay[]>('app_diary_days', { days: pastDays(today, pastCount) }) : [];
		return ready(answer, past, actions, zone);
	} catch (e) {
		return isUnavailable(e) ? { kind: 'unavailable' } : { kind: 'error', message: String(e) };
	}
}

/** Seals `text` as a new entry of `today` and structures the day; returns the fresh load. */
export async function diaryAppend(today: string, text: string, voice: boolean, pastCount: number, actions: ActionView[], zone: string): Promise<DiaryLoad> {
	try {
		const answer = await invoke<DayAnswer>('app_diary_append', { day: today, text, voice });
		const past = pastCount ? await invoke<RustDay[]>('app_diary_days', { days: pastDays(today, pastCount) }) : [];
		return ready(answer, past, actions, zone);
	} catch (e) {
		return isUnavailable(e) ? { kind: 'unavailable' } : { kind: 'error', message: String(e) };
	}
}

/** Reads one day for the calendar or the full card, with its note; null off iPhone, on error or with no entries. */
export async function diaryDay(day: string, actions: ActionView[], zone: string): Promise<{ view: DiaryDayView; note: string | null } | null> {
	try {
		const answer = await invoke<DayAnswer>('app_diary_day', { day });
		return answer.view.entries.length ? { view: toDayView(answer.view, doneOn(day, actions, zone)), note: structureNote(answer.structure) } : null;
	} catch {
		return null;
	}
}

/** The agents read toggle: whether it is on, from the engine. */
export async function diaryShareStatus(): Promise<boolean> {
	return (await invoke<{ on: boolean }>('app_diary_status')).on;
}

/** Turns the agents read toggle on or off; resolves with the new state. */
export async function diaryShare(on: boolean): Promise<boolean> {
	return (await invoke<{ on: boolean }>('app_diary_share', { on })).on;
}

/** A diary run's proposal opened on this phone: its real summary and diff, each null when not sealed or not opened. */
export type OpenedProposal = { summary: string | null; diff: unknown };

/** True when proposal `p` came from a diary run: its diff, or a part of it, is sealed. */
export function isSealed(p: Proposal): boolean {
	const diff = p.diff as Record<string, unknown> | null;
	if (!diff || typeof diff !== 'object') return false;
	return 'sealed' in diff || Object.values(diff).some((v) => !!v && typeof v === 'object' && 'sealed' in v);
}

/** Opens the sealed proposals among `proposals` on this phone, by id; ones that fail to open are left out. */
export async function openProposals(proposals: Proposal[]): Promise<Record<string, OpenedProposal>> {
	const out: Record<string, OpenedProposal> = {};
	for (const p of proposals.filter(isSealed)) {
		try {
			out[p.id] = await invoke<OpenedProposal>('app_diary_proposal', { id: p.id });
		} catch {
			continue;
		}
	}
	return out;
}

/** `proposals` with each opened one's summary and diff put in place, for display only. */
export function withOpened(proposals: Proposal[], opened: Record<string, OpenedProposal>): Proposal[] {
	return proposals.map((p) => {
		const o = opened[p.id];
		return o ? { ...p, summary: o.summary ?? p.summary, diff: o.diff ?? p.diff } : p;
	});
}
