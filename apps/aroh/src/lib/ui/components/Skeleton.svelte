<script lang="ts">
	/**
	 * Placeholder bars for content still loading (B87): `lines` pulsing bars, the last one shorter,
	 * announced once as `label` to screen readers. Never shows data, so nothing false renders.
	 */
	let { lines = 2, label = 'Loading' }: { lines?: number; label?: string } = $props();
	const count = $derived(Math.max(1, lines));
</script>

<div class="flex flex-col gap-2" role="status" aria-label={label}>
	{#each { length: count } as _, i (i)}
		<div class="h-4 animate-pulse rounded-sm bg-border motion-reduce:animate-none {i === count - 1 && count > 1 ? 'w-2/3' : 'w-full'}" aria-hidden="true"></div>
	{/each}
</div>
