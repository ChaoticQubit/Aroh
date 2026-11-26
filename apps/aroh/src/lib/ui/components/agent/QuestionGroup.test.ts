import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import QuestionGroup from './QuestionGroup.svelte';
import { groupMessage, knownValues, withAnswer, type QuestionGroup as Group } from '$lib/app/questions';
import type { Question } from '$lib/app/engine';

const questions: Question[] = [
	{ id: 'format', text: 'Which format?', kind: 'choice', choices: ['Print', 'E-reader'] },
	{ id: 'reading_days', text: 'How many days a week?', kind: 'number', choices: [], unit: 'days per week', min: 1, max: 7 }
];
const group: Group = { runId: 'r', node: 'questions', prompt: 'Answer these.', questions, values: knownValues(questions), skipped: [], sending: false, error: null };

describe('QuestionGroup answers (B106, B107)', () => {
	it('b107: a choice question shows Other, and the Other text field only once Other is picked', () => {
		const closed = render(QuestionGroup, { props: { group: groupMessage(group) } }).body;
		expect(closed).toMatch(/>\s*Other\s*</);
		expect(closed).not.toContain('Other answer to question 1');
		const open = render(QuestionGroup, { props: { group: groupMessage(withAnswer(group, 'format', { picked: [], other: 'Audiobook' })) } }).body;
		expect(open).toContain('aria-label="Other answer to question 1"');
		expect(open).toContain('value="Audiobook"');
	});

	it('b106: a days-a-week slider ends at 7 even after 9 was typed', () => {
		const body = render(QuestionGroup, { props: { group: groupMessage(withAnswer(group, 'reading_days', 9)) } }).body;
		expect(body).toMatch(/max="7"/);
		expect(body).not.toMatch(/max="9"/);
		expect(body).toMatch(/value="7"/);
	});
});
