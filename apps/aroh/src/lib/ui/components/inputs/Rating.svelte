<script lang="ts">
	import Field from '../Field.svelte';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/** A row of numbered buttons from min to max. Emits the picked number. */
	let { spec, onchange }: { spec: SpecOf<'rating'>; onchange?: OnChange } = $props();

	const id = $props.id();
	let value = $derived(spec.value);
	const steps = $derived(Array.from({ length: spec.max - spec.min + 1 }, (_, i) => spec.min + i));
</script>

<Field label={spec.label} labelId={id} detail={value === null ? 'Not rated' : `${value} of ${spec.max}`}>
	<div role="radiogroup" aria-labelledby={id} class="flex gap-1">
		{#each steps as step (step)}
			<button
				type="button"
				role="radio"
				aria-checked={value === step}
				class="h-11 min-w-0 flex-1 rounded-sm border text-sm tabular-nums transition-colors outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {value === step
					? 'border-fg bg-surface-2 font-semibold text-fg'
					: value !== null && step < value
						? 'border-border-strong bg-surface-2 text-fg'
						: 'border-border bg-bg text-fg hover:bg-surface'}"
				onclick={() => {
					value = step;
					onchange?.(step);
				}}
			>
				{step}
			</button>
		{/each}
	</div>
</Field>
