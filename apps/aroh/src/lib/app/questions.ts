import { OTHER_CHOICE, type ChatMessage, type ComponentSpec } from '$lib/ui/contract';
import type { LimitsCard } from '$lib/ui/plan/limits';
import { boundedNumber } from '$lib/ui/format';
import type { Question } from './engine';

/** Slider bounds for a number question that names none, as in runs started before units existed. */
export const FALLBACK_BOUNDS = { min: 0, max: 100 } as const;

/** The choice every intake choice question ends with; picking it opens a free-text field (B107). */
export const OTHER = OTHER_CHOICE;

/** The node of the limits card, whose fixed choices are read by their first word, so it gets no Other. */
const LIMIT_NODE = 'limit_choice';

/** What the owner picked on a choice question: its own choices, and the Other text (null while Other is off). */
export type ChoicePick = { picked: string[]; other: string | null };

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
	card?: LimitsCard | null;
	picks?: Record<string, ChoicePick>;
};

/** An answer input's value as the text posted: choices joined, numbers as digits, blanks empty. */
export function answerText(value: unknown): string {
	if (Array.isArray(value)) return value.map(String).join(', ').trim();
	if (value === null || value === undefined) return '';
	if (typeof value === 'number' && !Number.isFinite(value)) return '';
	return String(value).trim();
}

/** A number question's own bounds when both are finite numbers with min below max, else null. */
function ownBounds(q: Question): { min: number; max: number } | null {
	const ok = typeof q.min === 'number' && typeof q.max === 'number' && Number.isFinite(q.min) && Number.isFinite(q.max) && q.min < q.max;
	return ok ? { min: q.min as number, max: q.max as number } : null;
}

/** The slider bounds of a number question: its own when both are numbers with min below max, else the fallback. */
export function boundsOf(q: Question): { min: number; max: number } {
	return ownBounds(q) ?? { ...FALLBACK_BOUNDS };
}

/** A choice question's own choices, any model-sent Other dropped so Other shows once. */
function choicesOf(q: Question): string[] {
	return q.choices.filter((c) => c.trim().toLowerCase() !== OTHER.toLowerCase());
}

/**
 * The pick an answer text stands for on choice question `q`: whole choices matched in order (a choice
 * may hold a comma), the rest as the Other text. A single-pick question is one choice or Other text.
 */
function pickFromText(q: Question, text: string | undefined): ChoicePick {
	const own = choicesOf(q);
	const t = (text ?? '').trim();
	if (!t) return { picked: [], other: null };
	if (own.includes(t)) return { picked: [t], other: null };
	if (!q.multi) return { picked: [], other: t };
	const parts = t.split(', ');
	const picked: string[] = [];
	const rest: string[] = [];
	for (let i = 0; i < parts.length; ) {
		let j = parts.length;
		while (j > i && !own.includes(parts.slice(i, j).join(', '))) j--;
		if (j > i) {
			picked.push(parts.slice(i, j).join(', '));
			i = j;
		} else rest.push(parts[i++]);
	}
	return { picked, other: rest.length ? rest.join(', ') : null };
}

/** The pick shown for choice question `q` in group `g`: the one the owner made, else read from its answer text. */
function pickOf(g: QuestionGroup, q: Question): ChoicePick {
	return g.picks?.[q.id] ?? pickFromText(q, g.values[q.id]);
}

/** A pick kept to the question's rules: own choices only, and one answer on a single-pick question (Other text wins). */
function fitPick(q: Question, pick: ChoicePick): ChoicePick {
	const picked = choicesOf(q).filter((c) => pick.picked.includes(c));
	if (q.multi) return { picked, other: pick.other };
	return pick.other !== null ? { picked: [], other: pick.other } : { picked: picked.slice(0, 1), other: null };
}

/** The answer text a pick posts: picks in choice order, then the Other text, joined with commas. */
function pickText(pick: ChoicePick): string {
	return [...pick.picked, (pick.other ?? '').trim()].filter(Boolean).join(', ');
}

/** True when `value` is a choice pick from the answer card. */
function isPick(value: unknown): value is ChoicePick {
	return typeof value === 'object' && value !== null && Array.isArray((value as ChoicePick).picked) && 'other' in value;
}

