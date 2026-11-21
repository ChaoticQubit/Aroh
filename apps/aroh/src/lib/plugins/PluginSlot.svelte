<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import Slot from './Slot.svelte';
	import type { QueryData, RenderModel, SlotName } from './types';

	let { name, goalId }: { name: SlotName; goalId?: string } = $props();

	let model = $state<RenderModel | null>(null);
	let data = $state<QueryData>({});
	let error = $state<string | null>(null);

	$effect(() => {
		const goal = goalId ?? null;
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
	});
</script>

{#if error}
	<p class="plugin-error" role="alert">Plugins failed to load: {error}</p>
{:else if model}
	<Slot {name} {model} {data} />
{/if}

<style>
	.plugin-error {
		margin: 0 0 12px;
		color: #c4261d;
		font-size: 0.875rem;
	}
</style>
