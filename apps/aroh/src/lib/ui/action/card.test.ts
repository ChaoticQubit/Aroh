import { describe, expect, it } from 'vitest';
import { cardDiff, progress, restText, setState, setsLine, type Card, type CardLog } from './card';

const legDay: Card = {
	v: 1,
	depth: 'full',
	why: 'Build leg strength.',
	outline: null,
	blocks: [
		{
			kind: 'workout',
			exercises: [
				{ name: 'Back squat', unit: 'kg', rest_s: 120, cue: null, sets: [{ reps: 8, weight: 60 }, { reps: 8, weight: 60 }, { reps: 8, weight: 60 }] },
				{ name: 'Walking lunges', unit: 'kg', rest_s: 90, cue: null, sets: [{ reps: 10, weight: 20 }, { reps: 10, weight: 20 }] }
			]
		},
		{ kind: 'checklist', title: 'Before you start', items: [{ text: 'Water bottle' }, { text: 'Belt' }] }
	]
};

const log: CardLog = {
	sets: [
		{ block: 0, row: 0, set: 0, reps: 8, weight: 60, done: true },
		{ block: 0, row: 0, set: 1, reps: 7, weight: 60, done: true }
	],
	checked: [{ block: 1, row: 0 }],
	effort: null,
	note: null
};

describe('setsLine', () => {
	it('collapses equal sets', () => {
		expect(setsLine([{ reps: 8, weight: 60 }, { reps: 8, weight: 60 }, { reps: 8, weight: 60 }], 'kg')).toBe('3 x 8 @ 60 kg');
	});
	it('lists reps when they differ', () => {
		expect(setsLine([{ reps: 8, weight: 60 }, { reps: 8, weight: 60 }, { reps: 7, weight: 60 }], 'kg')).toBe('8, 8, 7 @ 60 kg');
	});
	it('drops the load for bodyweight', () => {
		expect(setsLine([{ reps: 12, weight: null }, { reps: 12, weight: null }], null)).toBe('2 x 12');
	});
});

describe('restText', () => {
	it('shows whole minutes as minutes, else seconds', () => {
		expect(restText(120)).toBe('2 min');
		expect(restText(90)).toBe('90 s');
	});
});

describe('setState', () => {
	it('tells planned, done and edited apart', () => {
		const ex = (legDay.blocks[0] as Extract<Card['blocks'][number], { kind: 'workout' }>).exercises[0];
		expect(setState(ex, log, 0, 0, 0)).toBe('done');
		expect(setState(ex, log, 0, 0, 1)).toBe('edited');
		expect(setState(ex, log, 0, 0, 2)).toBe('planned');
	});
});

describe('progress', () => {
	it('counts sets and checklist items', () => {
		expect(progress(legDay, log)).toEqual({ done: 3, total: 7 });
	});
});

describe('cardDiff', () => {
	it('names swaps, drops, changes and the new length', () => {
		const after: Card = structuredClone(legDay);
		const w = after.blocks[0] as Extract<Card['blocks'][number], { kind: 'workout' }>;
		w.exercises[1] = { name: 'Bulgarian split squat', unit: 'kg', rest_s: 90, cue: null, sets: [{ reps: 8, weight: 16 }, { reps: 8, weight: 16 }] };
		w.exercises[0] = { ...w.exercises[0], rest_s: 90 };
		expect(cardDiff({ card: legDay, minutes: 60 }, { card: after, minutes: 45 })).toEqual([
			{ op: 'change', text: 'Length 60 min to 45 min' },
			{ op: 'change', text: 'Back squat rest 2 min to 90 s' },
			{ op: 'remove', text: 'Walking lunges 2 x 10 @ 20 kg' },
			{ op: 'add', text: 'Bulgarian split squat 2 x 8 @ 16 kg' }
		]);
	});
	it('is empty for the same card', () => {
		expect(cardDiff({ card: legDay, minutes: 60 }, { card: legDay, minutes: 60 })).toEqual([]);
	});
});
