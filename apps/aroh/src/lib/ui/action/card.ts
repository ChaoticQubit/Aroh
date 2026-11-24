/** One planned set: reps and load, `weight` null for bodyweight. */
export type SetPlan = { reps: number; weight: number | null };

/** One exercise row of a workout block. */
export type Exercise = { name: string; unit: 'kg' | 'lb' | null; rest_s: number; cue: string | null; sets: SetPlan[] };

/** A structured block of a card: a workout table, ordered steps, or a checklist. */
export type Block =
	| { kind: 'workout'; exercises: Exercise[] }
	| { kind: 'steps'; title: string; items: { text: string; minutes: number | null }[] }
	| { kind: 'checklist'; title: string; items: { text: string }[] };

/** The card of one action (Q71): full in the next 2 weeks, a one-line outline later. */
export type Card = { v: 1; depth: 'full' | 'outline'; why: string; blocks: Block[]; outline: string | null };

/** Header facts of an action card: title, goal, category hue 1-6, local date and start, length, week and coach. */
export type CardMeta = { title: string; goal: string; hue: number; date: string; start: string; minutes: number; week: string; coach: string };

/** One logged set, addressed by block, row and set index. */
export type SetLog = { block: number; row: number; set: number; reps: number; weight: number | null; done: boolean };

/** The session log of one action: sets, ticked checklist rows, effort and a note. */
export type CardLog = { sets: SetLog[]; checked: { block: number; row: number }[]; effort: 'easy' | 'ok' | 'hard' | null; note: string | null };

/** One line of a card diff. */
export type DiffLine = { op: 'add' | 'remove' | 'change'; text: string };

/**
 * Formats sets as `3 x 8 @ 60 kg`, or `8, 8, 7 @ 60 kg` when reps differ.
 * The load is the first set's; bodyweight sets (`weight` null) show no load.
 * Input: at least one set. Output: the display line.
 */
export function setsLine(sets: SetPlan[], unit: Exercise['unit']): string {
	console.assert(sets.length > 0, 'an exercise has at least one set');
	const reps = sets.map((s) => s.reps);
	const same = reps.every((r) => r === reps[0]);
	const count = same ? `${sets.length} x ${reps[0]}` : reps.join(', ');
	const weight = sets[0].weight;
	return weight === null ? count : `${count} @ ${weight}${unit ? ` ${unit}` : ''}`;
}

/** Formats rest seconds: whole minutes from 60 s as `2 min`, anything else as `90 s`. */
export function restText(seconds: number): string {
	console.assert(seconds >= 0, 'rest is never negative');
	return seconds >= 60 && seconds % 60 === 0 ? `${seconds / 60} min` : `${seconds} s`;
}

/**
 * State of set `set` of `exercise` (row `row` of block `block`) in `log`:
 * `planned` when not ticked, `done` when ticked as planned, `edited` when ticked with other numbers.
 */
export function setState(exercise: Exercise, log: CardLog, block: number, row: number, set: number): 'planned' | 'done' | 'edited' {
	console.assert(set >= 0 && set < exercise.sets.length, 'set index inside the exercise');
	const hit = log.sets.find((s) => s.block === block && s.row === row && s.set === set && s.done);
	if (!hit) return 'planned';
	const plan = exercise.sets[set];
	return hit.reps === plan.reps && hit.weight === plan.weight ? 'done' : 'edited';
}

/**
 * Session progress: ticked sets plus ticked checklist rows over all sets and checklist rows.
 * Steps are not counted. Returns `{ done, total }` with `done <= total`.
 */
export function progress(card: Card, log: CardLog): { done: number; total: number } {
	let done = 0;
	let total = 0;
	card.blocks.forEach((b, block) => {
		if (b.kind === 'workout') {
			b.exercises.forEach((e, row) => {
				total += e.sets.length;
				done += e.sets.filter((_, set) => setState(e, log, block, row, set) !== 'planned').length;
			});
		} else if (b.kind === 'checklist') {
			total += b.items.length;
			done += b.items.filter((_, row) => log.checked.some((c) => c.block === block && c.row === row)).length;
		}
	});
	console.assert(done <= total, 'progress never exceeds the plan');
	return { done, total };
}

/**
 * Diff of two versions of a card and its length in minutes, for the chat's applied-edit card.
 * Exercises match by name within the same block; steps and checklist rows match by text.
 * Output order: length, then per block the changed, removed and added lines.
 */
export function cardDiff(before: { card: Card; minutes: number }, after: { card: Card; minutes: number }): DiffLine[] {
	const out: DiffLine[] = [];
	if (before.minutes !== after.minutes) out.push({ op: 'change', text: `Length ${before.minutes} min to ${after.minutes} min` });
	const blocks = Math.max(before.card.blocks.length, after.card.blocks.length);
	for (let i = 0; i < blocks; i++) {
		const a = before.card.blocks[i];
		const b = after.card.blocks[i];
		const old = a ? rows(a) : [];
		const now = b ? rows(b) : [];
		for (const r of old) {
			const match = now.find((n) => n.key === r.key);
			if (match) out.push(...r.changes(match));
		}
		for (const r of old) if (!now.some((n) => n.key === r.key)) out.push({ op: 'remove', text: r.text });
		for (const n of now) if (!old.some((r) => r.key === n.key)) out.push({ op: 'add', text: n.text });
	}
	return out;
}

type Row = { key: string; text: string; changes: (to: Row) => DiffLine[]; ex?: Exercise };

/** Rows of one block as keyed, printable entries with a field-level comparison. */
function rows(b: Block): Row[] {
	if (b.kind === 'workout') {
		return b.exercises.map((e) => ({
			key: `w:${e.name}`,
			text: `${e.name} ${setsLine(e.sets, e.unit)}`,
			ex: e,
			changes: (to: Row) => {
				const t = to.ex!;
				const lines: DiffLine[] = [];
				const from = setsLine(e.sets, e.unit);
				const next = setsLine(t.sets, t.unit);
				if (from !== next) lines.push({ op: 'change', text: `${e.name} ${from} to ${next}` });
				if (e.rest_s !== t.rest_s) lines.push({ op: 'change', text: `${e.name} rest ${restText(e.rest_s)} to ${restText(t.rest_s)}` });
				return lines;
			}
		}));
	}
	return b.items.map((item) => ({ key: `${b.kind}:${item.text}`, text: item.text, changes: () => [] }));
}
