import { invoke } from '@tauri-apps/api/core';
import type { ActionView, CategorySlot } from '$lib/ui/contract';
import type { Proposal } from './client';
import { formatYmd, shiftDay } from '$lib/ui/format';

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
};

/** One session done that day, linked from the card. */
export type DiaryDone = { id: string; title: string; start: string; category: CategorySlot };

/** One diary day: verbatim entries, the structure (null until structured) and the day's done sessions. */
export type DiaryDayView = { day: string; entries: DiaryEntryView[]; structure: DiaryStructure | null; done: DiaryDone[] };

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

/** Maps a Rust day to the card view; a stale or missing card shows as not structured. */
export function toDayView(day: RustDay, done: DiaryDone[]): DiaryDayView {
	const c = day.stale ? null : day.card;
	const entries = [...day.entries].sort((a, b) => a.at.localeCompare(b.at) || a.id.localeCompare(b.id));
	const structure = c
		? { summary: c.summary, mood: c.mood, energy: c.energy, highlights: c.highlights, food: c.food, water: c.water, workout: c.workout, notesForTomorrow: c.notes_for_tomorrow, structuredAt: c.structured_at }
		: null;
	return { day: day.day, entries, structure, done };
}

/** The owner-facing line for a structuring outcome; null when there is nothing to say. */
export function structureNote(outcome: string): string | null {
	if (outcome === 'ok' || outcome === 'empty') return null;
	if (outcome === 'no_key') return NO_KEY;
	return `Not structured: ${outcome}. Your words are saved.`;
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

/** True when `load` shows today written but not structured while a Claude key is set, so a structure call is due. */
export function needsStructure(load: DiaryLoad): boolean {
	return load.kind === 'ready' && load.today.entries.length > 0 && !load.today.structure && load.note === null;
}

/** Structures local `today` from this phone (one model call), then returns the fresh load. */
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
