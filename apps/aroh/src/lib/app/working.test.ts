import { describe, expect, it } from 'vitest';
import type { ChatMessage } from '$lib/ui/contract';
import { bubbleOf, followOf, runWorking, splitTail } from './working';

const owner: ChatMessage = { type: 'message', from: 'owner', name: 'You', text: 'Move it to mornings' };
const reply: ChatMessage = { type: 'message', from: 'persona', name: 'Aroh', text: 'Done.' };

describe('working bubble (Q87)', () => {
	it('shows no bubble while nothing is in flight', () => {
		expect(bubbleOf(false, [], null)).toBeNull();
	});

	it('shows the bubble while work runs, with the step of the run once one is known', () => {
		expect(bubbleOf(true, [], null)).toEqual({ type: 'typing', step: null });
		const live: ChatMessage[] = [{ type: 'working', step: 'Researching', cost: '$0.01' }];
		expect(bubbleOf(true, live, null)).toEqual({ type: 'typing', step: 'Researching' });
	});

	it('a failed send replaces the bubble with the error', () => {
		expect(bubbleOf(true, [], { text: 'hi', message: 'Network down' })).toEqual({ type: 'failed', text: 'Network down' });
		expect(bubbleOf(false, [], { text: 'hi', message: 'Network down' })).toEqual({ type: 'failed', text: 'Network down' });
	});

	it('peels the bubble off the thread so the rest scrolls as before', () => {
		const typing: ChatMessage = { type: 'typing', step: null };
		expect(splitTail([owner, typing])).toEqual({ body: [owner], tail: typing });
		expect(splitTail([owner, reply])).toEqual({ body: [owner, reply], tail: null });
		expect(splitTail([])).toEqual({ body: [], tail: null });
	});
});

describe('a run working for the chat (Q87)', () => {
	it('works until its first read lands, then while queued, running or checkpointed', () => {
		expect(runWorking(1, false, null)).toBe(true);
		for (const status of ['queued', 'running', 'checkpointed']) expect(runWorking(1, true, status)).toBe(true);
	});

	it('stops working once it waits, ends or fails, and with no run followed', () => {
		for (const status of ['waiting_human', 'done', 'failed', 'cancelled']) expect(runWorking(1, true, status)).toBe(false);
		expect(runWorking(0, false, null)).toBe(false);
		expect(runWorking(0, true, 'running')).toBe(false);
	});
});

describe('following the newest message (Q87)', () => {
	const typing: ChatMessage = { type: 'typing', step: null };

	it('scrolls to the bubble right after Send', () => {
		expect(followOf(typing, false, true)).toBe('scroll');
	});

	it('keeps the bubble in view while the owner reads the end', () => {
		expect(followOf(typing, true, false)).toBe('scroll');
	});

	it('shows the jump pill instead when the owner scrolled up', () => {
		expect(followOf(typing, false, false)).toBe('pill');
	});

	it('stays put with no bubble, and scrolls to an error only at the end', () => {
		expect(followOf(null, false, false)).toBe('stay');
		expect(followOf(null, true, false)).toBe('stay');
		expect(followOf({ type: 'failed', text: 'x' }, true, false)).toBe('scroll');
		expect(followOf({ type: 'failed', text: 'x' }, false, false)).toBe('stay');
	});
});
