<script lang="ts">
	import type { LogHandler, QueryData, RenderModel, SlotName, ToggleHandler } from './types';
	import StatTile from './StatTile.svelte';
	import ProgressBar from './ProgressBar.svelte';
	import Checklist from './Checklist.svelte';
	import ActionList from './ActionList.svelte';
	import QuickChips from './QuickChips.svelte';
	import TextNote from './TextNote.svelte';

	let {
		name,
		model,
		data,
		onlog,
		ontoggle
	}: {
		name: SlotName;
		model: RenderModel;
		data: QueryData;
		onlog?: LogHandler;
		ontoggle?: ToggleHandler;
	} = $props();

	const instances = $derived(model.slots[name] ?? []);
	const trackers = $derived(new Map(model.trackers.map((t) => [t.id, t])));
</script>

{#if instances.length > 0}
	<div class="aroh-slot" data-slot={name}>
		{#each instances as inst (inst.key)}
			{#if inst.component === 'stat-tile'}
				<StatTile props={inst.props} {data} />
			{:else if inst.component === 'progress-bar'}
				<ProgressBar props={inst.props} {data} />
			{:else if inst.component === 'checklist'}
				<Checklist props={inst.props} {data} {ontoggle} />
			{:else if inst.component === 'action-list'}
				<ActionList props={inst.props} {data} />
			{:else if inst.component === 'quick-chips'}
				{@const tracker = trackers.get(String(inst.props.tracker))}
				{#if tracker}<QuickChips props={inst.props} {tracker} {onlog} />{/if}
			{:else if inst.component === 'text-note'}
				<TextNote props={inst.props} />
			{/if}
		{/each}
	</div>
{/if}

<style>
	.aroh-slot {
		--aroh-bg: #ffffff;
		--aroh-card: #f5f5f7;
		--aroh-chip: #ffffff;
		--aroh-fg: #1c1c1e;
		--aroh-muted: #6b6b70;
		--aroh-line: #d8d8dc;
		--aroh-track: #e5e5ea;
		--aroh-accent: #0a7d55;
		--aroh-danger: #c4261d;
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: 12px;
		padding: 12px max(16px, env(safe-area-inset-right)) 12px max(16px, env(safe-area-inset-left));
		color: var(--aroh-fg);
		font-family: system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
		font-size: 1rem;
		line-height: 1.4;
		-webkit-text-size-adjust: 100%;
	}
	@media (prefers-color-scheme: dark) {
		.aroh-slot {
			--aroh-bg: #000000;
			--aroh-card: #1c1c1e;
			--aroh-chip: #2c2c2e;
			--aroh-fg: #f2f2f7;
			--aroh-muted: #a1a1a8;
			--aroh-line: #3a3a3c;
			--aroh-track: #3a3a3c;
			--aroh-accent: #34c98e;
			--aroh-danger: #ff6b60;
		}
	}
	.aroh-slot :global(.card) {
		background: var(--aroh-card);
		border-radius: 14px;
		padding: 16px;
		min-width: 0;
	}
	.aroh-slot :global(.label) {
		margin: 0 0 8px;
		font-size: 0.9375rem;
		font-weight: 600;
	}
	.aroh-slot :global(.empty) {
		margin: 0;
		color: var(--aroh-muted);
	}
</style>
