import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import ActionEditor from './ActionEditor.svelte';
import type { ActionView } from '$lib/ui/contract';

const base: ActionView = { id: 'a1', title: 'Leg day', start: '2026-11-26T06:00:00Z', end: '2026-11-26T06:45:00Z', status: 'planned', locked: false, category: 1, goalTitle: 'Routines' };

describe('ActionEditor delete confirm (B100)', () => {
	it('b100: a series session asks this session or this and all future; a one-off keeps the single confirm', () => {
		const series = render(ActionEditor, { props: { action: { ...base, series: true }, confirming: true } }).body;
		expect(series).toContain('This session only');
		expect(series).toContain('This and all future sessions');
		const single = render(ActionEditor, { props: { action: base, confirming: true } }).body;
		expect(single).not.toContain('This and all future sessions');
		expect(single).toMatch(/>\s*Delete\s*</);
	});
});
