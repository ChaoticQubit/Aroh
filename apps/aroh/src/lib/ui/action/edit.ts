import type { Block, Card, CardLog, Exercise, SetLog } from './card';

/** Most rows of one block and sets of one exercise, as the server checks them. */
export const MAX_ROWS = 30;
export const MAX_SETS = 12;

/** One editable cell of a workout row. */
export type WorkoutField = 'name' | 'sets' | 'reps' | 'weight' | 'rest';

/** One editable cell of a steps or checklist row. */
export type ListField = 'text' | 'minutes';


/** Deep copy of `card` that also works on Svelte state proxies. */
const copy = (card: Card): Card => JSON.parse(JSON.stringify(card)) as Card;

function bounded(text: string, max: number): number | null {
	const n = Number(text.replace(',', '.'));
	return Number.isFinite(n) && n >= 0 && n <= max ? n : null;
}

/**
 * Sets cell `field` of row `row` in workout block `block` to the typed `text`. `sets` changes the
 * set count (copying the last set), `reps` and `weight` change every set, `rest` is seconds.
 * Text that is not a number in range leaves the card unchanged. Returns the new card.
 */
export function setWorkoutCell(card: Card, block: number, row: number, field: WorkoutField, text: string): Card {
	const next = copy(card);
	const b = next.blocks[block];
	console.assert(b?.kind === 'workout', 'a workout cell is in a workout block');
	if (b.kind !== 'workout') return card;
	const e = b.exercises[row];
	if (field === 'name') {
		e.name = text.slice(0, 2000);
		return next;
	}
	if (field === 'weight' && text.trim() === '') {
		e.sets = e.sets.map((s) => ({ ...s, weight: null }));
		return next;
	}
	if (text.trim() === '') return card;
	const n = bounded(text, field === 'rest' ? 1800 : field === 'weight' ? 2000 : field === 'sets' ? MAX_SETS : 3600);
	if (n === null) return card;
	if (field === 'sets') {
		const count = Math.max(1, Math.round(n));
		const last = e.sets[e.sets.length - 1];
		e.sets = Array.from({ length: count }, (_, i) => e.sets[i] ?? { ...last });
	} else if (field === 'reps') e.sets = e.sets.map((s) => ({ ...s, reps: Math.round(n) }));
	else if (field === 'weight') e.sets = e.sets.map((s) => ({ ...s, weight: n }));
	else e.rest_s = Math.round(n);
	console.assert(e.sets.length >= 1 && e.sets.length <= MAX_SETS, 'sets stay in bounds');
	return next;
}

/** Sets cell `field` of row `row` in steps or checklist block `block` to `text`. Returns the new card. */
export function setListCell(card: Card, block: number, row: number, field: ListField, text: string): Card {
	const next = copy(card);
	const b = next.blocks[block];
	if (b.kind === 'workout') return card;
	const item = b.items[row];
	if (field === 'text') item.text = text.slice(0, 2000);
	else if (b.kind === 'steps') {
		const n = text.trim() === '' ? null : bounded(text, 1440);
		(item as { minutes: number | null }).minutes = n === null ? null : Math.round(n);
	}
	return next;
}

/** Sets the why or a block title. Returns the new card. */
export function setText(card: Card, field: 'why' | { title: number }, text: string): Card {
	const next = copy(card);
	const value = text.slice(0, 2000);
	if (field === 'why') next.why = value;
	else {
		const b = next.blocks[field.title];
		if (b.kind !== 'workout') b.title = value;
	}
	return next;
}

/** Adds an empty row (exercise, step or item) at the end of block `block`, up to the row limit. */
export function addRow(card: Card, block: number): Card {
	const next = copy(card);
	const b = next.blocks[block];
	const rows = b.kind === 'workout' ? b.exercises.length : b.items.length;
	if (rows >= MAX_ROWS) return card;
	if (b.kind === 'workout') b.exercises.push({ name: 'New exercise', unit: b.exercises[0]?.unit ?? 'kg', rest_s: 90, cue: null, sets: [{ reps: 10, weight: null }] });
	else if (b.kind === 'steps') b.items.push({ text: '', minutes: null });
	else b.items.push({ text: '' });
	return next;
}

function rowsOf(b: Block): unknown[] {
	return b.kind === 'workout' ? b.exercises : b.items;
}

/** Removes row `row` of block `block`; a workout keeps at least one exercise. */
export function removeRow(card: Card, block: number, row: number): Card {
	const next = copy(card);
	const rows = rowsOf(next.blocks[block]);
	if (row < 0 || row >= rows.length || (next.blocks[block].kind === 'workout' && rows.length === 1)) return card;
	rows.splice(row, 1);
	return next;
}

/** Moves row `from` of block `block` to index `to` (clamped). Returns the new card. */
export function moveRow(card: Card, block: number, from: number, to: number): Card {
	const next = copy(card);
	const rows = rowsOf(next.blocks[block]);
	const target = Math.max(0, Math.min(rows.length - 1, to));
	if (from === target || from < 0 || from >= rows.length) return card;
	const [moved] = rows.splice(from, 1);
	rows.splice(target, 0, moved);
	console.assert(rows.length === rowsOf(card.blocks[block]).length, 'a move keeps every row');
	return next;
}

/**
 * Ticks set `set` of exercise `row` in block `block` as planned, or unticks it when ticked.
 * Returns the new log.
 */
