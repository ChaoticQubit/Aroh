<script lang="ts">
	import Ellipsis from '@lucide/svelte/icons/ellipsis';
	import FileText from '@lucide/svelte/icons/file-text';
	import Link2 from '@lucide/svelte/icons/link-2';
	import Plus from '@lucide/svelte/icons/plus';
	import * as Sheet from '$lib/ui/primitives/sheet';
	import NoteEditor from './NoteEditor.svelte';
	import type { Block } from '$lib/app/blocks';
	import { moveTargets, pageHref, UNTITLED, type LinkItem } from '$lib/app/notes';
	import { onMount, tick } from 'svelte';
	import type { PageKind, PageView, TreeRow } from '$lib/app/notebook';

	/**
	 * One notebook page (Q96): title, block editor, subpages with Add subpage, and backlinks. A
	 * free page also has a menu with Rename, Move (pick a parent) and Delete with confirm. `rows`
	 * lists the pages a move can pick; `remote` carries the page after a sync into the editor;
	 * `header` false hides the kind and title row for a screen that names the page itself; the
	 * callbacks run the page commands.
	 */
	let {
		view,
		remote = null,
		rows,
		header = true,
		busy = false,
		label,
		links,
		onsave,
		onerror,
		onrename,
		onmove,
		ondelete,
		onaddsub,
		onopen
	}: {
		view: PageView;
		remote?: { blocks: Block[] } | null;
		rows: TreeRow[];
		header?: boolean;
		busy?: boolean;
		label: (kind: PageKind, target: string) => string;
		links: (query: string) => Promise<LinkItem[]>;
		onsave: (blocks: Block[]) => Promise<unknown>;
		onerror?: (error: unknown) => void;
		onrename: (title: string) => void;
		onmove: (parentId: string | null) => void;
		ondelete: () => void;
		onaddsub: () => void;
		onopen: (kind: PageKind, target: string) => void;
	} = $props();

	const KIND: Record<PageKind, string> = { page: 'Page', day: 'Day page', action: 'Action page', goal: 'Goal page' };
	const PANEL = 'flex max-h-[80dvh] flex-col gap-2 overflow-y-auto rounded-t-[var(--radius-dialog)] border-t border-border bg-bg pt-2 pr-[max(1rem,env(safe-area-inset-right))] pb-[max(1rem,env(safe-area-inset-bottom))] pl-[max(1rem,env(safe-area-inset-left))] text-fg shadow-none';
	const ITEM = 'flex min-h-11 w-full items-center gap-3 rounded-sm px-3 text-left text-md outline-none hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50';
	const free = $derived(view.kind === 'page');
	let title = $state('');
	let titleInput = $state<HTMLInputElement | null>(null);
	let sheet = $state<'menu' | 'move' | 'delete' | null>(null);
	const targets = $derived(moveTargets(rows, view.id));
	$effect(() => {
		title = view.title;
	});

	onMount(() => {
		if (free && view.title === UNTITLED && !view.blocks.length) void tick().then(() => titleInput?.select());
	});

	/** Renames the page when the edited title differs. */
	function commitTitle() {
		const next = title.trim();
		if (next && next !== view.title) onrename(next);
		else title = view.title;
	}
</script>

