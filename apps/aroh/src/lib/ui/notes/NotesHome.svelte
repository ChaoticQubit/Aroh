<script lang="ts">
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import FileText from '@lucide/svelte/icons/file-text';
	import Plus from '@lucide/svelte/icons/plus';
	import Search from '@lucide/svelte/icons/search';
	import { pageHref, pageTree, recentPages, type TreeNode } from '$lib/app/notes';
	import type { PageKind, SearchHit, TreeRow } from '$lib/app/notebook';

	/**
	 * The Notes tab (Q96): search, today's page, recent pages, the page tree with collapsible
	 * parents and New page. `rows` is the flat page list, `search` finds pages by text, `onnew`
	 * creates a top-level page. Rows open through plain links.
	 */
	let { rows, today, busy = false, search, onnew }: { rows: TreeRow[]; today: string; busy?: boolean; search: (query: string) => Promise<SearchHit[]>; onnew: () => void } = $props();

	const RECENT = 5;
	const SEARCH_MS = 250;
	const KIND: Record<PageKind, string> = { page: 'Page', day: 'Day', action: 'Action', goal: 'Goal' };
	const tree = $derived(pageTree(rows));
	const recent = $derived(recentPages(rows, RECENT));
	let query = $state('');
	let hits = $state<SearchHit[] | null>(null);
	let open = $state<string[]>([]);
	let timer: ReturnType<typeof setTimeout> | undefined;
	let asked = 0;

	/** Searches after typing pauses; only the newest answer is shown. */
	function onquery() {
		clearTimeout(timer);
		const q = query.trim();
		if (!q) return void (hits = null);
		timer = setTimeout(async () => {
			const mine = ++asked;
			const found = await search(q).catch(() => []);
			if (mine === asked) hits = found;
		}, SEARCH_MS);
	}

	const ROW = 'flex min-h-11 min-w-0 flex-1 items-center gap-3 rounded-sm px-2 text-md text-fg no-underline outline-none hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50';
</script>

{#snippet node(n: TreeNode, depth: number)}
	{@const expanded = open.includes(n.id)}
	<li>
		<div class="flex items-center" style="padding-left: {depth * 1.25}rem">
			{#if n.children.length}
				<button type="button" class="flex size-11 shrink-0 items-center justify-center rounded-sm text-muted outline-none focus-visible:ring-3 focus-visible:ring-ring/50" aria-expanded={expanded} aria-label="{expanded ? 'Collapse' : 'Expand'} {n.title || 'Untitled'}" onclick={() => (open = expanded ? open.filter((o) => o !== n.id) : [...open, n.id])}>
					<ChevronRight class="size-4 transition-transform {expanded ? 'rotate-90' : ''}" aria-hidden="true" />
				</button>
			{:else}
				<span class="size-11 shrink-0" aria-hidden="true"></span>
			{/if}
			<a href={pageHref('page', n.id)} class={ROW}><span class="truncate">{n.title || 'Untitled'}</span>{#if n.kind !== 'page'}<span class="shrink-0 text-sm text-muted">{KIND[n.kind]}</span>{/if}</a>
		</div>
		{#if expanded}
			<ul>{#each n.children as c (c.id)}{@render node(c, depth + 1)}{/each}</ul>
		{/if}
	</li>
{/snippet}

<div class="mx-auto flex w-full max-w-2xl flex-col gap-5 p-4">
	<header class="flex items-center justify-between gap-3">
		<h2 class="text-2xl leading-tight font-semibold">Notes</h2>
		<button type="button" class="flex min-h-11 items-center gap-2 rounded-sm bg-fg px-4 text-sm font-medium text-bg outline-none focus-visible:ring-3 focus-visible:ring-ring/50 disabled:opacity-60" disabled={busy} onclick={onnew}><Plus class="size-4" aria-hidden="true" />New page</button>
	</header>

	<label class="flex items-center gap-2 rounded-sm border border-border bg-bg px-3 focus-within:ring-3 focus-within:ring-ring/50">
		<Search class="size-4 shrink-0 text-muted" aria-hidden="true" />
		<span class="sr-only">Search pages</span>
		<input type="search" class="min-h-11 w-full bg-transparent text-lg outline-none" placeholder="Search pages" autocomplete="off" bind:value={query} oninput={onquery} />
	</label>

	{#if hits}
		<section aria-label="Search results" class="flex flex-col gap-1">
			{#each hits as h (h.id)}
				<a href={pageHref('page', h.id)} class="{ROW} flex-col items-start justify-center py-2">
					<span class="flex w-full items-center gap-2"><span class="truncate font-medium">{h.title || 'Untitled'}</span><span class="ml-auto shrink-0 text-sm text-muted">{KIND[h.kind]}</span></span>
					{#if h.snippet}<span class="line-clamp-2 text-sm text-muted">{h.snippet}</span>{/if}
				</a>
			{:else}
				<p class="px-2 text-sm text-muted">No pages match.</p>
			{/each}
		</section>
	{:else}
		<a href={pageHref('day', today)} class="{ROW} flex-none border border-border bg-bg px-3"><FileText class="size-4 text-muted" aria-hidden="true" />Today's page<span class="ml-auto text-sm text-muted">{today}</span></a>

		{#if recent.length}
			<section aria-labelledby="notes-recent" class="flex flex-col gap-1">
				<h3 id="notes-recent" class="px-2 text-sm font-semibold text-muted">Recent</h3>
				<ul>
					{#each recent as r (r.id)}
						<li><a href={pageHref('page', r.id)} class={ROW}><span class="truncate">{r.title || 'Untitled'}</span><span class="ml-auto shrink-0 text-sm text-muted">{KIND[r.kind]}</span></a></li>
					{/each}
				</ul>
			</section>
		{/if}

		<section aria-labelledby="notes-pages" class="flex flex-col gap-1">
			<h3 id="notes-pages" class="px-2 text-sm font-semibold text-muted">Pages</h3>
			{#if tree.length}
				<ul>{#each tree as n (n.id)}{@render node(n, 0)}{/each}</ul>
			{:else}
				<p class="px-2 text-sm text-muted">No pages yet. Start one with New page.</p>
			{/if}
		</section>
	{/if}
</div>
