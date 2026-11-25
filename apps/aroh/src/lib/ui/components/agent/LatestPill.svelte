<script lang="ts">
	import ArrowDown from '@lucide/svelte/icons/arrow-down';

	/** Floating pill (Q87) above a chat composer while the agents work and the owner scrolled up; `onjump` scrolls to the newest entry. */
	let { onjump }: { onjump: () => void } = $props();
</script>

<button
	type="button"
	data-latest
	class="absolute bottom-full left-1/2 mb-2 flex h-11 -translate-x-1/2 items-center gap-2 rounded-full bg-fg px-4 text-sm font-medium whitespace-nowrap text-bg shadow-md outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
	onclick={onjump}><ArrowDown class="size-4" />Working, jump to latest</button
>
