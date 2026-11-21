<script lang="ts">
	import type { ActionView, SettableStatus } from './api';
	import { formatTime, goalHue } from './time';

	interface Props {
		action: ActionView;
		now?: boolean;
		onstatus: (action: ActionView, status: SettableStatus) => void;
		onopen?: (action: ActionView) => void;
	}

	let { action, now = false, onstatus, onopen }: Props = $props();

	const open = $derived(action.status === 'planned' || action.status === 'missed');
</script>

<li
	class="row"
	class:now
	class:done={action.status === 'done'}
	class:skipped={action.status === 'skipped'}
	class:missed={action.status === 'missed'}
	style:--hue={goalHue(action.goal_color_index)}
>
	<button class="body" type="button" onclick={() => onopen?.(action)} aria-label="Open {action.title}">
		<span class="time num">
			{#if now}<span class="now-label">Now</span>{/if}
			<span>{formatTime(action.start_at)}</span>
			<span class="end">{formatTime(action.end_at)}</span>
		</span>
		<span class="text">
			<span class="title">{action.title}</span>
			{#if action.goal_title}<span class="goal"><span class="swatch" aria-hidden="true"></span>{action.goal_title}</span>{/if}
			{#if action.status === 'missed'}<span class="state">Missed</span>{/if}
			{#if action.status === 'skipped'}<span class="state">Skipped</span>{/if}
		</span>
	</button>
	<span class="actions">
		{#if open}
			<button class="primary" type="button" onclick={() => onstatus(action, 'done')}>Done</button>
			<button class="quiet" type="button" onclick={() => onstatus(action, 'skipped')}>Skip</button>
		{:else}
			<button class="quiet" type="button" onclick={() => onstatus(action, 'planned')} aria-label="Undo {action.status} for {action.title}">
				{#if action.status === 'done'}<span class="check" aria-hidden="true">✓</span>{/if}Undo
			</button>
		{/if}
	</span>
</li>

<style>
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 64px;
		padding: 8px 8px 8px 0;
		background: var(--cal-raised);
		border: 1px solid var(--cal-line);
		border-left: 4px solid var(--hue);
		border-radius: 8px;
		list-style: none;
		transition: background-color var(--ease), opacity var(--ease);
	}
	.row.now {
		background: color-mix(in srgb, var(--hue) var(--tint), var(--cal-raised));
		border-color: var(--hue);
		border-left-width: 6px;
		min-height: 88px;
	}
	.row.done {
		background: color-mix(in srgb, var(--hue) var(--tint), var(--cal-raised));
	}
	.row.done .title {
		text-decoration: line-through;
		text-decoration-thickness: 1px;
	}
	.row.skipped {
		border-style: dashed;
		border-left-style: dashed;
		background: transparent;
	}
	.row.skipped .title {
		color: var(--cal-muted);
	}
	.row.missed {
		background: repeating-linear-gradient(
			135deg,
			color-mix(in srgb, var(--hue) var(--tint), var(--cal-raised)) 0 6px,
			var(--cal-raised) 6px 12px
		);
	}
	.body {
		flex: 1;
		min-width: 0;
		min-height: var(--tap);
		display: flex;
		align-items: flex-start;
		gap: 12px;
		padding: 4px 0 4px 12px;
		background: none;
		border: 0;
		text-align: left;
		cursor: pointer;
		border-radius: 6px;
	}
	.time {
		display: flex;
		flex-direction: column;
		width: 3.4em;
		flex: none;
		font-size: 15px;
		font-weight: 600;
		line-height: 20px;
	}
	.end {
		font-weight: 400;
		color: var(--cal-muted);
		font-size: 13px;
	}
	.now-label {
		font-size: 12px;
		font-weight: 600;
		color: var(--now);
	}
	.text {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.title {
		font-size: 16px;
		line-height: 22px;
		overflow-wrap: anywhere;
	}
	.now .title {
		font-size: 18px;
		font-weight: 600;
	}
	.goal,
	.state {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 13px;
		color: var(--cal-muted);
	}
	.swatch {
		width: 8px;
		height: 8px;
		border-radius: 2px;
		background: var(--hue);
	}
	.actions {
		display: flex;
		gap: 6px;
		flex: none;
	}
	.actions button {
		min-width: var(--tap);
		min-height: var(--tap);
		padding: 0 12px;
		border-radius: 999px;
		font-size: 15px;
		cursor: pointer;
		transition: transform var(--ease);
	}
	.actions button:active {
		transform: scale(0.96);
	}
	.primary {
		background: var(--cal-ink);
		color: var(--cal-raised);
		border: 1px solid var(--cal-ink);
		font-weight: 600;
	}
	.quiet {
		background: transparent;
		border: 1px solid var(--cal-line);
	}
	.check {
		margin-right: 4px;
	}
	button:focus-visible {
		outline: 2px solid var(--cal-accent);
		outline-offset: 2px;
	}
</style>
