import type { ChatMessage } from '$lib/ui/contract';
import { ACTIVE } from './engine';

/** A send that did not reach the agents: the owner's `text`, kept for Retry, and the refusal `message`. */
export type FailedSend = { text: string; message: string };

/** The working bubble or the error that ends a chat thread (Q87). */
export type ChatTail = Extract<ChatMessage, { type: 'typing' | 'failed' }>;

/**
 * Whether a run the chat follows (`following` of them) is working: until its first read lands
 * (`read` false), then while its `status` is queued, running or checkpointed.
 */
export function runWorking(following: number, read: boolean, status: string | null): boolean {
	console.assert(following >= 0, 'following counts runs being followed');
	if (following === 0) return false;
	return !read || (status !== null && ACTIVE.includes(status));
}

/**
 * The entry that ends the thread: the failed send's error, else while `working` the bubble with
 * the step of the `working` row in `live`, else none.
 */
export function bubbleOf(working: boolean, live: ChatMessage[], failed: FailedSend | null): ChatTail | null {
	if (failed) return { type: 'failed', text: failed.message };
	if (!working) return null;
	const row = live.findLast((m) => m.type === 'working');
	return { type: 'typing', step: row?.type === 'working' ? row.step : null };
}

/** Splits a trailing bubble or error off `thread`, so the rest renders and scrolls as before. */
export function splitTail(thread: ChatMessage[]): { body: ChatMessage[]; tail: ChatTail | null } {
	const last = thread.at(-1);
	if (last?.type === 'typing' || last?.type === 'failed') return { body: thread.slice(0, -1), tail: last };
	return { body: thread, tail: null };
}

/**
 * What the chat does about `tail`: right after Send, or while the owner sees the end (`atEnd`), it
 * scrolls the bubble or error into view; with the owner scrolled up the bubble becomes the jump pill.
 */
export function followOf(tail: ChatTail | null, atEnd: boolean, sentNow: boolean): 'scroll' | 'pill' | 'stay' {
	if (!tail) return 'stay';
	if (sentNow || atEnd) return 'scroll';
	return tail.type === 'typing' ? 'pill' : 'stay';
}
