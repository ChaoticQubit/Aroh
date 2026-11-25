import { describe, expect, it } from 'vitest';
import type { PluginSummary, PluginVersion } from './api';
import { cadenceText, describeSections, fieldText, pluginPage, pluginPageFor, pluginRows, trackerRows } from './describe';
import allKinds from './fixtures/all-kinds.tree.json';
import learning from './fixtures/learning.tree.json';

function version(id: string, name: string, n: number, active: boolean): PluginVersion {
	return { id, name, version: n, enabled: active, active, tree: {}, error: null };
}

describe('plugin page words', () => {
	it('b81: says every check-in cadence in plain words', () => {
		expect(cadenceText({ freq: 'daily', weekdays: [], at: '21:00' })).toBe('Every day at 21:00');
		expect(cadenceText({ freq: 'weekly', weekdays: ['Mon', 'Thu'], at: '20:00' })).toBe('Every Monday and Thursday at 20:00');
		expect(cadenceText({ freq: 'weekly', weekdays: ['Mon', 'Wed', 'Fri'], at: '07:30' })).toBe('Every Monday, Wednesday and Friday at 07:30');
		expect(cadenceText({ freq: 'weekly', weekdays: [], at: '19:00' })).toBe('Once a week at 19:00');
		expect(cadenceText(null)).toBe('No set check-in time');
	});

	it('b81: names each field with its unit, or its kind when it has none', () => {
		expect(fieldText({ key: 'w', label: 'Weight', kind: 'number', unit: 'kg' })).toBe('Weight (kg)');
		expect(fieldText({ key: 'n', label: 'Note', kind: 'text', unit: null })).toBe('Note (text)');
		expect(fieldText({ key: 'd', label: 'Stretched', kind: 'bool' })).toBe('Stretched (yes or no)');
		const rows = trackerRows([{ name: 'Water', fields: [{ key: 'ml', label: 'Water', kind: 'number', unit: 'ml' }], cadence: { freq: 'weekly', weekdays: ['Sun'], at: '19:00' } }]);
		expect(rows).toEqual([{ name: 'Water', fields: ['Water (ml)'], cadence: 'Every Sunday at 19:00' }]);
	});

	it('b81: describes every section item of every kind with real words', () => {
		const sections = describeSections(allKinds);
		expect(sections.map((s) => s.title)).toEqual(['Inputs', 'Display', 'Planning', 'Agent']);
		const items = sections.flatMap((s) => s.items);
		expect(items).toHaveLength((allKinds.sections as { items: unknown[] }[]).reduce((n, s) => n + s.items.length, 0));
		for (const text of items) {
			expect(text.trim().length).toBeGreaterThan(0);
			expect(text).not.toMatch(/undefined|null|\[object/);
		}
		expect(items).toContain('Enter Weight in kg, from the Weight tracker');
		expect(items).toContain('Line chart of Weight in kg, from the Weight tracker');
		expect(items).toContain('Pick one of Gym or Run: Session');
		expect(items).toContain('Insight from Coach: Steady progress');
		expect(items).toContain('Question from Coach: How much did you weigh this morning?');
		expect(describeSections(learning).length).toBeGreaterThan(0);
	});

	it('b81: shows nothing for a tree that is missing or malformed', () => {
		expect(describeSections(null)).toEqual([]);
		expect(describeSections({ sections: 'x' })).toEqual([]);
		expect(describeSections({ sections: [{ title: 'Odd', items: [{ kind: 'mystery_kind' }, 7] }] })).toEqual([{ title: 'Odd', items: ['Mystery kind'] }]);
	});
});

describe('plugin page links', () => {
	it('b81: the goal-page row opens the active version of its name, else the newest', () => {
		const versions = [version('m3', 'ml', 3, false), version('m2', 'ml', 2, true), version('m1', 'ml', 1, false), version('f1', 'fitness', 1, false)];
		expect(pluginPageFor(versions, 'ml', 'g1')).toBe('/plugins/m2?goal=g1');
		expect(pluginPageFor([version('m1', 'ml', 1, false), version('m3', 'ml', 3, false)], 'ml', 'g1')).toBe('/plugins/m3?goal=g1');
		expect(pluginPageFor(versions, 'missing', 'g1')).toBeNull();
		expect(pluginPage('a b')).toBe('/plugins/a%20b');
	});

	it('b81: the coach library lists each live plugin with its goal and version', () => {
		const list: PluginSummary[] = [
			{ id: 'p1', name: 'machine-learning-course-1', title: 'ML course', version: 1, active: true, goal_id: 'g1', goal_title: 'Complete Machine Learning Course - 1' },
			{ id: 'p2', name: 'habits', title: 'Habits', version: 2, active: false, goal_id: null, goal_title: null }
		];
		expect(pluginRows(list)).toEqual([
			{ id: 'p1', title: 'ML course', note: 'Complete Machine Learning Course - 1 · machine-learning-course-1 v1, active', href: '/plugins/p1' },
			{ id: 'p2', title: 'Habits', note: 'No goal · habits v2', href: '/plugins/p2' }
		]);
	});
});
