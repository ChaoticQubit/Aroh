<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { Editor } from '@tiptap/core';
	import { TextSelection } from '@tiptap/pm/state';
	import { blocksToDoc, docToBlocks, mergeRemote, type Block, type DocJson } from '$lib/app/blocks';
	import { slashItems, type LinkItem, type SlashItem } from '$lib/app/notes';
	import type { PageKind } from '$lib/app/notebook';
	import { Saver } from '$lib/app/saver';
	import { editorExtensions, ensureIds, OPENED, type MenuState } from './extensions';
	import SuggestMenu from './SuggestMenu.svelte';

	/**
	 * The block editor of one page (Tiptap on a plain element). `blocks` is the stored content at
	 * open; edits save through `onsave` with the full ordered block list about 800 ms after typing
	 * pauses, and at once on blur, navigation, hiding the app and unmount. The `onsave` given at
	 * mount is the one used, so a late save never lands on another page. `label` names a link
	 * target, `links` answers the [[ menu, `onopen` follows a tapped page link. Each new `remote`
	 * (the page as the store has it after a sync) is merged in per block before the next save,
	 * because a save replaces the whole stored page.
	 */
	let {
		blocks,
		remote = null,
		label,
		links,
		onsave,
		onerror,
		onopen
	}: {
		blocks: Block[];
		remote?: { blocks: Block[] } | null;
		label: (kind: PageKind, target: string) => string;
		links: (query: string) => Promise<LinkItem[]>;
		onsave: (blocks: Block[]) => Promise<unknown>;
		onerror?: (error: unknown) => void;
		onopen: (kind: PageKind, target: string) => void;
	} = $props();

	const SAVE_MS = 800;
	let element: HTMLDivElement;
	let editor: Editor | null = null;
	let slash = $state<MenuState<SlashItem> | null>(null);
	let link = $state<MenuState<LinkItem> | null>(null);
	let last: Block[] = [];
	let base: Block[] = [];
	const newId = () => crypto.randomUUID();
	let save: (blocks: Block[]) => Promise<unknown> = () => Promise.resolve();
	const saver = new Saver<Block[]>(SAVE_MS, async (next) => ((await save(next)), (base = next)), (e) => onerror?.(e));

	/** The editor content as stored blocks, keeping the order keys of blocks that did not move. */
	function read(): Block[] {
		console.assert(editor !== null, 'read needs a mounted editor');
		last = docToBlocks(editor!.getJSON() as DocJson, last, newId);
		return last;
	}

	/** True when two block lists hold the same blocks in the same order. */
	const sameList = (a: Block[], b: Block[]) => JSON.stringify(a) === JSON.stringify(b);

	/** Merges the store's `stored` blocks into the open edits; saves when the merge differs from the store. */
	function merge(stored: Block[]) {
		if (!editor) return;
		const local = read();
		const merged = mergeRemote(base, local, stored);
		base = stored;
		if (!sameList(merged, local)) {
			editor.commands.setContent(blocksToDoc(merged), { emitUpdate: false });
			last = merged;
		}
		if (!sameList(merged, stored)) saver.schedule(read);
	}

	$effect(() => {
		const next = remote;
		if (next) untrack(() => merge(next.blocks));
	});

	beforeNavigate(() => void saver.flush());

	onMount(() => {
		last = blocks;
		base = blocks;
		save = onsave;
		editor = new Editor({
			element,
			extensions: editorExtensions({
				label: (kind, target) => label(kind, target),
				links: (query) => links(query),
				slash: slashItems,
				slashMenu: { show: (s) => (slash = s), key: () => false },
				linkMenu: { show: (s) => (link = s), key: () => false },
				newId
			}),
			content: blocksToDoc(blocks),
			editorProps: {
				attributes: { class: 'note-editor text-lg', 'aria-label': 'Page text', autocapitalize: 'sentences', spellcheck: 'true' },
				handleDOMEvents: {
					click: (view, event) => {
						const link = (event.target as Element | null)?.closest?.('a[data-page-link]');
						if (!link) return false;
						event.preventDefault();
						const pos = view.posAtDOM(link, 0);
						const node = view.state.doc.nodeAt(pos);
						if (node?.type.name === 'pageLink') view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, pos + node.nodeSize)));
						onopen(link.getAttribute('data-kind') as PageKind, link.getAttribute('data-target') ?? '');
						return true;
					}
				}
			},
			onUpdate: ({ transaction }) => {
				if (!transaction.getMeta(OPENED)) saver.schedule(read);
			},
			onBlur: () => void saver.flush()
		});
		ensureIds(editor, newId);
		const hide = () => document.visibilityState === 'hidden' && void saver.flush();
		document.addEventListener('visibilitychange', hide);
		return () => {
			document.removeEventListener('visibilitychange', hide);
			const ending = saver.flush();
			void ending.finally(() => editor?.destroy());
		};
	});
