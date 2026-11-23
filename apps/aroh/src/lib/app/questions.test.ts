import { describe, expect, it } from 'vitest';
import { sliderRange, typedNumber } from '$lib/ui/format';
import { answerText, FALLBACK_BOUNDS, groupMessage, inputFor, scrollFor, withAnswer, type QuestionGroup } from './questions';

describe('question inputs', () => {
	it('a number question shows its unit and its own slider bounds', () => {
		const q = { id: 'chapters_done', text: 'Chapters finished?', kind: 'number' as const, choices: [], unit: 'chapters', min: 0, max: 30 };
		expect(inputFor(q, '4')).toEqual({ kind: 'number_input', label: 'Your answer', unit: 'chapters', value: 4, step: 1, min: 0, max: 30 });
	});

	it('an old-shape number question (no unit, min or max) gets no unit, never count, and fallback slider bounds', () => {
		const q = { id: 'pages_remaining', text: 'Pages left?', kind: 'number' as const, choices: [] };
		const input = inputFor(q, undefined);
		expect(input).toMatchObject({ kind: 'number_input', unit: '', value: null, ...FALLBACK_BOUNDS });
		expect(inputFor({ ...q, unit: null, min: 10, max: 5 }, '')).toMatchObject({ unit: '', ...FALLBACK_BOUNDS });
	});

	it('a typed number past the fallback bounds is kept and the slider reaches it (build 2: 315 pages became 100)', () => {
		const q = { id: 'pages_read', text: 'Pages read so far?', kind: 'number' as const, choices: [], unit: 'pages' };
		const g: QuestionGroup = { runId: 'r1', prompt: 'Answer these.', questions: [q], values: {}, skipped: [], sending: false, error: null };
		const input = groupMessage(withAnswer(g, 'pages_read', typedNumber(315, 1))).items[0].input;
		expect(input).toMatchObject({ kind: 'number_input', value: 315, ...FALLBACK_BOUNDS });
		if (input.kind !== 'number_input') throw new Error('number question renders a number input');
		expect(sliderRange(input.min, input.max, input.value)).toEqual({ min: 0, max: 315 });
	});

	it('choice and text questions keep their answer', () => {
		expect(inputFor({ id: 'c', text: 'When?', kind: 'choice', choices: ['Morning', 'Evening'] }, 'Evening')).toMatchObject({ kind: 'choice', value: ['Evening'], multi: false });
		expect(inputFor({ id: 't', text: 'Limits?', kind: 'text', choices: [] }, 'none')).toMatchObject({ kind: 'note', value: 'none' });
	});

	it('answer text: numbers as digits, choices joined, blanks empty', () => {
		expect([answerText(30), answerText(['Evening']), answerText(undefined), answerText(null), answerText('  '), answerText(Number.NaN)]).toEqual(['30', 'Evening', '', '', '', '']);
	});
});

describe('chat scroll around a question group (build 1 feedback)', () => {
	it('shows a new group from its first question, stays put while the owner answers, and follows later messages', () => {
		const q = (id: string) => ({ id, text: `${id}?`, kind: 'number' as const, choices: [], unit: 'chapters', min: 0, max: 30 });
		const g: QuestionGroup = { runId: 'r1', prompt: 'Answer these.', questions: [q('a'), q('b')], values: {}, skipped: [], sending: false, error: null };
		const say = { type: 'message' as const, from: 'persona' as const, name: 'Aroh', text: 'Hi' };
		const first = scrollFor([say, groupMessage(g)], null);
		expect(first.to).toBe('group');
		expect(scrollFor([say, groupMessage(withAnswer(g, 'a', 4))], first.seen)).toEqual({ to: 'stay', seen: first.seen });
		expect(scrollFor([say, groupMessage(g, true), say], first.seen).to).toBe('bottom');
		expect(scrollFor([say], null)).toEqual({ to: 'bottom', seen: null });
		expect(scrollFor([], null).to).toBe('bottom');
	});
});
