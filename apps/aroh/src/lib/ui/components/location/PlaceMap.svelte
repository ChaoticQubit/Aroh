<script lang="ts">
	/**
	 * Neutral street sketch with place pins on a 360 by 200 grid, drawn in tokens only.
	 * `pins` are positions on that grid; `ring` draws the 150 m arrival radius around the first pin.
	 * `image` is a MapKit snapshot (a data URL) shown instead of the sketch when the device has one.
	 */
	let {
		label,
		pins,
		ring = false,
		height = 160,
		image
	}: { label: string; pins: { x: number; y: number; name: string }[]; ring?: boolean; height?: number; image?: string } = $props();

	const STREETS = ['M0 60 H360', 'M0 140 H360', 'M70 0 V200', 'M190 0 V200', 'M300 0 V200', 'M0 190 L120 0'];
</script>

<figure class="overflow-hidden rounded-sm border border-border bg-surface-2" style="height: {height}px">
	{#if image}
		<img src={image} alt={label} class="size-full object-cover" />
	{:else}
		<svg viewBox="0 0 360 200" preserveAspectRatio="xMidYMid slice" class="size-full" role="img" aria-label={label}>
			{#each STREETS as d (d)}
				<path {d} fill="none" stroke="var(--bg)" stroke-width="10" />
				<path {d} fill="none" stroke="var(--border)" stroke-width="1" />
			{/each}
			{#if ring && pins[0]}
				<circle cx={pins[0].x} cy={pins[0].y} r="42" fill="none" stroke="var(--border-strong)" stroke-width="1" stroke-dasharray="4 3" />
			{/if}
			{#each pins as p, i (i)}
				<circle cx={p.x} cy={p.y} r={i === 0 ? 7 : 5} fill="var(--fg)" stroke="var(--bg)" stroke-width="2" />
			{/each}
		</svg>
	{/if}
	<figcaption class="sr-only">{pins.map((p) => p.name).join(', ')}</figcaption>
</figure>
