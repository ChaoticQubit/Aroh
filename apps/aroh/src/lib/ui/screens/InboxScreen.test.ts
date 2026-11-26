import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import InboxScreen from './InboxScreen.svelte';
import type { InboxItem, RunView } from './types';
import { gymPrompt } from '$lib/ui/mockdata/location';

const proposal: InboxItem = { id: 'p1', spec: { kind: 'proposal_card', persona: 'Adaptation analyst', summary: 'Move Read', changes: [] } };
const suggestion: InboxItem = { id: 'f1', spec: { kind: 'proposal_card', persona: 'Planner', summary: 'Evenings?', changes: [] } };
const insight: InboxItem = { id: 'i1', spec: { kind: 'insight_card', persona: 'Calendar', title: 'No free day', body: 'Busy all week.' } };
const triage: InboxItem = { id: 'Work', spec: { kind: 'question_card', persona: 'Calendar triage', question: 'Work: 3 events. Keep them as busy time?', input: { kind: 'choice', label: 'x', options: ['Keep', 'Ignore'], multi: false, value: ['Keep'] } } };
const run: RunView = { id: 'r1', confirm: false, persona: 'Adaptation analyst', title: 'Replanning: Read', step: 'Failed', progress: 100, tone: 'missed', working: false };

describe('Inbox delete (B115)', () => {
	it('b115: every entry kind has a labelled delete button, and there is one Clear all', () => {
		const body = render(InboxScreen, { props: { approvals: [proposal], suggestions: [suggestion], insights: [insight], runs: [run], triage: [triage], places: [gymPrompt] } }).body;
		for (const label of ['Delete Iron Works Gym', 'Delete Move Read', 'Delete Evenings?', 'Delete No free day', 'Delete Work: 3 events. Keep them as busy time?', 'Delete Replanning: Read']) expect(body).toContain(`aria-label="${label}"`);
		expect(body.match(/>\s*Clear all\s*</g)).toHaveLength(1);
	});

	it('b115: an empty Inbox has no Clear all', () => {
		const body = render(InboxScreen, { props: { approvals: [], suggestions: [], insights: [], runs: [] } }).body;
		expect(body).not.toMatch(/Clear all/);
		expect(body).toContain('Nothing needs you right now.');
	});
});
