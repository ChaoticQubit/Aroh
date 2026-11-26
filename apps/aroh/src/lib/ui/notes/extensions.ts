import { Extension, Node, mergeAttributes, type Editor, type Extensions, type Range } from '@tiptap/core';
import { Plugin, PluginKey, type Transaction } from '@tiptap/pm/state';
import StarterKit from '@tiptap/starter-kit';
import { TaskItem, TaskList } from '@tiptap/extension-list';
import { Placeholder } from '@tiptap/extensions';
import Suggestion, { type SuggestionKeyDownProps, type SuggestionProps } from '@tiptap/suggestion';
import { uniqueIds } from '$lib/app/blocks';
import type { LinkItem, SlashItem } from '$lib/app/notes';
import type { PageKind } from '$lib/app/notebook';

/** The transaction meta key marking the id pass on open, which is not an edit to save. */
export const OPENED = 'blockIdOpened';

/** How long the [[ menu waits after a keystroke before it searches pages. */
const LINK_DEBOUNCE_MS = 150;

/** The top-level node types that carry a stable `blockId`. */
export const BLOCK_TYPES = ['paragraph', 'heading', 'bulletList', 'orderedList', 'taskList', 'blockquote', 'horizontalRule'];

/** The open state of a suggestion menu, as the Svelte component draws it. */
export type MenuState<T> = { items: T[]; index: number; rect: DOMRect | null; pick: (item: T) => void };

/** Where a suggestion menu reports its state and asks for key handling. */
export type MenuHost<T> = { show: (state: MenuState<T> | null) => void; key: (event: KeyboardEvent) => boolean };

/** What the editor needs from the page around it. */
export type EditorHooks = {
	label: (kind: PageKind, target: string) => string;
	links: (query: string) => Promise<LinkItem[]>;
	slash: (query: string) => SlashItem[];
	slashMenu: MenuHost<SlashItem>;
	linkMenu: MenuHost<LinkItem>;
	newId: () => string;
};

/**
 * The block ids each top-level node should have after `trs`: a node replaced in place (a
 * paragraph turned into a heading) keeps the id of the node it replaced, then missing and
 * repeated ids get fresh ones.
 */
export function idsAfter(trs: readonly Transaction[], fresh: () => string): string[] | null {
	const last = trs[trs.length - 1];
	if (!last || !trs.some((tr) => tr.docChanged)) return null;
	const before = trs[0].before;
	const doc = last.doc;
	const inherited = new Map<number, string>();
	before.forEach((node, offset) => {
		const id = node.attrs.blockId as string | null | undefined;
		if (!id) return;
		let pos = offset;
		for (const tr of trs) pos = tr.mapping.map(pos, 1);
		inherited.set(pos, id);
	});
	const present = new Set<string>();
	doc.forEach((node) => node.attrs.blockId && present.add(node.attrs.blockId as string));
	const wanted: (string | null)[] = [];
	doc.forEach((node, offset) => {
		const own = node.attrs.blockId as string | null | undefined;
		const old = inherited.get(offset);
		wanted.push(own ?? (old && !present.has(old) ? (present.add(old), old) : null));
	});
	return uniqueIds(wanted, fresh);
}

/** Gives every top-level node of a freshly opened `editor` a unique `blockId`, outside the undo history. */
export function ensureIds(editor: Editor, fresh: () => string) {
	const { state } = editor;
	const current: (string | null)[] = [];
	state.doc.forEach((node) => current.push((node.attrs.blockId as string | null) ?? null));
	const ids = uniqueIds(current, fresh);
	const tr = state.tr;
	state.doc.forEach((node, offset, i) => {
		if (node.attrs.blockId !== ids[i] && node.type.spec.attrs?.blockId) tr.setNodeAttribute(offset, 'blockId', ids[i]);
	});
	if (tr.docChanged) editor.view.dispatch(tr.setMeta('addToHistory', false).setMeta(OPENED, true));
}

/** Keeps a unique `blockId` on every top-level node, outside the undo history. */
function blockIds(newId: () => string) {
	return Extension.create({
		name: 'blockId',
		addGlobalAttributes() {
			return [
				{
					types: BLOCK_TYPES,
					attributes: {
						blockId: {
							default: null,
							keepOnSplit: false,
							parseHTML: (el) => el.getAttribute('data-block-id'),
							renderHTML: (attrs) => (attrs.blockId ? { 'data-block-id': attrs.blockId } : {})
						}
					}
				}
			];
		},
		addProseMirrorPlugins() {
			return [
				new Plugin({
					key: new PluginKey('blockId'),
					appendTransaction: (trs, _old, state) => {
						const ids = idsAfter(trs, newId);
						if (!ids) return null;
						const tr = state.tr;
						let i = 0;
						state.doc.forEach((node, offset) => {
							if (node.attrs.blockId !== ids[i] && node.type.spec.attrs?.blockId) tr.setNodeAttribute(offset, 'blockId', ids[i]);
							i++;
						});
						return tr.docChanged ? tr.setMeta('addToHistory', false) : null;
					}
				})
			];
		}
	});
}

