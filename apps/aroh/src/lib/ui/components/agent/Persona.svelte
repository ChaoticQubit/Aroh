<script lang="ts">
	/** Persona name with an optional kind note, the byline of every agent surface. */
	let { name, note }: { name: string; note?: string } = $props();
</script>

<p class="flex items-center gap-1.5 text-xs">
	<span class="font-medium text-fg">{name}</span>
	{#if note}<span class="text-muted">{note}</span>{/if}
</p>