<article class="mx-auto flex w-full max-w-2xl flex-col gap-4 p-4">
	{#if header}
		<header class="flex items-start gap-2">
			<div class="flex min-w-0 flex-1 flex-col gap-1">
				<p class="text-sm text-muted">{KIND[view.kind]}</p>
				{#if free}
					<input bind:this={titleInput} class="w-full bg-transparent text-2xl leading-tight font-semibold outline-none placeholder:text-muted" placeholder="Untitled" aria-label="Page title" bind:value={title} onblur={commitTitle} onkeydown={(e) => e.key === 'Enter' && (e.preventDefault(), titleInput?.blur())} />
				{:else}
					<h2 class="text-2xl leading-tight font-semibold">{view.title || label(view.kind, view.target)}</h2>
				{/if}
			</div>
			{#if free}
				<button type="button" class="flex size-11 shrink-0 items-center justify-center rounded-sm text-muted outline-none hover:text-fg focus-visible:ring-3 focus-visible:ring-ring/50" aria-label="Page menu" aria-haspopup="dialog" onclick={() => (sheet = 'menu')}><Ellipsis class="size-5" aria-hidden="true" /></button>
			{/if}
		</header>
	{:else}
		<h3 class="border-t border-border px-1 pt-4 text-sm font-semibold text-muted">{KIND[view.kind]}</h3>
	{/if}

	{#key view.id}
		<NoteEditor blocks={view.blocks} {remote} {label} {links} {onsave} {onerror} {onopen} />
	{/key}

	<section aria-labelledby="note-subpages" class="flex flex-col gap-1 border-t border-border pt-4">
		<h3 id="note-subpages" class="px-1 text-sm font-semibold text-muted">Subpages</h3>
		{#if view.children.length}
			<ul>
				{#each view.children as c (c.id)}
					<li><a href={pageHref('page', c.id)} class="{ITEM} text-fg no-underline"><FileText class="size-4 shrink-0 text-muted" aria-hidden="true" /><span class="truncate">{c.title || 'Untitled'}</span></a></li>
				{/each}
			</ul>
		{/if}
		<button type="button" class="{ITEM} text-muted disabled:opacity-60" disabled={busy} onclick={onaddsub}><Plus class="size-4 shrink-0" aria-hidden="true" />Add subpage</button>
	</section>

	<section aria-labelledby="note-backlinks" class="flex flex-col gap-1 border-t border-border pt-4">
		<h3 id="note-backlinks" class="px-1 text-sm font-semibold text-muted">Backlinks</h3>
		{#if view.backlinks.length}
			<ul>
				{#each view.backlinks as b (b.id)}
					<li><a href={pageHref('page', b.id)} class="{ITEM} text-fg no-underline"><Link2 class="size-4 shrink-0 text-muted" aria-hidden="true" /><span class="truncate">{b.title || 'Untitled'}</span><span class="ml-auto shrink-0 text-sm text-muted">{KIND[b.kind]}</span></a></li>
				{/each}
			</ul>
		{:else}
			<p class="px-1 text-sm text-muted">No pages link here yet. Type [[ on another page to link it.</p>
		{/if}
	</section>
</article>

{#if sheet}
	<Sheet.Root open={true} onOpenChange={(open) => !open && (sheet = null)}>
		<Sheet.Content side="bottom" showCloseButton={false} aria-describedby={undefined} class={PANEL}>
			<span class="mx-auto h-1 w-9 shrink-0 rounded-full bg-border-strong" aria-hidden="true"></span>
			{#if sheet === 'menu'}
				<Sheet.Title class="sr-only">Page menu</Sheet.Title>
				<button type="button" class={ITEM} onclick={() => ((sheet = null), setTimeout(() => titleInput?.focus(), 0))}>Rename</button>
				<button type="button" class={ITEM} onclick={() => (sheet = 'move')}>Move</button>
				<button type="button" class="{ITEM} text-status-missed" onclick={() => (sheet = 'delete')}>Delete</button>
			{:else if sheet === 'move'}
				<Sheet.Title class="px-3 pt-1 text-md font-semibold">Move "{view.title || 'Untitled'}" under</Sheet.Title>
				<button type="button" class="{ITEM} disabled:text-muted" disabled={view.parent_id === null} onclick={() => ((sheet = null), onmove(null))}>Top level</button>
				{#each targets as t (t.id)}
					<button type="button" class="{ITEM} disabled:text-muted" disabled={view.parent_id === t.id} onclick={() => ((sheet = null), onmove(t.id))}><span class="truncate">{t.title || 'Untitled'}</span>{#if t.kind !== 'page'}<span class="ml-auto shrink-0 text-sm text-muted">{KIND[t.kind]}</span>{/if}</button>
				{/each}
			{:else}
				<Sheet.Title class="px-3 pt-1 text-md font-semibold">Delete "{view.title || 'Untitled'}"?</Sheet.Title>
				<p class="px-3 text-sm text-muted">The page and its text are removed. This cannot be undone.</p>
				<div class="flex gap-2 pt-2">
					<button type="button" class="min-h-11 flex-1 rounded-sm border border-border bg-bg text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => (sheet = null)}>Cancel</button>
					<button type="button" class="min-h-11 flex-1 rounded-sm bg-status-missed text-sm font-medium text-bg outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => ((sheet = null), ondelete())}>Delete page</button>
				</div>
			{/if}
		</Sheet.Content>
	</Sheet.Root>
{/if}