/** An inline link to a page, day, action or goal; the label is looked up when drawn. */
function pageLink(label: EditorHooks['label']) {
	return Node.create({
		name: 'pageLink',
		group: 'inline',
		inline: true,
		atom: true,
		selectable: true,
		addAttributes() {
			return {
				target_kind: { default: 'page', parseHTML: (el) => el.getAttribute('data-kind'), renderHTML: () => ({}) },
				target: { default: '', parseHTML: (el) => el.getAttribute('data-target'), renderHTML: () => ({}) }
			};
		},
		parseHTML() {
			return [{ tag: 'a[data-page-link]' }];
		},
		renderHTML({ node, HTMLAttributes }) {
			const kind = node.attrs.target_kind as PageKind;
			const target = node.attrs.target as string;
			return ['a', mergeAttributes(HTMLAttributes, { 'data-page-link': '', 'data-kind': kind, 'data-target': target, role: 'link', class: 'page-link' }), label(kind, target)];
		},
		renderText({ node }) {
			return `[[${label(node.attrs.target_kind as PageKind, node.attrs.target as string)}]]`;
		}
	});
}

/** The render callbacks that turn suggestion events into menu state for `host`. */
function menuRender<T>(host: MenuHost<T>) {
	return () => {
		let index = 0;
		let props: SuggestionProps<T, T> | null = null;
		const show = () => props && host.show({ items: props.items, index: Math.min(index, Math.max(props.items.length - 1, 0)), rect: props.clientRect?.() ?? null, pick: (item) => props?.command(item) });
		return {
			onStart: (p: SuggestionProps<T, T>) => ((props = p), (index = 0), show()),
			onUpdate: (p: SuggestionProps<T, T>) => ((props = p), (index = 0), show()),
			onExit: () => ((props = null), host.show(null)),
			onKeyDown: ({ event }: SuggestionKeyDownProps) => {
				if (!props || !props.items.length) return false;
				if (event.key === 'ArrowDown') return (index = (index + 1) % props.items.length), show(), true;
				if (event.key === 'ArrowUp') return (index = (index - 1 + props.items.length) % props.items.length), show(), true;
				if (event.key === 'Enter') return props.command(props.items[Math.min(index, props.items.length - 1)]), true;
				if (event.key === 'Escape') return host.show(null), true;
				return host.key(event);
			}
		};
	};
}

/** Runs the block command for slash entry `id` over `range`. */
export function runSlash(editor: Editor, range: Range, id: string) {
	const chain = editor.chain().focus().deleteRange(range);
	if (id === 'paragraph') chain.setParagraph();
	else if (id === 'h1' || id === 'h2' || id === 'h3') chain.setHeading({ level: Number(id[1]) as 1 | 2 | 3 });
	else if (id === 'bullet') chain.toggleBulletList();
	else if (id === 'numbered') chain.toggleOrderedList();
	else if (id === 'check') chain.toggleTaskList();
	else if (id === 'quote') chain.toggleBlockquote();
	else if (id === 'divider') chain.setHorizontalRule();
	else if (id === 'link') chain.insertContent('[[');
	chain.run();
}

/** The slash menu: '/' after a space or at a line start opens the block list. */
function slashMenu(hooks: EditorHooks) {
	return Extension.create({
		name: 'slashMenu',
		addProseMirrorPlugins() {
			return [
				Suggestion<SlashItem, SlashItem>({
					editor: this.editor,
					pluginKey: new PluginKey('slashMenu'),
					char: '/',
					items: ({ query }) => hooks.slash(query),
					command: ({ editor, range, props }) => runSlash(editor, range, props.id),
					render: menuRender(hooks.slashMenu)
				})
			];
		}
	});
}

/** The [[ menu: pages, days, actions and goals; picking one inserts a pageLink. */
function linkMenu(hooks: EditorHooks) {
	return Extension.create({
		name: 'linkMenu',
		addProseMirrorPlugins() {
			return [
				Suggestion<LinkItem, LinkItem>({
					editor: this.editor,
					pluginKey: new PluginKey('linkMenu'),
					char: '[[',
					allowSpaces: true,
					allowedPrefixes: null,
					debounce: LINK_DEBOUNCE_MS,
					items: ({ query }) => hooks.links(query),
					command: ({ editor, range, props }) =>
						editor
							.chain()
							.focus()
							.deleteRange(range)
							.insertContent([{ type: 'pageLink', attrs: { target_kind: props.target_kind, target: props.target } }, { type: 'text', text: ' ' }])
							.run(),
					render: menuRender(hooks.linkMenu)
				})
			];
		}
	});
}

/** The schema-only extension list: phase 1 blocks, block ids and pageLink, with no menus. */
export function schemaExtensions(label: EditorHooks['label'] = (_k, t) => t, newId: () => string = () => crypto.randomUUID()): Extensions {
	return [
		StarterKit.configure({ codeBlock: false, code: false, heading: { levels: [1, 2, 3] }, link: { openOnClick: false, autolink: true } }),
		TaskList,
		TaskItem.configure({ nested: true }),
		blockIds(newId),
		pageLink(label)
	];
}

/** Every editor extension: the schema plus placeholder and the slash and [[ menus. */
export function editorExtensions(hooks: EditorHooks): Extensions {
	return [...schemaExtensions(hooks.label, hooks.newId), Placeholder.configure({ placeholder: 'Write, or type / for blocks and [[ to link' }), slashMenu(hooks), linkMenu(hooks)];
}
