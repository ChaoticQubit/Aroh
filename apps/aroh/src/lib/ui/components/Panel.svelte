<script lang="ts">
	import type { Snippet } from 'svelte';
	import * as Card from '$lib/ui/primitives/card';

	/**
	 * Hairline frame shared by display and planning components: a title row with an optional
	 * trailing slot, then the body.
	 */
	let { title, aside, children }: { title: string; aside?: Snippet; children: Snippet } = $props();
</script>

<Card.Root size="sm" class="gap-3">
	<Card.Header class="flex items-center justify-between gap-3">
		<Card.Title class="text-sm font-medium text-fg">{title}</Card.Title>
		{#if aside}
			<div class="text-xs text-muted tabular-nums">{@render aside()}</div>
		{/if}
	</Card.Header>
	<Card.Content class="flex flex-col gap-3">
		{@render children()}
	</Card.Content>
</Card.Root>
