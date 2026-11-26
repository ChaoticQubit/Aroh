<script lang="ts">
	import NotesHome from '$lib/ui/notes/NotesHome.svelte';
	import NotePage from '$lib/ui/notes/NotePage.svelte';
	import NotesUnavailable from '$lib/ui/notes/NotesUnavailable.svelte';
	import type { Block } from '$lib/app/blocks';
	import type { PageView, TreeRow } from '$lib/app/notebook';
	import { linkItems, linkLabel } from '$lib/app/notes';

	const today = '2026-11-26';
	const actions = [{ id: 'a1', title: 'Long run' }, { id: 'a2', title: 'Swim drills' }];
	const goals = [{ id: 'g1', title: 'Run a marathon' }];
	let rows = $state<TreeRow[]>([
		{ id: 'p1', kind: 'page', title: 'Reading list', parent_id: null, updated_at: '2026-11-25T10:00:00Z' },
		{ id: 'p2', kind: 'page', title: 'Novels', parent_id: 'p1', updated_at: '2026-11-24T10:00:00Z' },
		{ id: 'd1', kind: 'day', title: '2026-11-26', parent_id: null, updated_at: '2026-11-26T08:00:00Z' }
	]);
	let saved = $state<Block[]>([]);
	let saves = $state(0);
	let view = $state<PageView>({
		id: 'p1',
		kind: 'page',
		target: 'p1',
		title: 'Reading list',
		parent_id: null,
		blocks: [
			{ id: 'b1', type: 'heading', order: 'V', content: { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'This month' }] } },
			{ id: 'b2', type: 'paragraph', order: 'k', content: { type: 'paragraph', content: [{ type: 'text', text: 'Started on ' }, { type: 'pageLink', attrs: { target_kind: 'day', target: today } }] } }
		],
		children: [{ id: 'p2', title: 'Novels' }],
		backlinks: [{ id: 'd1', kind: 'day', title: '2026-11-26' }],
		updated_at: '2026-11-25T10:00:00Z'
	});
	let opened = $state('');
</script>

<div class="grid min-h-dvh gap-6 bg-surface p-4 text-fg lg:grid-cols-[24rem_1fr_24rem]">
	<section class="rounded-md border border-border bg-bg" aria-label="Notes tab">
		<NotesHome {rows} {today} search={async (q) => rows.filter((r) => r.title.toLowerCase().includes(q.toLowerCase())).map((r) => ({ id: r.id, kind: r.kind, title: r.title, snippet: '' }))} onnew={() => (rows = [...rows, { id: `p${rows.length + 1}`, kind: 'page', title: 'Untitled', parent_id: null, updated_at: new Date().toISOString() }])} />
	</section>
	<section class="rounded-md border border-border bg-bg" aria-label="Page">
		<NotePage
			{view}
			{rows}
			label={(k, t) => linkLabel(k, t, { rows, actions, goals })}
			links={async (q) => linkItems(q, { today, pages: rows.filter((r) => r.title.toLowerCase().includes(q.toLowerCase())).map((r) => ({ id: r.id, kind: r.kind, title: r.title, snippet: '' })), actions, goals })}
			onsave={async (blocks) => ((saved = blocks), saves++)}
			onrename={(title) => (view = { ...view, title })}
			onmove={(parent_id) => (view = { ...view, parent_id })}
			ondelete={() => (opened = 'deleted')}
			onaddsub={() => (opened = 'add subpage')}
			onopen={(k, t) => (opened = `${k}/${t}`)}
		/>
	</section>
	<section class="flex flex-col gap-3 rounded-md border border-border bg-bg p-4 text-sm" aria-label="Saved blocks">
		<p>Saves: <span data-testid="saves">{saves}</span>. Opened: <span data-testid="opened">{opened}</span></p>
		<pre data-testid="saved" class="overflow-auto text-xs whitespace-pre-wrap">{JSON.stringify(saved.map((b) => ({ id: b.id, type: b.type, order: b.order })), null, 1)}</pre>
		<NotesUnavailable />
	</section>
</div>
