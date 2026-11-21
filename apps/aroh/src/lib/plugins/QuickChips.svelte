<script lang="ts">
	import type { LogHandler, PropValue, RenderTracker } from './types';
	import { showNumber, textProp } from './resolve';

	let {
		props,
		tracker,
		onlog
	}: { props: Record<string, PropValue>; tracker: RenderTracker; onlog?: LogHandler } = $props();

	const label = $derived(textProp(props.label));
</script>

<section class="card">
	{#if label}<h3 class="label">{label}</h3>{/if}
	<div class="chips" role="group" aria-label={label || `Log ${tracker.id}`}>
		{#each tracker.chips as amount, i (i)}
			<button
				type="button"
				class="chip"
				disabled={!onlog}
				aria-label={`Log ${showNumber(amount)} ${tracker.unit} of ${tracker.id}`}
				onclick={() => onlog?.(tracker.id, amount)}
			>
				+{showNumber(amount)} {tracker.unit}
			</button>
		{/each}
	</div>
</section>

<style>
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	.chip {
		min-height: 44px;
		min-width: 44px;
		padding: 0 16px;
		border: 1px solid var(--aroh-line);
		border-radius: 999px;
		background: var(--aroh-chip);
		color: inherit;
		font: inherit;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
		cursor: pointer;
		touch-action: manipulation;
	}
	.chip:active {
		background: var(--aroh-track);
	}
	.chip:focus-visible {
		outline: 2px solid var(--aroh-accent);
		outline-offset: 2px;
	}
	.chip:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
