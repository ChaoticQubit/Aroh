import type { ChatMessage, ComponentSpec } from '$lib/ui/contract';
import type { Question } from './engine';

/** Slider bounds for a number question that names none, as in runs started before units existed. */
export const FALLBACK_BOUNDS = { min: 0, max: 100 } as const;

/** One open question set of a run: the questions, the owner's answers by id, skips, and the submit state. */
export type QuestionGroup = {
	runId: string;
	node: string;
	prompt: string;
	questions: Question[];
	values: Record<string, string>;
	skipped: string[];
	sending: boolean;
	error: string | null;
};

/** An answer input's value as the text posted: choices joined, numbers as digits, blanks empty. */
export function answerText(value: unknown): string {
	if (Array.isArray(value)) return value.map(String).join(', ').trim();
	if (value === null || value === undefined) return '';
	if (typeof value === 'number' && !Number.isFinite(value)) return '';
	return String(value).trim();
}

/** The slider bounds of a number question: its own when both are numbers with min below max, else the fallback. */
export function boundsOf(q: Question): { min: number; max: number } {
	const ok = typeof q.min === 'number' && typeof q.max === 'number' && Number.isFinite(q.min) && Number.isFinite(q.max) && q.min < q.max;
	return ok ? { min: q.min as number, max: q.max as number } : { ...FALLBACK_BOUNDS };
}

/** The answer input of one question holding `value`: number with its unit and slider, choice chips, or free text. */
export function inputFor(q: Question, value: string | undefined): ComponentSpec {
	if (q.kind === 'number') {
		const n = value === undefined || value === '' ? null : Number(value);
		return { kind: 'number_input', label: 'Your answer', unit: q.unit ?? '', value: n !== null && Number.isFinite(n) ? n : null, step: 1, ...boundsOf(q) };
	}
	if (q.kind === 'choice') return { kind: 'choice', label: 'Your answer', options: q.choices, multi: false, value: value ? [value] : [] };
	return { kind: 'note', label: 'Your answer', placeholder: 'Type your answer', value: value ?? '' };
}

/** The ids of the questions with neither an answer nor a skip, in question order. */
export function missingOf(g: QuestionGroup): string[] {
	return g.questions.filter((q) => !g.values[q.id] && !g.skipped.includes(q.id)).map((q) => q.id);
}

/** The owner-facing count line: answered, skipped and still open. */
export function statusOf(g: QuestionGroup): string {
	const missing = missingOf(g).length;
	const skipped = g.skipped.length;
	const answered = g.questions.length - missing - skipped;
	const parts = [`${answered} of ${g.questions.length} answered`];
	if (skipped) parts.push(`${skipped} skipped`);
	const checks = g.questions.filter((q) => q.known && missingOf(g).includes(q.id)).length;
	if (!missing) return `${parts.join(', ')}. Ready to send.`;
	return checks ? `${parts.join(', ')}. ${missing} still open, ${checks} to confirm.` : `${parts.join(', ')}. ${missing} still open.`;
}

/** The `questions` answer to post: every question in order, a skip as `answer: null`. Null while any is open. */
export function submissionOf(g: QuestionGroup): { id: string; answer: string | null }[] | null {
	if (missingOf(g).length) return null;
	return g.questions.map((q) => ({ id: q.id, answer: g.skipped.includes(q.id) ? null : g.values[q.id] }));
}

/** Sets one answer from an input value; an empty value clears it. Answering a skipped question un-skips it. */
export function withAnswer(g: QuestionGroup, id: string, value: unknown): QuestionGroup {
	if (!g.questions.some((q) => q.id === id)) return g;
	const text = answerText(value);
	const values = { ...g.values };
	if (text) values[id] = text;
	else delete values[id];
	return { ...g, values, skipped: text ? g.skipped.filter((s) => s !== id) : g.skipped, error: null };
}

/** Marks one question skipped, or un-skips it with `skipped` false. */
export function withSkip(g: QuestionGroup, id: string, skipped: boolean): QuestionGroup {
	if (!g.questions.some((q) => q.id === id)) return g;
	const rest = g.skipped.filter((s) => s !== id);
	return { ...g, skipped: skipped ? [...rest, id] : rest, error: null };
}

/** Typed composer text: fills the first open question. Returns null when none is open. */
export function fillNext(g: QuestionGroup, text: string): QuestionGroup | null {
	const id = missingOf(g)[0];
	return id ? withAnswer(g, id, text) : null;
}

/** The group as a chat message: inputs rebuilt from the answers, the count line, and any error. */
export function groupMessage(g: QuestionGroup, submitted = false): Extract<ChatMessage, { type: 'questions' }> {
	return {
		type: 'questions',
		persona: g.node === 'limit_choice' ? 'Aroh' : 'Intake interviewer',
		prompt: g.prompt,
		items: g.questions.map((q) => ({
			id: q.id,
			text: q.text,
			input: inputFor(q, g.values[q.id]),
			...(q.known ? { known: { value: q.known.value, source: `From ${q.known.source}` } } : {})
		})),
		values: { ...g.values },
		skipped: [...g.skipped],
		missing: missingOf(g),
		status: statusOf(g),
		submitted,
		sending: g.sending,
		error: g.error
	};
}

/**
 * Where the chat scrolls after `thread` changed, given the key of the open group already shown
 * (`seen`): to a new open group's first question, nowhere while the owner answers the group shown,
 * else to the bottom. Returns the target and the key to remember.
 */
export function scrollFor(thread: ChatMessage[], seen: string | null): { to: 'bottom' | 'group' | 'stay'; seen: string | null } {
	const last = thread.at(-1);
	if (last?.type !== 'questions' || last.submitted) return { to: 'bottom', seen: null };
	const key = last.items.map((i) => i.id).join(',');
	return key === seen ? { to: 'stay', seen } : { to: 'group', seen: key };
}
