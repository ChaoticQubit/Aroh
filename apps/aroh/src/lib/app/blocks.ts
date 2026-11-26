/** A ProseMirror node as JSON: type, optional attrs, children, text and marks. */
export type NodeJson = { type: string; attrs?: Record<string, unknown>; content?: NodeJson[]; text?: string; marks?: { type: string; attrs?: Record<string, unknown> }[] };

/** The editor document as JSON: one top-level node per block. */
export type DocJson = { type: 'doc'; content: NodeJson[] };

/** One stored block: a top-level node, its stable id, its type and its fractional order key. */
export type Block = { id: string; type: string; order: string; content: NodeJson };

const DIGITS = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz';
const MAX_KEY = 64;

/** The key strictly between digit strings `a` ('' for the start) and `b` (null for the end). */
function midpoint(a: string, b: string | null): string {
	if (b !== null) {
		let n = 0;
		while ((a[n] ?? '0') === b[n]) n++;
		if (n > 0) return b.slice(0, n) + midpoint(a.slice(n), b.slice(n));
	}
	const lo = a ? DIGITS.indexOf(a[0]) : 0;
	const hi = b !== null ? DIGITS.indexOf(b[0]) : DIGITS.length;
	if (hi - lo > 1) return DIGITS[Math.round((lo + hi) / 2)];
	if (b !== null && b.length > 1) return b.slice(0, 1);
	return DIGITS[lo] + midpoint(a.slice(1), null);
}

/**
 * A fractional order key that sorts strictly between `a` and `b` as plain strings; null means
 * no neighbour on that side. Keys use base-62 digits and never end in '0', so there is always
 * room before and between. Throws when `a` does not sort before `b`.
 */
export function keyBetween(a: string | null, b: string | null): string {
	if (a !== null && b !== null && a >= b) throw new Error(`order keys out of order: ${a} >= ${b}`);
	const key = midpoint(a ?? '', b);
	console.assert((a === null || a < key) && (b === null || key < b), 'order key between its neighbours');
	console.assert(key.length > 0 && !key.endsWith('0'), 'order key is not empty and leaves room before it');
	return key;
}

/** `count` increasing keys strictly between `a` and `b`, split by halves so they stay short. */
export function keysBetween(a: string | null, b: string | null, count: number): string[] {
	console.assert(Number.isInteger(count) && count >= 0, 'key count is a whole number');
	if (count === 0) return [];
	const mid = keyBetween(a, b);
	const left = Math.floor((count - 1) / 2);
	return [...keysBetween(a, mid, left), mid, ...keysBetween(mid, b, count - 1 - left)];
}

/** The ids with the first of each repeated id kept and every missing or repeated id replaced by `fresh()`. */
export function uniqueIds(ids: (string | null | undefined)[], fresh: () => string): string[] {
	const seen = new Set<string>();
	const out = ids.map((id) => {
		const next = id && !seen.has(id) ? id : fresh();
		seen.add(next);
		return next;
	});
	console.assert(new Set(out).size === out.length, 'block ids are unique');
	return out;
}

/** The indices of a longest strictly increasing run of the present `keys`, in order. */
function keptIndices(keys: (string | undefined)[]): number[] {
	const best: number[][] = [];
	keys.forEach((key, i) => {
		if (key === undefined) return;
		let chain: number[] = [];
		for (let j = 0; j < i; j++) {
			const prev = keys[j];
			if (prev !== undefined && prev < key && best[j] && best[j].length > chain.length) chain = best[j];
		}
		best[i] = [...chain, i];
	});
	return best.reduce<number[]>((longest, chain) => (chain && chain.length > longest.length ? chain : longest), []);
}

/** `node` without the editor-only blockId attribute. */
function stripId(node: NodeJson): NodeJson {
	if (!node.attrs || !('blockId' in node.attrs)) return node;
	const { blockId: _, ...attrs } = node.attrs;
	return { ...node, attrs };
}