export function toggleSet(card: Card, log: CardLog, block: number, row: number, set: number): CardLog {
	const b = card.blocks[block];
	if (b.kind !== 'workout') return log;
	const plan = b.exercises[row].sets[set];
	const rest = log.sets.filter((s) => !(s.block === block && s.row === row && s.set === set));
	const hit = log.sets.find((s) => s.block === block && s.row === row && s.set === set && s.done);
	return { ...log, sets: hit ? rest : [...rest, { block, row, set, reps: plan.reps, weight: plan.weight, done: true }] };
}

/** Changes the logged reps or load of one set by `reps` and `weight` and marks it done. */
export function stepSet(card: Card, log: CardLog, block: number, row: number, set: number, reps: number, weight: number): CardLog {
	const b = card.blocks[block];
	if (b.kind !== 'workout') return log;
	const plan = b.exercises[row].sets[set];
	const hit = log.sets.find((s) => s.block === block && s.row === row && s.set === set) ?? { block, row, set, reps: plan.reps, weight: plan.weight, done: true };
	const next: SetLog = {
		...hit,
		done: true,
		reps: Math.max(0, Math.min(3600, hit.reps + reps)),
		weight: hit.weight === null && weight === 0 ? null : Math.max(0, Math.min(2000, (hit.weight ?? 0) + weight))
	};
	return { ...log, sets: [...log.sets.filter((s) => s !== hit), next] };
}

/** Ticks or unticks checklist row `row` of block `block`. */
export function toggleItem(log: CardLog, block: number, row: number): CardLog {
	const on = log.checked.some((c) => c.block === block && c.row === row);
	return { ...log, checked: on ? log.checked.filter((c) => !(c.block === block && c.row === row)) : [...log.checked, { block, row }] };
}

type Address = { block: number; row: number; set: number; exercise: Exercise };

function sets(card: Card): Address[] {
	const all: Address[] = [];
	card.blocks.forEach((b, block) => {
		if (b.kind === 'workout') b.exercises.forEach((exercise, row) => exercise.sets.forEach((_, set) => all.push({ block, row, set, exercise })));
	});
	return all;
}

const WORDS: Record<string, number> = { one: 1, two: 2, three: 3, four: 4, five: 5, six: 6, seven: 7, eight: 8, nine: 9, ten: 10, eleven: 11, twelve: 12, fifteen: 15, twenty: 20 };

function number(word: string): number | null {
	const n = Number(word.replace(',', '.'));
	return Number.isFinite(n) ? n : (WORDS[word] ?? null);
}

/**
 * Applies one spoken or typed log line to `log`: "last set 7 reps", "set 2 8 reps at 60",
 * "done" (ticks the next set as planned), "8 reps 62.5 kg" (the next set). "last set" is the
 * newest ticked set, else the next one. Returns the new log, or null when the line is not a log.
 */
export function voiceLog(card: Card, log: CardLog, text: string): CardLog | null {
	const words = text.toLowerCase().replace(/[^a-z0-9.,\s]/g, ' ').split(/\s+/).filter(Boolean);
	const all = sets(card);
	if (!all.length || !words.length) return null;
	const done = (a: Address) => log.sets.some((s) => s.block === a.block && s.row === a.row && s.set === a.set && s.done);
	const next = all.find((a) => !done(a)) ?? null;
	let target: Address | null = next;
	const lastAt = words.indexOf('last');
	if (lastAt >= 0 && words[lastAt + 1] === 'set') {
		const newest = log.sets.filter((s) => s.done).at(-1);
		target = (newest && all.find((a) => a.block === newest.block && a.row === newest.row && a.set === newest.set)) ?? next;
	}
	const setAt = words.findIndex((w, i) => w === 'set' && words[i - 1] !== 'last');
	if (setAt >= 0 && number(words[setAt + 1] ?? '') !== null) {
		const n = number(words[setAt + 1]) as number;
		const row = next ?? all[0];
		target = all.find((a) => a.block === row.block && a.row === row.row && a.set === n - 1) ?? null;
	}
	const before = (unit: string[]) => {
		const i = words.findIndex((w) => unit.includes(w));
		return i > 0 ? number(words[i - 1]) : null;
	};
	const reps = before(['reps', 'rep', 'times', 'seconds', 'sec']);
	const atIdx = words.indexOf('at');
	const weight = before(['kg', 'kilos', 'lb', 'lbs', 'pounds']) ?? (atIdx >= 0 ? number(words[atIdx + 1] ?? '') : null);
	const isDone = words.some((w) => ['done', 'finished', 'tick'].includes(w));
	if (!target || (reps === null && weight === null && !isDone)) return null;
	const plan = target.exercise.sets[target.set];
	const entry: SetLog = { block: target.block, row: target.row, set: target.set, reps: reps ?? plan.reps, weight: weight ?? plan.weight, done: true };
	if (entry.reps < 0 || entry.reps > 3600 || (entry.weight !== null && (entry.weight < 0 || entry.weight > 2000))) return null;
	const rest = log.sets.filter((s) => !(s.block === entry.block && s.row === entry.row && s.set === entry.set));
	return { ...log, sets: [...rest, entry] };
}

/**
 * What keeps `card` from saving, in the owner's words, or null when it saves: a full card
 * needs a why and every exercise a name. Checked before each save as you type.
 */
export function cardProblem(card: Card): string | null {
	if (card.depth === 'full' && !card.why.trim()) return 'Add a why to save.';
	for (const b of card.blocks) {
		if (b.kind === 'workout' && b.exercises.some((e) => !e.name.trim())) return 'Name every exercise to save.';
	}
	return null;
}

