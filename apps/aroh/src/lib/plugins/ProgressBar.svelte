<script lang="ts">
	import type { PropValue, QueryData } from './types';
	import { fraction, numberProp, showNumber, textProp } from './resolve';

	let { props, data }: { props: Record<string, PropValue>; data: QueryData } = $props();

	const label = $derived(textProp(props.label));
	const value = $derived(numberProp(props.value, data));
	const max = $derived(numberProp(props.max, data));
	const fill = $derived(fraction(value, max));
	const percent = $derived(Math.round(fill * 100));
</script>

<section class="card">
	<div class="head">
		<span class="label">{label}</span>
		<span class="nums">{showNumber(value)} / {showNumber(max)}</span>
	</div>
	<div
		class="track"
		role="progressbar"
		aria-label={label}
		aria-valuemin={0}
		aria-valuemax={100}
		aria-valuenow={percent}
		aria-valuetext={`${percent}%`}
	>
		<div class="bar" style:width={`${percent}%`}></div>
	</div>
</section>

<style>
	.head {
		display: flex;
		justify-content: space-between;
		gap: 8px;
		margin-bottom: 8px;
		font-size: 0.875rem;
	}
	.label {
		font-weight: 600;
	}
	.nums {
		color: var(--aroh-muted);
		font-variant-numeric: tabular-nums;
	}
	.track {
		height: 10px;
		border-radius: 999px;
		background: var(--aroh-track);
		overflow: hidden;
	}
	.bar {
		height: 100%;
		border-radius: inherit;
		background: var(--aroh-accent);
		transition: width 200ms ease;
	}
	@media (prefers-reduced-motion: reduce) {
		.bar {
			transition: none;
		}
	}
</style>
