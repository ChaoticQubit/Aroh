import { describe, expect, it } from 'vitest';
import { blocksToDoc, docToBlocks, keyBetween, keysBetween, mergeRemote, uniqueIds, type Block, type DocJson } from './blocks';

/** A new-id source that yields n1, n2, ... so tests can name the ids they expect. */
function counter(): () => string {
	let n = 0;
	return () => `n${++n}`;
}

/** A paragraph node with `text` and, when given, a block id. */
function para(text: string, blockId?: string) {
	return { type: 'paragraph', attrs: blockId ? { blockId } : {}, content: [{ type: 'text', text }] };
}

const doc = (...content: DocJson['content']): DocJson => ({ type: 'doc', content });

describe('fractional order keys', () => {
	it('puts a key strictly between its neighbours, or before the first and after the last', () => {
		const first = keyBetween(null, null);
		const after = keyBetween(first, null);
		const before = keyBetween(null, first);
		const mid = keyBetween(first, after);
		expect(before < first && first < mid && mid < after).toBe(true);
	});

	it('keeps finding room between two keys that are next to each other', () => {
		let lo = keyBetween(null, null);
		const hi = keyBetween(lo, null);
		for (let i = 0; i < 50; i++) {
			const k = keyBetween(lo, hi);
			expect(lo < k && k < hi).toBe(true);
			lo = k;
		}
	});

	it('spreads many new keys evenly so bulk inserts stay short', () => {
		const keys = keysBetween(null, null, 200);
		expect(keys).toHaveLength(200);
		for (let i = 1; i < keys.length; i++) expect(keys[i - 1] < keys[i]).toBe(true);
		expect(Math.max(...keys.map((k) => k.length))).toBeLessThanOrEqual(3);
	});

	it('refuses neighbours in the wrong order', () => {
		expect(() => keyBetween('b', 'a')).toThrow();
	});
});

describe('stable block ids in the editor', () => {
	it('keeps the first of a repeated id and gives missing or repeated ids new ones', () => {
		expect(uniqueIds(['a', null, 'a', 'b'], counter())).toEqual(['a', 'n1', 'n2', 'b']);
	});
});