/**
 * The editor `doc` as stored blocks in document order. Ids come from each top-level node's
 * blockId (a new `fresh()` id when missing or repeated). Blocks that kept their relative place
 * keep their `previous` order key; new and moved blocks get keys between their neighbours, so a
 * save changes only the blocks that changed. When a key grows past 64 characters every key is
 * spread out again, so keys stay well inside the store's 128-character limit.
 */
export function docToBlocks(doc: DocJson, previous: Block[], fresh: () => string = () => crypto.randomUUID()): Block[] {
	const nodes = doc.content ?? [];
	const ids = uniqueIds(nodes.map((n) => n.attrs?.blockId as string | undefined), fresh);
	const before = new Map(previous.map((b) => [b.id, b.order]));
	const old = ids.map((id) => before.get(id));
	const kept = new Set(keptIndices(old));
	const orders: string[] = new Array(nodes.length);
	let lo: string | null = null;
	let start = 0;
	for (let i = 0; i <= nodes.length; i++) {
		if (i < nodes.length && !kept.has(i)) continue;
		const hi = i < nodes.length ? old[i]! : null;
		keysBetween(lo, hi, i - start).forEach((key, k) => (orders[start + k] = key));
		if (i < nodes.length) orders[i] = hi!;
		lo = hi;
		start = i + 1;
	}
	const spread = orders.some((o) => o.length > MAX_KEY) ? keysBetween(null, null, nodes.length) : orders;
	const blocks = nodes.map((node, i) => ({ id: ids[i], type: node.type, order: spread[i], content: stripId(node) }));
	for (let i = 1; i < blocks.length; i++) console.assert(blocks[i - 1].order < blocks[i].order, 'block orders increase');
	return blocks;
}

/** The editor document for stored `blocks`: sorted by order then id, each node tagged with its id. An empty page is one empty paragraph. */
export function blocksToDoc(blocks: Block[]): DocJson {
	if (!blocks.length) return { type: 'doc', content: [{ type: 'paragraph' }] };
	const sorted = [...blocks].sort((a, b) => (a.order < b.order ? -1 : a.order > b.order ? 1 : a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
	return { type: 'doc', content: sorted.map((b) => ({ ...b.content, type: b.content.type ?? b.type, attrs: { ...(b.content.attrs ?? {}), blockId: b.id } })) };
}

/** True when two blocks hold the same type, order and content. */
function sameBlock(a: Block, b: Block): boolean {
	return a.type === b.type && a.order === b.order && JSON.stringify(a.content) === JSON.stringify(b.content);
}

/**
 * Merges a page that changed elsewhere (`remote`) into the open edits (`local`), per block, with
 * `base` the list both last agreed on. A block changed here wins; a block left alone here takes
 * the remote version; a block deleted on one side stays deleted unless the other side changed it;
 * new blocks from both sides are kept. Returns blocks sorted by order, then id.
 */
export function mergeRemote(base: Block[], local: Block[], remote: Block[]): Block[] {
	const was = new Map(base.map((x) => [x.id, x]));
	const there = new Map(remote.map((x) => [x.id, x]));
	const here = new Map(local.map((x) => [x.id, x]));
	const out: Block[] = [];
	for (const mine of local) {
		const old = was.get(mine.id);
		const theirs = there.get(mine.id);
		const touched = !old || !sameBlock(old, mine);
		if (theirs) out.push(touched ? mine : theirs);
		else if (touched) out.push(mine);
	}
	for (const theirs of remote) {
		if (here.has(theirs.id)) continue;
		const old = was.get(theirs.id);
		if (!old || !sameBlock(old, theirs)) out.push(theirs);
	}
	console.assert(new Set(out.map((x) => x.id)).size === out.length, 'merged block ids are unique');
	return out.sort((a, b) => (a.order < b.order ? -1 : a.order > b.order ? 1 : a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}
