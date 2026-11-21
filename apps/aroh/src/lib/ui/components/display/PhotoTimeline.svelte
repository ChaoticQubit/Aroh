<script lang="ts">
	import ImageIcon from '@lucide/svelte/icons/image';
	import Panel from '../Panel.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDay } from '$lib/ui/format';
	import type { SpecOf } from '$lib/ui/contract';

	/** Progress photos in a horizontal strip, oldest first, each captioned with its day. */
	let { spec }: { spec: SpecOf<'photo_timeline'> } = $props();

	const { zone } = getClock();
</script>

<Panel title={spec.label}>
	{#snippet aside()}<span>{spec.photos.length} photos</span>{/snippet}
	{#if spec.photos.length}
		<ul class="-mx-1 flex snap-x gap-2 overflow-x-auto px-1 pb-1">
			{#each spec.photos as p (p.at)}
				<li class="flex w-24 shrink-0 snap-start flex-col gap-1">
					{#if p.src}
						<img src={p.src} alt="Progress photo, {formatDay(p.at, zone)}" class="aspect-[3/4] w-full rounded-sm border border-border object-cover" />
					{:else}
						<div class="flex aspect-[3/4] w-full items-center justify-center rounded-sm border border-border bg-surface text-muted" role="img" aria-label="Photo placeholder, {formatDay(p.at, zone)}">
							<ImageIcon class="size-4" />
						</div>
					{/if}
					<span class="text-xs text-muted">{formatDay(p.at, zone)}</span>
				</li>
			{/each}
		</ul>
	{:else}
		<p class="text-sm text-muted">No photos yet</p>
	{/if}
</Panel>
