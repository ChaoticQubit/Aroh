<script lang="ts">
	import { untrack, type Snippet } from 'svelte';
	import { setClock } from '$lib/ui/clock';

	/** Gives one mockup frame its own fixed clock, so a frame can show another day as today. */
	let { now, zone, children }: { now: string; zone: string; children: Snippet } = $props();

	setClock(untrack(() => ({ now, zone })));
</script>

{@render children()}