</script>

{#snippet slashRow(item: SlashItem)}
	<span>{item.label}</span>
{/snippet}

{#snippet linkRow(item: LinkItem)}
	<span class="min-w-0 flex-1 truncate">{item.label}</span>
	<span class="shrink-0 text-sm text-muted">{item.hint}</span>
{/snippet}

<div bind:this={element} class="note-host min-h-48"></div>
<SuggestMenu menu={slash} label="Blocks" row={slashRow} />
<SuggestMenu menu={link} label="Link to" row={linkRow} />

<style>
	.note-host :global(.note-editor) {
		min-height: 12rem;
		outline: none;
		line-height: 1.6;
		padding-bottom: 6rem;
		-webkit-user-select: text;
		user-select: text;
	}
	.note-host :global(.note-editor > * + *) {
		margin-top: 0.5rem;
	}
	.note-host :global(h1) {
		font-size: 1.75rem;
		line-height: 1.25;
		font-weight: 600;
		margin-top: 1rem;
	}
	.note-host :global(h2) {
		font-size: 1.4rem;
		line-height: 1.3;
		font-weight: 600;
		margin-top: 0.75rem;
	}
	.note-host :global(h3) {
		font-size: 1.2rem;
		font-weight: 600;
	}
	.note-host :global(ul:not([data-type='taskList'])) {
		list-style: disc;
		padding-left: 1.5rem;
	}
	.note-host :global(ol) {
		list-style: decimal;
		padding-left: 1.5rem;
	}
	.note-host :global(ul[data-type='taskList']) {
		list-style: none;
		padding-left: 0;
	}
	.note-host :global(ul[data-type='taskList'] li) {
		display: flex;
		align-items: flex-start;
		gap: 0.6rem;
	}
	.note-host :global(ul[data-type='taskList'] li > label) {
		display: flex;
		min-height: 2.75rem;
		min-width: 1.75rem;
		align-items: flex-start;
		padding-top: 0.3rem;
	}
	.note-host :global(ul[data-type='taskList'] input[type='checkbox']) {
		width: 1.25rem;
		height: 1.25rem;
		accent-color: var(--accent);
	}
	.note-host :global(ul[data-type='taskList'] li > div) {
		flex: 1;
		min-width: 0;
	}
	.note-host :global(li[data-checked='true'] > div) {
		color: var(--muted);
		text-decoration: line-through;
	}
	.note-host :global(blockquote) {
		border-left: 3px solid var(--border-strong);
		padding-left: 0.9rem;
		color: var(--muted);
	}
	.note-host :global(hr) {
		border: none;
		border-top: 1px solid var(--border);
		margin: 1rem 0;
	}
	.note-host :global(hr.ProseMirror-selectednode) {
		border-top-color: var(--accent);
	}
	.note-host :global(a:not(.page-link)) {
		color: var(--accent);
		text-decoration: underline;
	}
	.note-host :global(.page-link) {
		color: var(--accent);
		background: color-mix(in oklch, var(--accent) 10%, transparent);
		border-radius: 0.25rem;
		padding: 0.05rem 0.3rem;
		cursor: pointer;
		text-decoration: none;
	}
	.note-host :global(.page-link.ProseMirror-selectednode) {
		outline: 2px solid var(--accent);
	}
	.note-host :global(p.is-editor-empty:first-child::before) {
		content: attr(data-placeholder);
		float: left;
		height: 0;
		color: var(--muted);
		pointer-events: none;
	}
</style>