describe('doc to blocks and back', () => {
	it('round trips a doc through blocks without losing anything', () => {
		const d = doc(
			{ type: 'heading', attrs: { level: 2, blockId: 'h' }, content: [{ type: 'text', text: 'Plan' }] },
			para('one', 'p'),
			{ type: 'taskList', attrs: { blockId: 't' }, content: [{ type: 'taskItem', attrs: { checked: true }, content: [para('done')] }] },
			{ type: 'paragraph', attrs: { blockId: 'l' }, content: [{ type: 'pageLink', attrs: { target_kind: 'day', target: '2026-11-26' } }] }
		);
		const blocks = docToBlocks(d, [], counter());
		expect(blocks.map((b) => [b.id, b.type])).toEqual([
			['h', 'heading'],
			['p', 'paragraph'],
			['t', 'taskList'],
			['l', 'paragraph']
		]);
		expect(blocks.every((b) => !('blockId' in ((b.content.attrs as Record<string, unknown>) ?? {})))).toBe(true);
		expect(blocksToDoc(blocks)).toEqual(d);
	});

	it('keeps the id and order of a block whose text was edited', () => {
		const before = docToBlocks(doc(para('a', 'x'), para('b', 'y')), [], counter());
		const after = docToBlocks(doc(para('a edited', 'x'), para('b', 'y')), before, counter());
		expect(after.map((b) => [b.id, b.order])).toEqual(before.map((b) => [b.id, b.order]));
		expect(after[0].content).toEqual({ type: 'paragraph', attrs: {}, content: [{ type: 'text', text: 'a edited' }] });
	});

	it('gives a new block a new id and an order between its neighbours, leaving them as they were', () => {
		const before = docToBlocks(doc(para('a', 'x'), para('c', 'z')), [], counter());
		const after = docToBlocks(doc(para('a', 'x'), para('b'), para('c', 'z')), before, counter());
		expect(after.map((b) => b.id)).toEqual(['x', 'n1', 'z']);
		expect(after[0].order).toBe(before[0].order);
		expect(after[2].order).toBe(before[1].order);
		expect(after[0].order < after[1].order && after[1].order < after[2].order).toBe(true);
	});

	it('reorders only the moved block when blocks change places', () => {
		const before = docToBlocks(doc(para('a', 'a'), para('b', 'b'), para('c', 'c')), [], counter());
		const after = docToBlocks(doc(para('c', 'c'), para('a', 'a'), para('b', 'b')), before, counter());
		const old = new Map(before.map((b) => [b.id, b.order]));
		expect(after.filter((b) => b.order !== old.get(b.id)).map((b) => b.id)).toEqual(['c']);
		for (let i = 1; i < after.length; i++) expect(after[i - 1].order < after[i].order).toBe(true);
	});

	it('respaces every order key once one grows past 64 characters, so keys stay within the store limit', () => {
		let blocks: Block[] = [];
		const nodes: DocJson['content'] = [];
		const fresh = counter();
		for (let i = 0; i < 600; i++) {
			nodes.push(para(`line ${i}`, `b${i}`));
			blocks = docToBlocks(doc(...nodes), blocks, fresh);
		}
		expect(Math.max(...blocks.map((x) => x.order.length))).toBeLessThanOrEqual(64);
		for (let i = 1; i < blocks.length; i++) expect(blocks[i - 1].order < blocks[i].order).toBe(true);
	});

	it('orders blocks from storage by order, then id', () => {
		const blocks: Block[] = [
			{ id: 'b', type: 'paragraph', order: 'V', content: para('second') },
			{ id: 'a', type: 'paragraph', order: 'K', content: para('first') }
		];
		expect(blocksToDoc(blocks).content.map((n) => n.attrs?.blockId)).toEqual(['a', 'b']);
	});

	it('opens an empty page as one empty paragraph', () => {
		expect(blocksToDoc([])).toEqual({ type: 'doc', content: [{ type: 'paragraph' }] });
	});
});

describe('merging a synced page into open edits', () => {
	const b = (id: string, order: string, text: string): Block => ({ id, type: 'paragraph', order, content: para(text) });
	const ids = (blocks: Block[]) => blocks.map((x) => `${x.id}:${(x.content.content?.[0] as { text: string }).text}`);

	it('keeps local edits and new local blocks, and takes remote edits to blocks left alone here', () => {
		const base = [b('a', 'F', 'a'), b('c', 'V', 'c'), b('d', 'k', 'd')];
		const local = [b('a', 'F', 'a local'), b('n', 'P', 'new here'), b('c', 'V', 'c'), b('d', 'k', 'd')];
		const remote = [b('a', 'F', 'a remote'), b('c', 'V', 'c remote'), b('d', 'k', 'd'), b('r', 's', 'new there')];
		expect(ids(mergeRemote(base, local, remote))).toEqual(['a:a local', 'n:new here', 'c:c remote', 'd:d', 'r:new there']);
	});

	it('drops blocks deleted on either side unless this side changed them', () => {
		const base = [b('a', 'F', 'a'), b('c', 'V', 'c'), b('d', 'k', 'd')];
		const local = [b('c', 'V', 'c'), b('d', 'k', 'd edited')];
		const remote = [b('a', 'F', 'a'), b('c', 'V', 'c')];
		expect(ids(mergeRemote(base, local, remote))).toEqual(['c:c', 'd:d edited']);
	});

	it('changes nothing when the remote page is what this side last saved', () => {
		const base = [b('a', 'F', 'a')];
		const local = [b('a', 'F', 'a typed'), b('n', 'V', 'n')];
		expect(mergeRemote(base, local, base)).toEqual(local);
	});
});
