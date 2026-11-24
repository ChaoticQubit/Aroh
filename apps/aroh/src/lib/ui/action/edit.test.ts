import { describe, expect, it } from 'vitest';
import type { Card, CardLog } from './card';
import { dragSteps } from './rows';
import { addRow, cardProblem, moveRow, removeRow, setListCell, setText, setWorkoutCell, stepSet, toggleItem, toggleSet, voiceLog } from './edit';

const card = (): Card => ({
	v: 1,
	depth: 'full',
	why: 'Legs.',
	outline: null,
	blocks: [
		{
			kind: 'workout',
			exercises: [
				{ name: 'Squat', unit: 'kg', rest_s: 120, cue: null, sets: [{ reps: 8, weight: 60 }, { reps: 8, weight: 60 }] },
				{ name: 'Lunge', unit: 'kg', rest_s: 90, cue: null, sets: [{ reps: 10, weight: 20 }] }
			]
		},
		{ kind: 'checklist', title: 'Bring', items: [{ text: 'Belt' }, { text: 'Chalk' }] }
	]
});
const empty = (): CardLog => ({ sets: [], checked: [], effort: null, note: null });

describe('card edits', () => {
	it('sets workout cells and ignores bad numbers', () => {
		let c = setWorkoutCell(card(), 0, 0, 'sets', '4');
		const w = () => (c.blocks[0].kind === 'workout' ? c.blocks[0].exercises[0] : null)!;
		expect(w().sets).toHaveLength(4);
		c = setWorkoutCell(c, 0, 0, 'reps', '5');
		c = setWorkoutCell(c, 0, 0, 'weight', '62,5');
		expect(w().sets.every((s) => s.reps === 5 && s.weight === 62.5)).toBe(true);
		expect(setWorkoutCell(c, 0, 0, 'reps', 'abc')).toBe(c);
		expect(setWorkoutCell(c, 0, 0, 'sets', '99')).toBe(c);
		expect(setWorkoutCell(c, 0, 0, 'sets', ' ')).toBe(c);
		expect(setWorkoutCell(c, 0, 0, 'rest', '')).toBe(c);
		c = setWorkoutCell(c, 0, 0, 'weight', '');
		expect(w().sets[0].weight).toBeNull();
		c = setWorkoutCell(c, 0, 0, 'name', 'Front squat');
		expect(w().name).toBe('Front squat');
		expect(card().blocks[0]).toEqual(card().blocks[0]);
	});

	it('edits text, adds, removes and moves rows', () => {
		let c = setText(card(), 'why', 'Strong legs.');
		c = setText(c, { title: 1 }, 'Pack');
		expect([c.why, c.blocks[1].kind === 'checklist' && c.blocks[1].title]).toEqual(['Strong legs.', 'Pack']);
		c = addRow(c, 1);
		c = setListCell(c, 1, 2, 'text', 'Shoes');
		c = moveRow(c, 1, 2, 0);
		expect(c.blocks[1].kind === 'checklist' && c.blocks[1].items.map((i) => i.text)).toEqual(['Shoes', 'Belt', 'Chalk']);
		c = removeRow(c, 1, 1);
		expect(c.blocks[1].kind === 'checklist' && c.blocks[1].items).toHaveLength(2);
		c = removeRow(removeRow(c, 0, 0), 0, 0);
		expect(c.blocks[0].kind === 'workout' && c.blocks[0].exercises).toHaveLength(1);
	});

	it('logs sets by tap and steps', () => {
		let log = toggleSet(card(), empty(), 0, 0, 1);
		expect(log.sets).toEqual([{ block: 0, row: 0, set: 1, reps: 8, weight: 60, done: true }]);
		log = stepSet(card(), log, 0, 0, 1, -1, 2.5);
		expect(log.sets).toEqual([{ block: 0, row: 0, set: 1, reps: 7, weight: 62.5, done: true }]);
		expect(toggleSet(card(), log, 0, 0, 1).sets).toEqual([]);
		log = toggleItem(toggleItem(log, 1, 0), 1, 1);
		expect(toggleItem(log, 1, 0).checked).toEqual([{ block: 1, row: 1 }]);
	});

	it('reads spoken log lines', () => {
		let log = voiceLog(card(), empty(), 'done')!;
		expect(log.sets).toEqual([{ block: 0, row: 0, set: 0, reps: 8, weight: 60, done: true }]);
		log = voiceLog(card(), log, 'Last set seven reps')!;
		expect(log.sets).toEqual([{ block: 0, row: 0, set: 0, reps: 7, weight: 60, done: true }]);
		log = voiceLog(card(), log, '8 reps at 62.5')!;
		expect(log.sets.at(-1)).toEqual({ block: 0, row: 0, set: 1, reps: 8, weight: 62.5, done: true });
		log = voiceLog(card(), log, 'set 1 9 reps')!;
		expect(log.sets.find((s) => s.row === 1)).toEqual({ block: 0, row: 1, set: 0, reps: 9, weight: 20, done: true });
		expect(voiceLog(card(), log, 'swap lunges for split squats')).toBeNull();
		expect(voiceLog(card(), empty(), '9999 reps')).toBeNull();
	});

	it('names what keeps a card from saving and rounds row drags', () => {
		expect(cardProblem(card())).toBeNull();
		expect(cardProblem({ ...card(), why: ' ' })).toBe('Add a why to save.');
		expect(cardProblem(setWorkoutCell(card(), 0, 1, 'name', ''))).toBe('Name every exercise to save.');
		expect([dragSteps(0, 60), dragSteps(35, 60), dragSteps(-100, 60), dragSteps(29, 60)]).toEqual([0, 1, -2, 0]);
	});
});
