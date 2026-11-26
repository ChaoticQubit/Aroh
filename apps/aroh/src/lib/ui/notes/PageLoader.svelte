<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { goto } from '$app/navigation';
	import NotePage from '$lib/ui/notes/NotePage.svelte';
	import NotesUnavailable from '$lib/ui/notes/NotesUnavailable.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatYmd } from '$lib/ui/format';
	import { data } from '$lib/app/data.svelte';
	import { isUnavailable } from '$lib/app/diary';
	import * as notebook from '$lib/app/notebook';
	import { linkItems, linkLabel, pageHref, recentPages, UNTITLED } from '$lib/app/notes';
	import type { Block } from '$lib/app/blocks';

	/**
	 * Opens the page for `kind` and `target` and runs its commands; mounted once per page.
	 * `embedded` drops the page's own header when a screen above it already names the page.
	 */
	let { kind, target, embedded = false }: { kind: notebook.PageKind; target: string; embedded?: boolean } = $props();

	type Load = { kind: 'loading' } | { kind: 'unavailable' } | { kind: 'error'; message: string } | { kind: 'ready'; view: notebook.PageView };

	const RECENT_LINKS = 5;
	const clock = getClock();
	const today = formatYmd(clock.now, clock.zone);
	let load = $state<Load>({ kind: 'loading' });
	let rows = $state<notebook.TreeRow[]>([]);
	let busy = $state(false);
	let failed = $state<string | null>(null);
	let remote = $state<{ blocks: Block[] } | null>(null);
	const actions = $derived.by(() => {
		const now = Date.parse(clock.now);
		return [...(data.snapshot?.actions ?? [])].sort((a, b) => Math.abs(Date.parse(a.start_at) - now) - Math.abs(Date.parse(b.start_at) - now));
	});
	const goals = $derived((data.snapshot?.goals ?? []).filter((g) => g.status !== 'archived'));

	onMount(() => {
		void open();
		const unlisten = listen('aroh://changed', () => void reread());
		return () => void unlisten.then((off) => off()).catch(() => {});
	});

	/** Opens the page and the page tree. */
	async function open() {
		try {
			const [view, tree] = await Promise.all([notebook.pageGet(kind, target), notebook.pagesTree().catch(() => [])]);
			rows = tree;
			load = { kind: 'ready', view };
		} catch (e) {
			load = isUnavailable(e) ? { kind: 'unavailable' } : { kind: 'error', message: String(e) };
		}
	}

	/** After a sync, reads the page again and hands its blocks to the editor to merge before any save. */
	async function reread() {
		if (load.kind !== 'ready') return;
		const id = load.view.id;
		const fresh = await notebook.pageGet('page', id).catch(() => null);
		if (!fresh || load.kind !== 'ready' || load.view.id !== id) return;
		load = { kind: 'ready', view: { ...fresh, blocks: load.view.blocks } };
		remote = { blocks: fresh.blocks };
		rows = await notebook.pagesTree().catch(() => rows);
	}

	/** The [[ suggestions: search hits for typed text, recent pages before anything is typed. */
	async function links(query: string) {
		const q = query.trim();
		const pages = q ? await notebook.pagesSearch(q).catch(() => []) : recentPages(rows, RECENT_LINKS).map((r) => ({ id: r.id, kind: r.kind, title: r.title, snippet: '' }));
		return linkItems(q, { today, pages, actions, goals });
	}

	/** Runs one page command, shows its error, and keeps the page and tree current. */
	async function run(call: () => Promise<notebook.PageView | null>) {
		busy = true;
		failed = null;
		try {
			const view = await call();
			if (view && load.kind === 'ready') load = { kind: 'ready', view: { ...view, blocks: load.view.blocks } };
			rows = await notebook.pagesTree().catch(() => rows);
		} catch (e) {
			failed = String(e);
		} finally {
			busy = false;
		}
	}

	/** Creates a subpage of `parent` and opens it. */
	async function addSub(parent: string) {
		busy = true;
		try {
			const p = await notebook.pageCreate(parent, UNTITLED);
			await goto(pageHref('page', p.id));
		} catch (e) {
			failed = String(e);
		} finally {
			busy = false;
		}
	}
</script>

{#if load.kind === 'unavailable'}
	<NotesUnavailable />
{:else if load.kind === 'error'}
	<p role="alert" class="p-4 text-sm text-status-missed">{load.message}</p>
{:else if load.kind === 'ready'}
	{@const id = load.view.id}
	{#if failed}<p role="alert" class="px-4 pt-3 text-sm text-status-missed">{failed}</p>{/if}
	<NotePage
		view={load.view}
		{remote}
		{rows}
		header={!embedded}
		{busy}
		label={(k, t) => linkLabel(k, t, { rows, actions, goals })}
		{links}
		onsave={(blocks: Block[]) => notebook.pageSave(id, blocks)}
		onerror={(e) => (failed = `Not saved yet: ${String(e)}`)}
		onrename={(title) => void run(() => notebook.pageRename(id, title))}
		onmove={(parent) => void run(() => notebook.pageMove(id, parent))}
		ondelete={() => void notebook.pageDelete(id).then(() => goto('/notes', { replaceState: true }), (e) => (failed = String(e)))}
		onaddsub={() => void addSub(id)}
		onopen={(k, t) => void goto(pageHref(k, t))}
	/>
{/if}
