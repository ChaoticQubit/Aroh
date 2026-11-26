import { describe, expect, it } from 'vitest';
import { sliderRange, typedNumber } from '$lib/ui/format';
import { answerText, FALLBACK_BOUNDS, groupMessage, inputFor, knownValues, OTHER, scrollFor, submissionOf, withAnswer, type QuestionGroup } from './questions';
import type { Question } from './engine';

describe('question inputs', () => {
	it('a number question shows its unit and its own slider bounds', () => {
		const q = { id: 'chapters_done', text: 'Chapters finished?', kind: 'number' as const, choices: [], unit: 'chapters', min: 0, max: 30 };
		expect(inputFor(q, '4')).toEqual({ kind: 'number_input', label: 'Your answer', unit: 'chapters', value: 4, step: 1, min: 0, max: 30, clamp: true });
	});

	it('an old-shape number question (no unit, min or max) gets no unit, never count, and fallback slider bounds', () => {
		const q = { id: 'pages_remaining', text: 'Pages left?', kind: 'number' as const, choices: [] };
		const input = inputFor(q, undefined);
		expect(input).toMatchObject({ kind: 'number_input', unit: '', value: null, ...FALLBACK_BOUNDS });
		expect(inputFor({ ...q, unit: null, min: 10, max: 5 }, '')).toMatchObject({ unit: '', ...FALLBACK_BOUNDS });
	});

	it('a typed number past the fallback bounds is kept and the slider reaches it (build 2: 315 pages became 100)', () => {
		const q = { id: 'pages_read', text: 'Pages read so far?', kind: 'number' as const, choices: [], unit: 'pages' };
		const g: QuestionGroup = { runId: 'r1', node: 'questions', prompt: 'Answer these.', questions: [q], values: {}, skipped: [], sending: false, error: null };
		const input = groupMessage(withAnswer(g, 'pages_read', typedNumber(315, 1))).items[0].input;
		expect(input).toMatchObject({ kind: 'number_input', value: 315, ...FALLBACK_BOUNDS });
		if (input.kind !== 'number_input') throw new Error('number question renders a number input');
		expect(sliderRange(input.min, input.max, input.value)).toEqual({ min: 0, max: 315 });
	});

	it('choice and text questions keep their answer', () => {
		expect(inputFor({ id: 'c', text: 'When?', kind: 'choice', choices: ['Morning', 'Evening'] }, 'Evening')).toMatchObject({ kind: 'choice', value: ['Evening'], multi: false, options: ['Morning', 'Evening', OTHER] });
		expect(inputFor({ id: 't', text: 'Limits?', kind: 'text', choices: [] }, 'none')).toMatchObject({ kind: 'note', value: 'none' });
	});

	it('answer text: numbers as digits, choices joined, blanks empty', () => {
		expect([answerText(30), answerText(['Evening']), answerText(undefined), answerText(null), answerText('  '), answerText(Number.NaN)]).toEqual(['30', 'Evening', '', '', '', '']);
	});
});

describe('still true checks', () => {
	it('a question the profile answers shows its known value and source, and counts as answered with it', () => {
		const known = { fact_id: 'f1', value: '30 min', source: 'your answer on Run a 5k' };
		const questions = [{ id: 'minutes', text: 'Minutes a day?', kind: 'text' as const, choices: [], known }, { id: 'old', text: 'Old?', kind: 'text' as const, choices: [] }];
		const g: QuestionGroup = { runId: 'r1', node: 'questions', prompt: 'Answer these.', questions, values: knownValues(questions), skipped: [], sending: false, error: null };
		const m = groupMessage(g);
		expect(m.items[0].known).toEqual({ value: '30 min', source: 'From your answer on Run a 5k' });
		expect(m.items[1].known).toBeUndefined();
		expect(m.missing).toEqual(['old']);
		expect(m.status).toBe('1 of 2 answered. 1 still open.');
		const cleared = groupMessage(withAnswer(g, 'minutes', ''));
		expect(cleared.missing).toEqual(['minutes', 'old']);
		expect(cleared.status).toBe('0 of 2 answered. 2 still open.');
	});

	it('a blank or absent remembered value seeds no answer', () => {
		const q = (id: string, known: Question['known']): Question => ({ id, text: id, kind: 'text', choices: [], known });
		expect(knownValues([q('a', { fact_id: 'f', value: '  ', source: 's' }), q('b', null), q('c', undefined), q('d', { fact_id: 'g', value: 'Mon', source: 's' })])).toEqual({ d: 'Mon' });
	});
});

