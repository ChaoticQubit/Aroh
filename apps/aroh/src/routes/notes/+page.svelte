<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import NotesHome from '$lib/ui/notes/NotesHome.svelte';
	import NotesUnavailable from '$lib/ui/notes/NotesUnavailable.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatYmd } from '$lib/ui/format';
	import { isUnavailable } from '$lib/app/diary';
	import * as notebook from '$lib/app/notebook';
	import { pageHref, UNTITLED } from '$lib/app/notes';

	type Load = { kind: 'loading' } | { kind: 'unavailable' } | { kind: 'error'; message: string } | { kind: 'ready'; rows: notebook.TreeRow[] };

	const clock = getClock();
	const today = formatYmd(clock.now, clock.zone);
	let load = $state<Load>({ kind: 'loading' });
	let busy = $state(false);
	let failed = $state<string | null>(null);

	onMount(async () => {
		try {
			load = { kind: 'ready', rows: await notebook.pagesTree() };
		} catch (e) {
			load = isUnavailable(e) ? { kind: 'unavailable' } : { kind: 'error', message: String(e) };
		}
	});

	/** Creates an empty top-level page and opens it. */
	async function onnew() {
		busy = true;
		try {
			const p = await notebook.pageCreate(null, UNTITLED);
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
	{#if failed}<p role="alert" class="px-4 pt-3 text-sm text-status-missed">{failed}</p>{/if}
	<NotesHome rows={load.rows} {today} {busy} search={notebook.pagesSearch} {onnew} />
{/if}
