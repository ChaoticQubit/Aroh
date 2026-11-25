<script lang="ts">
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import { Badge } from '$lib/ui/primitives/badge';
	import type { CoachGroup, CoachView } from './types';
	import type { PluginRow } from '$lib/plugins/describe';

	/**
	 * Coach library: with `goalTitle`, the coaches working on that goal first, then one hairline list
	 * per team with its category dot, each row the coach's name, style line and a "Your copy" badge.
	 * `onopen` gets the coach's slug, `onback` returns to where the owner came from. `status` shows above.
	 * A Plugins group lists every live plugin from `plugins`, each linking to its page; null while loading.
	 */
	let {
		groups,
		coaches = [],
		goalTitle = null,
		status = null,
		plugins = [],
		pluginStatus = null,
		backLabel = 'Settings',
		onopen,
		onback
	}: {
		groups: CoachGroup[];
		coaches?: CoachView[];
		goalTitle?: string | null;
		status?: string | null;
		plugins?: PluginRow[] | null;
		pluginStatus?: string | null;
		backLabel?: string;
		onopen?: (name: string) => void;
		onback?: () => void;
	} = $props();
</script>

{#snippet rows(list: CoachView[], label: string)}
	<ul class="divide-y divide-border overflow-hidden rounded-md border border-border bg-bg" aria-label={label}>
		{#each list as p (p.name)}
			<li>
				<button type="button" class="flex min-h-14 w-full items-center gap-3 px-4 py-2 text-left outline-none hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset" onclick={() => onopen?.(p.name)}>
					<span class="flex min-w-0 flex-1 flex-col">
						<span class="flex items-center gap-2">
							<span class="truncate text-sm font-medium">{p.display_name}</span>
							{#if p.source === 'library'}<Badge variant="outline">Your copy</Badge>{/if}
						</span>
						<span class="line-clamp-2 text-xs text-muted">{p.role}</span>
					</span>
					<ChevronRight class="size-4 shrink-0 text-muted" aria-hidden="true" />
				</button>
			</li>
		{/each}
	</ul>
{/snippet}

<div class="flex w-full max-w-3xl flex-col gap-5 p-4 @3xl:p-6">
	{#if onback}
		<button type="button" class="-ml-2 flex h-11 w-fit items-center gap-1 rounded-sm px-2 text-sm text-muted outline-none hover:text-fg focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => onback?.()}>
			<ChevronLeft class="size-4" aria-hidden="true" />{backLabel}
		</button>
	{/if}
	{#if status}<p class="text-sm text-muted" role="status">{status}</p>{/if}
	{#if goalTitle !== null}
		<section class="flex flex-col gap-2" aria-label="Coaches on this goal">
			<h2 class="text-sm font-semibold text-fg">Coaches on {goalTitle}</h2>
			{#if coaches.length}
				{@render rows(coaches, `Coaches on ${goalTitle}`)}
			{:else}
				<p class="text-sm text-muted">No coaches have worked on this goal yet. They join when the goal is planned.</p>
			{/if}
		</section>
	{/if}
	<p class="text-sm text-muted">Built-in coaches are read-only. Copy one to change its name, style, instructions and rules; later runs use your copy.</p>
	{#each groups as g (g.title)}
		<section class="flex flex-col gap-2" aria-label="{g.title} coaches">
			<h2 class="flex items-center gap-2 text-sm font-semibold text-fg">
				{#if g.slot !== null}<span class="size-2 shrink-0 rounded-full" style="background: var(--cat-{g.slot})" aria-hidden="true"></span>{/if}{g.title}
				<span class="text-xs font-normal text-muted tabular-nums">{g.personas.length}</span>
			</h2>
			{@render rows(g.personas, `${g.title} coaches`)}
		</section>
	{/each}
	<section class="flex flex-col gap-2" aria-labelledby="plugins-head">
		<h2 id="plugins-head" class="flex items-center gap-2 text-sm font-semibold text-fg">
			Plugins
			{#if plugins?.length}<span class="text-xs font-normal text-muted tabular-nums">{plugins.length}</span>{/if}
		</h2>
		{#if pluginStatus}<p class="text-sm text-muted" role="status">{pluginStatus}</p>{/if}
		{#if plugins === null}
			<p class="text-sm text-muted" role="status">Loading plugins.</p>
		{:else if plugins.length}
			<ul class="divide-y divide-border overflow-hidden rounded-md border border-border bg-bg" aria-label="Plugins">
				{#each plugins as p (p.id)}
					<li>
						<a href={p.href} class="flex min-h-14 w-full items-center gap-3 px-4 py-2 text-left text-fg no-underline outline-none hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset">
							<span class="flex min-w-0 flex-1 flex-col">
								<span class="truncate text-sm font-medium">{p.title}</span>
								<span class="line-clamp-2 text-xs text-muted">{p.note}</span>
							</span>
							<ChevronRight class="size-4 shrink-0 text-muted" aria-hidden="true" />
						</a>
					</li>
				{/each}
			</ul>
		{:else if !pluginStatus}
			<p class="text-sm text-muted">No plugins yet. An agent writes one when it plans a goal that needs its own dashboard.</p>
		{/if}
	</section>
</div>
