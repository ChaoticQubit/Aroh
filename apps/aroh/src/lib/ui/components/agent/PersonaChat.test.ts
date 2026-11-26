import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import PersonaChat from './PersonaChat.svelte';
import type { ChatMessage } from '$lib/ui/contract';

const proposal = (decided?: 'apply' | 'reject'): ChatMessage => ({
	type: 'proposal',
	key: 'r1:approve',
	id: 'p1',
	spec: { kind: 'proposal_card', persona: 'Adaptation analyst', summary: 'Read moves to Tuesday.', changes: [{ op: 'move', title: 'Read', before: 'Mon 18:00', after: 'Tue 15:00' }] },
	...(decided ? { decided } : {})
});

describe('a proposal in the chat thread (B114)', () => {
	it('b114: shows the summary, the changes, Apply and Dismiss', () => {
		const body = render(PersonaChat, { props: { messages: [proposal()] } }).body;
		expect(body).toContain('Read moves to Tuesday.');
		expect(body).toContain('Tue 15:00');
		expect(body).toMatch(/>\s*Apply\s*</);
		expect(body).toMatch(/>\s*Dismiss\s*</);
	});

	it('b114: a decided proposal shows what happened instead of the buttons', () => {
		const body = render(PersonaChat, { props: { messages: [proposal('apply')] } }).body;
		expect(body).toContain('Applied. You can undo this from the calendar.');
		expect(body).not.toMatch(/>\s*Dismiss\s*</);
	});
});
