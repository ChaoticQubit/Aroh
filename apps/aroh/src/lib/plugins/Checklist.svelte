<script lang="ts">
	import type { PropValue, QueryData, ToggleHandler } from './types';
	import { actionRow, listProp, textProp } from './resolve';

	let {
		props,
		data,
		ontoggle
	}: { props: Record<string, PropValue>; data: QueryData; ontoggle?: ToggleHandler } = $props();

	const label = $derived(textProp(props.label));
	const items = $derived(listProp(props.items, data));
	const rows = $derived((items ?? []).map(actionRow).filter((r) => r !== null));
</script>

<section class="card">
	<h3 class="label">{label}</h3>
	{#if items === null}
		<p class="empty">-</p>
	{:else if rows.length === 0}
		<p class="empty">Nothing here.</p>
	{:else}
		<ul>
			{#each rows as row (row.id)}
				<li>
					<label class="row">
						<input
							type="checkbox"
							checked={row.done}
							disabled={!ontoggle}
							onchange={(e) => ontoggle?.(row.id, e.currentTarget.checked)}
						/>
						<span class:done={row.done}>{row.title}</span>
					</label>
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: 44px;
	}
	input {
		width: 22px;
		height: 22px;
		margin: 0;
		accent-color: var(--aroh-accent);
		flex: none;
	}
	.done {
		color: var(--aroh-muted);
		text-decoration: line-through;
	}
</style>
