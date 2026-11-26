import { describe, expect, it } from 'vitest';
import { linkItems, linkLabel, moveTargets, pageHref, pageTree, recentPages, slashItems } from './notes';
import type { TreeRow } from './notebook';

const row = (id: string, kind: TreeRow['kind'], title: string, parent_id: string | null, updated_at = '2026-11-26T08:00:00Z'): TreeRow => ({ id, kind, title, parent_id, updated_at });

describe('page tree', () => {
	it('nests subpages under their parent, sorted by title, and hides childless day, action and goal pages', () => {
		const rows = [row('b', 'page', 'Books', null), row('a', 'page', 'Areas', null), row('a2', 'page', 'Zen', 'a'), row('a1', 'page', 'Health', 'a'), row('d', 'day', '2026-11-26', null), row('g', 'goal', 'Run a marathon', null), row('g1', 'page', 'Shoes', 'g')];
		const tree = pageTree(rows);
		expect(tree.map((n) => n.id)).toEqual(['a', 'b', 'g']);
		expect(tree[0].children.map((n) => n.id)).toEqual(['a1', 'a2']);
		expect(tree[2].children.map((n) => n.id)).toEqual(['g1']);
	});

	it('puts a page whose parent is gone at the top level instead of losing it', () => {
		expect(pageTree([row('x', 'page', 'Orphan', 'missing')]).map((n) => n.id)).toEqual(['x']);
	});

	it('survives a parent loop without hanging', () => {
		const tree = pageTree([row('a', 'page', 'A', 'b'), row('b', 'page', 'B', 'a')]);
		expect(tree.map((n) => n.id).sort()).toEqual(['a', 'b']);
	});
});

describe('recent pages', () => {
	it('lists the most recently edited pages first, at most `count`', () => {
		const rows = [row('a', 'page', 'A', null, '2026-11-20T00:00:00Z'), row('b', 'day', '2026-11-26', null, '2026-11-26T00:00:00Z'), row('c', 'page', 'C', null, '2026-11-24T00:00:00Z')];
		expect(recentPages(rows, 2).map((r) => r.id)).toEqual(['b', 'c']);
	});
});

describe('page links', () => {
	it('opens free pages by id and the others by their date, action or goal', () => {
		expect(pageHref('page', 'p1')).toBe('/notes/page/p1');
		expect(pageHref('day', '2026-11-26')).toBe('/notes/day/2026-11-26');
		expect(pageHref('action', 'a b')).toBe('/notes/action/a%20b');
	});

	it('offers today, yesterday and tomorrow, a typed date, pages, actions and goals for [[', () => {
		const items = linkItems('', {
			today: '2026-11-26',
			pages: [{ id: 'p1', kind: 'page', title: 'Reading list', snippet: '' }],
			actions: [{ id: 'a1', title: 'Long run' }],
			goals: [{ id: 'g1', title: 'Marathon' }]
		});
		expect(items.map((i) => [i.target_kind, i.target, i.label])).toEqual([
			['day', '2026-11-26', 'Today'],
			['day', '2026-11-25', 'Yesterday'],
			['day', '2026-11-27', 'Tomorrow'],
			['page', 'p1', 'Reading list'],
			['action', 'a1', 'Long run'],
			['goal', 'g1', 'Marathon']
		]);
	});

	it('filters by the typed words and takes a valid YYYY-MM-DD as a day', () => {
		const sources = { today: '2026-11-26', pages: [], actions: [{ id: 'a1', title: 'Long run' }, { id: 'a2', title: 'Swim' }], goals: [] };
		expect(linkItems('run', sources).map((i) => i.target)).toEqual(['a1']);
		expect(linkItems('tom', sources).map((i) => i.target)).toEqual(['2026-11-27']);
		expect(linkItems('2026-02-03', sources).map((i) => [i.target_kind, i.target])).toEqual([['day', '2026-02-03']]);
		expect(linkItems('2026-02-30', sources)).toEqual([]);
	});
});

describe('slash menu', () => {
	it('lists every phase 1 block and filters by name', () => {
		expect(slashItems('').map((i) => i.id)).toEqual(['paragraph', 'h1', 'h2', 'h3', 'bullet', 'numbered', 'check', 'quote', 'divider', 'link']);
		expect(slashItems('head').map((i) => i.id)).toEqual(['h1', 'h2', 'h3']);
		expect(slashItems('todo').map((i) => i.id)).toEqual(['check']);
	});
});

describe('move targets', () => {
	it('offers every page except the page itself and its subpages, sorted by title', () => {
		const rows = [row('a', 'page', 'A', null), row('a1', 'page', 'A1', 'a'), row('a11', 'page', 'A11', 'a1'), row('b', 'page', 'B', null), row('d', 'day', '2026-11-26', null)];
		expect(moveTargets(rows, 'a').map((r) => r.id)).toEqual(['d', 'b']);
		expect(moveTargets(rows, 'a11').map((r) => r.id)).toEqual(['d', 'a', 'a1', 'b']);
	});
});

describe('link labels', () => {
	it('names a link by page title, date, action or goal, with a fallback when it is gone', () => {
		const names = { rows: [row('p1', 'page', 'Reading list', null)], actions: [{ id: 'a1', title: 'Long run' }], goals: [{ id: 'g1', title: 'Marathon' }] };
		expect(linkLabel('page', 'p1', names)).toBe('Reading list');
		expect(linkLabel('day', '2026-11-26', names)).toBe('Thu, 26 Nov 2026');
		expect(linkLabel('action', 'a1', names)).toBe('Long run');
		expect(linkLabel('goal', 'g1', names)).toBe('Marathon');
		expect(linkLabel('page', 'gone', names)).toBe('Untitled page');
		expect(linkLabel('day', 'bad', names)).toBe('bad');
	});
});
