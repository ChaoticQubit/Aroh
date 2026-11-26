import { describe, expect, it } from 'vitest';
import { getSchema } from '@tiptap/core';
import { Node } from '@tiptap/pm/model';
import { EditorState } from '@tiptap/pm/state';
import { blocksToDoc, docToBlocks, type Block, type DocJson } from '$lib/app/blocks';
import { idsAfter, schemaExtensions } from './extensions';

const schema = getSchema(schemaExtensions());

/** A new-id source that yields n1, n2, ... */
function counter(): () => string {
	let n = 0;
	return () => `n${++n}`;
}

const stored: Block[] = [
	{ id: 'h', type: 'heading', order: 'F', content: { type: 'heading', attrs: { level: 1 }, content: [{ type: 'text', text: 'Week' }] } },
	{ id: 'p', type: 'paragraph', order: 'V', content: { type: 'paragraph', content: [{ type: 'text', text: 'See ' }, { type: 'pageLink', attrs: { target_kind: 'day', target: '2026-11-26' } }] } },
	{ id: 't', type: 'taskList', order: 'k', content: { type: 'taskList', content: [{ type: 'taskItem', attrs: { checked: true }, content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Run' }] }] }] } },
	{ id: 'r', type: 'horizontalRule', order: 's', content: { type: 'horizontalRule' } }
];

describe('editor schema and stored blocks', () => {
	it('round trips stored blocks through the real editor schema with ids and orders unchanged', () => {
		const json = Node.fromJSON(schema, blocksToDoc(stored)).toJSON() as DocJson;
		const back = docToBlocks(json, stored, counter());
		expect(back.map((b) => [b.id, b.type, b.order])).toEqual(stored.map((b) => [b.id, b.type, b.order]));
		expect(back[1].content.content?.[1]).toEqual({ type: 'pageLink', attrs: { target_kind: 'day', target: '2026-11-26' } });
		expect(Node.fromJSON(schema, blocksToDoc(back)).eq(Node.fromJSON(schema, json))).toBe(true);
	});

	it('keeps the id of a paragraph turned into a heading, and gives a split-off paragraph a new one', () => {
		const state = EditorState.create({ schema, doc: Node.fromJSON(schema, blocksToDoc(stored)) });
		const at = state.doc.child(0).nodeSize + 1;
		const turned = state.tr.setBlockType(at, at, schema.nodes.heading, { level: 2 });
		expect(idsAfter([turned], counter())).toEqual(['h', 'p', 't', 'r']);
		const split = state.tr.split(at + 2);
		expect(idsAfter([split], counter())).toEqual(['h', 'p', 'n1', 't', 'r']);
	});
});
