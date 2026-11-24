/** The part of `Storage` the chat keeps drafts and sent messages in; tests pass a map. */
export type Keep = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

/** One owner message the goal chat sent, with when (epoch ms). */
export type Sent = { text: string; at: number };

/** Longest draft or kept message, in characters. */
export const MAX_TEXT = 4000;

/** Most sent messages kept per goal; older ones drop off. */
export const MAX_KEPT = 50;

/** This device's web storage, or null where there is none (tests, a blocked store). */
export function deviceKeep(): Keep | null {
	try {
		return typeof localStorage === 'undefined' ? null : localStorage;
	} catch {
		return null;
	}
}

/** The unsent draft of chat `key` (`goal:<id>` or `card:<id>`), or empty. */
export function readDraft(store: Keep | null, key: string): string {
	try {
		return store?.getItem(`aroh.draft.${key}`) ?? '';
	} catch {
		return '';
	}
}

/** Keeps `text` as the draft of chat `key`, capped at `MAX_TEXT`; empty text removes it. */
export function writeDraft(store: Keep | null, key: string, text: string) {
	if (!text) return clearDraft(store, key);
	try {
		store?.setItem(`aroh.draft.${key}`, text.slice(0, MAX_TEXT));
	} catch {
		return;
	}
}

/** Drops the draft of chat `key`, once it is sent. */
export function clearDraft(store: Keep | null, key: string) {
	try {
		store?.removeItem(`aroh.draft.${key}`);
	} catch {
		return;
	}
}

/** The owner messages goal `goalId`'s chat sent on this device, oldest first; junk entries are skipped. */
export function readSent(store: Keep | null, goalId: string): Sent[] {
	try {
		const raw: unknown = JSON.parse(store?.getItem(`aroh.sent.${goalId}`) ?? '[]');
		if (!Array.isArray(raw)) return [];
		return raw.filter((m): m is Sent => typeof m?.text === 'string' && typeof m?.at === 'number').slice(-MAX_KEPT);
	} catch {
		return [];
	}
}

/** Adds one sent owner message to goal `goalId`'s kept history, keeping the newest `MAX_KEPT`. */
export function rememberSent(store: Keep | null, goalId: string, text: string, at: number) {
	const next = [...readSent(store, goalId), { text: text.slice(0, MAX_TEXT), at }].slice(-MAX_KEPT);
	console.assert(next.length <= MAX_KEPT, 'sent history over its cap');
	try {
		store?.setItem(`aroh.sent.${goalId}`, JSON.stringify(next));
	} catch {
		return;
	}
}