describe('chat scroll around a question group (build 1 feedback)', () => {
	it('shows a new group from its first question, stays put while the owner answers, and follows later messages', () => {
		const q = (id: string) => ({ id, text: `${id}?`, kind: 'number' as const, choices: [], unit: 'chapters', min: 0, max: 30 });
		const g: QuestionGroup = { runId: 'r1', node: 'questions', prompt: 'Answer these.', questions: [q('a'), q('b')], values: {}, skipped: [], sending: false, error: null };
		const say = { type: 'message' as const, from: 'persona' as const, name: 'Aroh', text: 'Hi' };
		const first = scrollFor([say, groupMessage(g)], null);
		expect(first.to).toBe('group');
		expect(scrollFor([say, groupMessage(withAnswer(g, 'a', 4))], first.seen)).toEqual({ to: 'stay', seen: first.seen });
		expect(scrollFor([say, groupMessage(g, true), say], first.seen).to).toBe('bottom');
		expect(scrollFor([say], null)).toEqual({ to: 'bottom', seen: null });
		expect(scrollFor([], null).to).toBe('bottom');
	});
});

describe('typing a free-text answer', () => {
	it('keeps a trailing space while typing and trims only on submit', () => {
		const g: QuestionGroup = { runId: 'r', node: 'n', prompt: 'p', questions: [{ id: 'why', kind: 'text', text: 'Why?' } as never], values: {}, skipped: [], sending: false, error: null };
		const typed = withAnswer(g, 'why', 'lose weight ');
		expect(typed.values.why).toBe('lose weight ');
		expect(submissionOf(withAnswer(typed, 'why', ' lose weight  '))).toEqual([{ id: 'why', answer: 'lose weight' }]);
		expect(withAnswer(typed, 'why', '   ').values.why).toBeUndefined();
	});
});


const group = (questions: Question[], node = 'questions'): QuestionGroup => ({ runId: 'r', node, prompt: 'p', questions, values: knownValues(questions), skipped: [], sending: false, error: null });

describe('bounded number questions (B106)', () => {
	const days: Question = { id: 'reading_days', text: 'How many days a week?', kind: 'number', choices: [], unit: 'days per week', min: 1, max: 7 };

	it('b106: a number question clamps its answer to its own min and max, so 8 days a week becomes 7', () => {
		const g = group([days]);
		expect(withAnswer(g, 'reading_days', 8).values.reading_days).toBe('7');
		expect(withAnswer(g, 'reading_days', 0).values.reading_days).toBe('1');
		expect(withAnswer(g, 'reading_days', '5').values.reading_days).toBe('5');
		const input = groupMessage(withAnswer(g, 'reading_days', 9)).items[0].input;
		expect(input).toMatchObject({ kind: 'number_input', value: 7, min: 1, max: 7, clamp: true });
	});

	it('b106: a remembered number past the bounds starts clamped; a remembered text value stays', () => {
		const known = (value: string) => ({ fact_id: '', value, source: 'goal description' });
		expect(knownValues([{ ...days, known: known('10') }, { ...days, id: 'm', known: known('30 min') }])).toEqual({ reading_days: '7', m: '30 min' });
	});

	it('b106: a question without its own bounds keeps the fallback slider unclamped (build 2: 315 pages)', () => {
		const pages: Question = { id: 'pages_read', text: 'Pages read?', kind: 'number', choices: [], unit: 'pages' };
		const g = withAnswer(group([pages]), 'pages_read', 315);
		expect(g.values.pages_read).toBe('315');
		expect(groupMessage(g).items[0].input).not.toHaveProperty('clamp');
	});
});

