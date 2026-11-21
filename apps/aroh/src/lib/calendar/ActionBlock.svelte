<script lang="ts">
	import type { ActionView } from './api';
	import { formatRange, goalHue } from './time';

	interface Props {
		action: ActionView;
		top: number;
		height: number;
		col: number;
		cols: number;
		onopen: (action: ActionView) => void;
	}

	let { action, top, height, col, cols, onopen }: Props = $props();

	const compact = $derived(height < 44);
</script>

<button
	type="button"
	class="block"
	class:compact
	class:done={action.status === 'done'}
	class:skipped={action.status === 'skipped'}
	class:missed={action.status === 'missed'}
	style:top="{top}px"
	style:height="{height}px"
	style:left="calc({(col / cols) * 100}% + 2px)"
	style:width="calc({100 / cols}% - 4px)"
	style:--hue={goalHue(action.goal_color_index)}
	onclick={() => onopen(action)}
	aria-label="{action.title}, {formatRange(action.start_at, action.end_at)}, {action.status}"
>
	<span class="title">
		{#if action.status === 'done'}<span aria-hidden="true">✓ </span>{/if}{action.title}
	</span>
	{#if !compact}<span class="time num">{formatRange(action.start_at, action.end_at)}</span>{/if}
</button>

<style>
	.block {
		position: absolute;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 2px;
		padding: 4px 6px;
		overflow: hidden;
		text-align: left;
		font-size: 13px;
		line-height: 16px;
		color: var(--cal-ink);
		background: color-mix(in srgb, var(--hue) var(--tint), var(--cal-raised));
		border: 1px solid color-mix(in srgb, var(--hue) 45%, var(--cal-raised));
		border-left: 3px solid var(--hue);
		border-radius: 6px;
		cursor: pointer;
		transition: top var(--ease), height var(--ease), left var(--ease), width var(--ease);
	}
	.block.compact {
		flex-direction: row;
		align-items: center;
		padding-block: 0 2px;
		overflow: visible;
	}
	.block.compact::after {
		content: '';
		position: absolute;
		inset: 50% -2px auto;
		height: 44px;
		transform: translateY(-50%);
	}
	.block.done {
		background: var(--hue);
		border-color: var(--hue);
		color: var(--goal-solid-ink);
	}
	.block.done .time {
		color: inherit;
	}
	.block.skipped {
		background: transparent;
		border-style: dashed;
		color: var(--cal-muted);
	}
	.block.skipped .title {
		text-decoration: line-through;
	}
	.block.missed {
		background: repeating-linear-gradient(
			135deg,
			color-mix(in srgb, var(--hue) 22%, var(--cal-raised)) 0 5px,
			var(--cal-raised) 5px 10px
		);
	}
	.title {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 100%;
	}
	.time {
		color: var(--cal-muted);
		font-size: 12px;
	}
	.block:focus-visible {
		outline: 2px solid var(--cal-accent);
		outline-offset: 1px;
		z-index: 2;
	}
</style>
