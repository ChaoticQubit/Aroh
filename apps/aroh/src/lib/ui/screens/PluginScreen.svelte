<script lang="ts">
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import { Badge } from '$lib/ui/primitives/badge';
	import Fold from '../components/Fold.svelte';
	import type { PluginDetail } from '$lib/plugins/api';
	import { describeSections, trackerRows } from '$lib/plugins/describe';

	/**
	 * One plugin version, read-only: title, description, version (marked when it is its goal's
	 * active one), goal, trackers with fields and cadence, dashboard sections in plain words and
	 * a "Show source" fold with the raw bundle. An invalid bundle shows the validator's error.
	 */
	let { plugin, backLabel = 'Back', onback }: { plugin: PluginDetail; backLabel?: string; onback?: () => void } = $props();

	const tree = $derived(plugin.tree && typeof plugin.tree === 'object' ? (plugin.tree as Record<string, unknown>) : null);
	const description = $derived(typeof tree?.description === 'string' ? tree.description : '');
	const trackers = $derived(trackerRows(plugin.trackers));
	const sections = $derived(describeSections(plugin.tree));
	let source = $state(false);
</script>

<div class="flex w-full max-w-3xl flex-col gap-5 p-4 @3xl:p-6">
	{#if onback}
		<button type="button" class="-ml-2 flex h-11 w-fit items-center gap-1 rounded-sm px-2 text-sm text-muted outline-none hover:text-fg focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => onback?.()}>
			<ChevronLeft class="size-4" aria-hidden="true" />{backLabel}
		</button>
	{/if}
	<header class="flex flex-col gap-1">
		<h1 class="text-xl font-semibold break-words text-fg">{plugin.title}</h1>
		<p class="flex flex-wrap items-center gap-2 text-sm text-muted">
			<span>{plugin.name}, version {plugin.version}</span>
			{#if plugin.active}<Badge variant="outline">Active</Badge>{:else}<span>Not the active version</span>{/if}
		</p>
		<p class="text-sm text-muted">{plugin.goal_title ? `Goal: ${plugin.goal_title}` : 'Not tied to a goal'}</p>
		{#if description}<p class="text-sm break-words text-fg">{description}</p>{/if}
	</header>

	{#if plugin.error}
		<p class="rounded-sm border border-border p-3 text-sm break-words text-fg" role="alert">This version does not validate: {plugin.error}</p>
	{/if}

	<section class="flex flex-col gap-2" aria-labelledby="plugin-trackers">
		<h2 id="plugin-trackers" class="text-sm font-semibold text-fg">Trackers</h2>
		{#if trackers.length}
			<ul class="divide-y divide-border rounded-md border border-border bg-bg">
				{#each trackers as t (t.name)}
					<li class="flex flex-col gap-0.5 px-4 py-3">
						<span class="text-sm font-medium break-words">{t.name}</span>
						<span class="text-sm break-words text-muted">{t.fields.join(', ')}</span>
						<span class="text-xs text-muted">{t.cadence}</span>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="text-sm text-muted">This plugin adds no trackers.</p>
		{/if}
	</section>

	<section class="flex flex-col gap-2" aria-labelledby="plugin-sections">
		<h2 id="plugin-sections" class="text-sm font-semibold text-fg">Dashboard</h2>
		{#if sections.length}
			{#each sections as s, i (i)}
				<div class="flex flex-col gap-1 rounded-md border border-border bg-bg px-4 py-3">
					<h3 class="text-sm font-medium break-words">{s.title}</h3>
					{#if s.items.length}
						<ul class="flex list-disc flex-col gap-0.5 pl-5 text-sm text-muted">
							{#each s.items as item, j (j)}<li class="break-words">{item}</li>{/each}
						</ul>
					{:else}
						<p class="text-sm text-muted">No items.</p>
					{/if}
				</div>
			{/each}
		{:else}
			<p class="text-sm text-muted">No dashboard sections.</p>
		{/if}
	</section>

	<Fold title="Show source" summary="The plugin file as the agent wrote it." bind:open={source}>
		<pre class="rounded-sm bg-surface-2 p-3 font-mono text-xs break-words whitespace-pre-wrap text-fg" aria-label="Plugin source">{plugin.bundle_toml}</pre>
	</Fold>
</div>
