<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import Slot from './Slot.svelte';
	import type { QueryData, RenderModel, SlotName } from './types';

	let { name, goalId }: { name: SlotName; goalId?: string } = $props();

	let model = $state<RenderModel | null>(null);
	let data = $state<QueryData>({});
	let error = $state<string | null>(null);

	function load(goal: string | null) {
		Promise.all([
			invoke<RenderModel>('plugins_model'),
			invoke<QueryData>('plugins_data', { goalId: goal })
		])
			.then(([m, d]) => {
				model = m;
				data = d;
				error = null;
			})
			.catch((e: unknown) => {
				error = String(e);
			});
	}

	/** Logs a quick-chip tap for a declared water or workout tracker, then reloads the slot data. */
	function logTracker(tracker: string, amount: number) {
		const decl = model?.trackers.find((t) => t.id === tracker);
		if (!decl || (decl.kind !== 'water' && decl.kind !== 'workout')) return;
		invoke('intake_log_chip', { chip: `${decl.kind} ${amount}${decl.unit}` })
			.then(() => load(goalId ?? null))
			.catch((e: unknown) => {
				error = String(e);
			});
	}

	$effect(() => {
		const goal = goalId ?? null;
		load(goal);
		const off = listen('aroh://actions-changed', () => load(goal));
		return () => {
			off.then((f) => f());
		};
	});
</script>

{#if error}
	<p class="plugin-error" role="alert">Plugins failed to load: {error}</p>
{:else if model}
	<Slot {name} {model} {data} onlog={logTracker} />
{/if}

<style>
	.plugin-error {
		margin: 0 0 12px;
		color: #c4261d;
		font-size: 0.875rem;
	}
</style>
