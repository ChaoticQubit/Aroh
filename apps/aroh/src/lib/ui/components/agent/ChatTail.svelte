<script lang="ts">
	import { Button } from '$lib/ui/primitives/button';
	import type { ChatTail } from '$lib/app/working';

	/**
	 * The end of every chat thread (Q87): the working bubble, three dots (still under reduced motion)
	 * with the current step when known, or a send that failed, with its error and Retry (`onretry`).
	 * Screen readers hear it from the chat's own live region, so the visual bubble stays hidden from them.
	 */
	let { tail, onretry }: { tail: ChatTail; onretry?: () => void } = $props();
</script>

{#if tail.type === 'typing'}
	<div data-working class="flex max-w-[92%] items-center gap-2.5 self-start rounded-sm border border-border bg-surface px-3 py-2.5" aria-hidden="true">
		<span class="flex items-center gap-1">
			{#each [0, 1, 2] as i (i)}<span class="dot size-2 rounded-full bg-muted" style="animation-delay: {i * 160}ms"></span>{/each}
		</span>
		{#if tail.step}<span class="text-sm text-muted">{tail.step}</span>{/if}
	</div>
{:else}
	<div data-failed class="flex items-center gap-3 rounded-sm border border-status-missed/40 bg-surface px-3 py-2">
		<p class="min-w-0 flex-1 text-sm text-fg">Not sent. {tail.text}</p>
		<Button size="touch" variant="outline" onclick={() => onretry?.()}>Retry</Button>
	</div>
{/if}

<style>
	.dot {
		animation: dot 1.2s ease-in-out infinite;
	}
	@keyframes dot {
		0%,
		80%,
		100% {
			opacity: 0.35;
			transform: translateY(0);
		}
		40% {
			opacity: 1;
			transform: translateY(-3px);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.dot {
			animation: none;
			opacity: 0.7;
		}
	}
</style>
