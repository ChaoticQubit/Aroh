<script lang="ts">
	import type { Snippet } from 'svelte';

	/**
	 * Labelled block shared by every component: a visible label, optional trailing detail, then content.
	 * `detailClass` recolours the detail, for example a status tone.
	 * `id` ties the label to a single control; omit it for groups, which use `aria-labelledby` instead.
	 */
	let { label, id, labelId, detail, detailClass, children }: { label: string; id?: string; labelId?: string; detail?: string; detailClass?: string; children: Snippet } = $props();
</script>

<div class="flex flex-col gap-2">
	<div class="flex items-baseline justify-between gap-3">
		{#if id}
			<label for={id} id={labelId} class="text-sm font-medium text-fg">{label}</label>
		{:else}
			<span id={labelId} class="text-sm font-medium text-fg">{label}</span>
		{/if}
		{#if detail}
			<span class="text-xs tabular-nums {detailClass ?? 'text-muted'}">{detail}</span>
		{/if}
	</div>
	{@render children()}
</div>
