<script lang="ts">
	import { page } from '$app/state';
	import type { PageKind } from '$lib/app/notebook';
	import PageLoader from '$lib/ui/notes/PageLoader.svelte';
	import DayRoute from '$lib/ui/diary/DayRoute.svelte';

	const KINDS: PageKind[] = ['page', 'day', 'action', 'goal'];
	const kind = $derived(KINDS.find((k) => k === page.params.kind) ?? null);
	const target = $derived(page.params.target ?? '');
</script>

{#if kind && target}
	{#key `${kind}/${target}`}
		{#if kind === 'day'}
			<DayRoute day={target} />
		{:else}
			<PageLoader {kind} {target} />
		{/if}
	{/key}
{:else}
	<p role="alert" class="p-4 text-sm text-status-missed">This page link is not valid.</p>
{/if}
