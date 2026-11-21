<script lang="ts">
	import type { PropValue, QueryData } from './types';
	import { actionRow, listProp, numberProp, showTime, textProp } from './resolve';

	let { props, data }: { props: Record<string, PropValue>; data: QueryData } = $props();

	const label = $derived(textProp(props.label));
	const limit = $derived(numberProp(props.limit, data) ?? 5);
	const items = $derived(listProp(props.items, data));
	const rows = $derived(
		(items ?? [])
			.map(actionRow)
			.filter((r) => r !== null)
			.slice(0, limit)
	);
</script>

<section class="card">
	<h3 class="label">{label}</h3>
	{#if items === null}
		<p class="empty">-</p>
	{:else if rows.length === 0}
		<p class="empty">Nothing scheduled.</p>
	{:else}
		<ul>
			{#each rows as row (row.id)}
				<li class="row">
					<span class="time">{showTime(row.startAt)}</span>
					<span class="title" class:done={row.done}>{row.title}</span>
					<span class="status status-{row.status}">{row.status}</span>
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
		display: grid;
		grid-template-columns: 4.5rem 1fr auto;
		align-items: center;
		gap: 8px;
		min-height: 44px;
		border-top: 1px solid var(--aroh-line);
	}
	.row:first-child {
		border-top: none;
	}
	.time {
		color: var(--aroh-muted);
		font-size: 0.875rem;
		font-variant-numeric: tabular-nums;
	}
	.title {
		overflow-wrap: anywhere;
	}
	.done {
		color: var(--aroh-muted);
		text-decoration: line-through;
	}
	.status {
		font-size: 0.75rem;
		padding: 2px 8px;
		border-radius: 999px;
		background: var(--aroh-track);
		color: var(--aroh-muted);
	}
	.status-done {
		color: var(--aroh-accent);
	}
	.status-missed {
		color: var(--aroh-danger);
	}
</style>