/**
 * The answer input of one question holding `value`: number with its unit and slider (clamped to its own
 * bounds), choice chips (with Other unless `other` is false, showing `pick` when given), or free text.
 */
export function inputFor(q: Question, value: string | undefined, pick?: ChoicePick, other = true): ComponentSpec {
	if (q.kind === 'number') {
		const n = value === undefined || value === '' ? null : Number(value);
		const own = ownBounds(q);
		return { kind: 'number_input', label: 'Your answer', unit: q.unit ?? '', value: n !== null && Number.isFinite(n) ? n : null, step: 1, ...boundsOf(q), ...(own ? { clamp: true } : {}) };
	}
	if (q.kind === 'choice') {
		if (!other) return { kind: 'choice', label: 'Your answer', options: q.choices, multi: false, value: value ? [value] : [] };
		const p = pick ?? pickFromText(q, value);
		return { kind: 'choice', label: 'Your answer', options: [...choicesOf(q), OTHER], multi: q.multi === true, value: [...p.picked, ...(p.other !== null ? [OTHER] : [])] };
	}
	return { kind: 'note', label: 'Your answer', placeholder: 'Type your answer', value: value ?? '' };
}

/**
 * The starting answers of a new group: each remembered (`known`) value, so it counts as
 * answered until the owner edits, clears or skips it (B82). Blank remembered values give none;
 * a number past the question's own bounds starts clamped to them (B106).
 */
export function knownValues(questions: Question[]): Record<string, string> {
	const values: Record<string, string> = {};
	for (const q of questions) {
		const text = q.known ? answerText(q.known.value) : '';
		if (!text) continue;
		const n = Number(text);
		values[q.id] = q.kind === 'number' && Number.isFinite(n) ? String(boundedNumber(n, ownBounds(q))) : (q.known as { value: string }).value;
	}
	return values;
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
	if (!missing) return `${parts.join(', ')}. Ready to send.`;
	return `${parts.join(', ')}. ${missing} still open.`;
}

/** The `questions` answer to post: every question in order, a skip as `answer: null`. Null while any is open. */
export function submissionOf(g: QuestionGroup): { id: string; answer: string | null }[] | null {
	if (missingOf(g).length) return null;
	return g.questions.map((q) => ({ id: q.id, answer: g.skipped.includes(q.id) ? null : g.values[q.id].trim() }));
}

/**
 * Sets one answer from an input value; an empty value clears it. Answering a skipped question un-skips it.
 * A number is clamped to the question's own bounds (B106); a choice pick posts as its joined text (B107).
 */
export function withAnswer(g: QuestionGroup, id: string, value: unknown): QuestionGroup {
	const q = g.questions.find((x) => x.id === id);
	if (!q) return g;
	const picks = { ...g.picks };
	delete picks[id];
	const typed = answerText(value);
	let text = typed;
	let kept = typeof value === 'string' ? value : typed;
	if (q.kind === 'choice' && isPick(value)) {
		const pick = fitPick(q, value);
		picks[id] = pick;
		text = kept = pickText(pick);
	} else if (q.kind === 'number' && typed !== '' && Number.isFinite(Number(typed))) {
		text = kept = String(boundedNumber(Number(typed), ownBounds(q)));
	}
	const values = { ...g.values };
	if (text) values[id] = kept;
	else delete values[id];
	return { ...g, values, picks, skipped: text ? g.skipped.filter((s) => s !== id) : g.skipped, error: null };
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
		persona: g.node === LIMIT_NODE ? 'Aroh' : 'Intake interviewer',
		node: g.node,
		...(g.node === LIMIT_NODE ? { card: g.card ?? null } : {}),
		prompt: g.prompt,
		items: g.questions.map((q) => {
			const pick = q.kind === 'choice' && g.node !== LIMIT_NODE ? pickOf(g, q) : undefined;
			return {
				id: q.id,
				text: q.text,
				input: inputFor(q, g.values[q.id], pick, g.node !== LIMIT_NODE),
				...(pick ? { other: pick.other } : {}),
				...(q.known ? { known: { value: q.known.value, source: `From ${q.known.source}` } } : {})
			};
		}),
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