describe('choice questions with Other and several picks (B107)', () => {
	const when: Question = { id: 'when', text: 'When can you read?', kind: 'choice', choices: ['Morning', 'Evening, after dinner', 'other'], multi: true };
	const format: Question = { id: 'format', text: 'Which format?', kind: 'choice', choices: ['Print', 'E-reader'] };

	it('b107: every choice question offers Other once, after its own choices; a model-sent Other is dropped', () => {
		const m = groupMessage(group([when, format]));
		expect(m.items[0].input).toMatchObject({ kind: 'choice', options: ['Morning', 'Evening, after dinner', OTHER], multi: true, value: [] });
		expect(m.items[1].input).toMatchObject({ kind: 'choice', options: ['Print', 'E-reader', OTHER], multi: false });
		expect(m.items[0].other).toBeNull();
	});

	it('b107: several picks and an Other text post as one joined answer, commas inside a choice kept', () => {
		let g = withAnswer(group([when]), 'when', { picked: ['Morning', 'Evening, after dinner'], other: null });
		expect(g.values.when).toBe('Morning, Evening, after dinner');
		expect(groupMessage(g).items[0].input).toMatchObject({ value: ['Morning', 'Evening, after dinner'] });
		g = withAnswer(g, 'when', { picked: ['Morning'], other: 'On the train, weekdays' });
		const m = groupMessage(g);
		expect(m.items[0].input).toMatchObject({ value: ['Morning', OTHER] });
		expect(m.items[0].other).toBe('On the train, weekdays');
		expect(submissionOf(g)).toEqual([{ id: 'when', answer: 'Morning, On the train, weekdays' }]);
	});

	it('b107: Other picked with no text yet keeps the field open and the question unanswered', () => {
		const g = withAnswer(group([format]), 'format', { picked: [], other: '' });
		const m = groupMessage(g);
		expect(m.items[0].input).toMatchObject({ value: [OTHER] });
		expect(m.items[0].other).toBe('');
		expect(m.missing).toEqual(['format']);
		expect(submissionOf(withAnswer(g, 'format', { picked: [], other: ' Audiobook ' }))).toEqual([{ id: 'format', answer: 'Audiobook' }]);
	});

	it('b107: a single-pick question keeps one answer: Other text replaces a pick', () => {
		const g = withAnswer(group([format]), 'format', { picked: ['Print'], other: 'Audiobook' });
		expect(g.values.format).toBe('Audiobook');
		expect(groupMessage(g).items[0].input).toMatchObject({ value: [OTHER] });
	});

	it('b105: a choice known from the goal description shows picked; a free-text known value shows as Other', () => {
		const known = (value: string) => ({ fact_id: '', value, source: 'goal description' });
		const m = groupMessage(group([{ ...format, known: known('E-reader') }, { ...when, id: 'w2', known: known('Morning, Evening, after dinner') }, { ...format, id: 'f2', known: known('Kindle app') }]));
		expect(m.items[0].input).toMatchObject({ value: ['E-reader'] });
		expect(m.items[0].known).toEqual({ value: 'E-reader', source: 'From goal description' });
		expect(m.items[1].input).toMatchObject({ value: ['Morning', 'Evening, after dinner'] });
		expect(m.items[2].input).toMatchObject({ value: [OTHER] });
		expect(m.items[2].other).toBe('Kindle app');
	});

	it('b107: typed composer text on a choice question becomes its Other answer', () => {
		const g = withAnswer(group([format]), 'format', 'Library audiobook');
		expect(g.values.format).toBe('Library audiobook');
		expect(groupMessage(g).items[0].other).toBe('Library audiobook');
	});

	it('b107: the limits card keeps its fixed choices, since its answer is read by the first word', () => {
		const pick: Question = { id: 'limit_choice', text: 'What should change?', kind: 'choice', choices: ['Move the date to Dec 15', 'Raise the daily minutes'] };
		const item = groupMessage(group([pick], 'limit_choice')).items[0];
		expect(item.input).toMatchObject({ options: ['Move the date to Dec 15', 'Raise the daily minutes'] });
		expect(item.other).toBeUndefined();
	});
});
