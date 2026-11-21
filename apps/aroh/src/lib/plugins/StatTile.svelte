<script lang="ts">
	import type { PropValue, QueryData } from './types';
	import { numberProp, showNumber, textProp } from './resolve';

	let { props, data }: { props: Record<string, PropValue>; data: QueryData } = $props();

	const label = $derived(textProp(props.label));
	const value = $derived(showNumber(numberProp(props.value, data)));
	const unit = $derived(textProp(props.unit));
</script>

<section class="card tile">
	<span class="label">{label}</span>
	<span class="value">{value}{#if unit}<span class="unit"> {unit}</span>{/if}</span>
</section>

<style>
	.tile {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.label {
		font-size: 0.875rem;
		color: var(--aroh-muted);
	}
	.value {
		font-size: 2rem;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
		line-height: 1.1;
	}
	.unit {
		font-size: 1rem;
		font-weight: 400;
		color: var(--aroh-muted);
	}
</style>
