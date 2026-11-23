<script lang="ts">
	import Minus from '@lucide/svelte/icons/minus';
	import Plus from '@lucide/svelte/icons/plus';
	import { Button } from '$lib/ui/primitives/button';
	import { Input } from '$lib/ui/primitives/input';
	import Field from '../Field.svelte';
	import Range from '../Range.svelte';
	import { sliderRange, stepNumber, typedNumber } from '$lib/ui/format';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/**
	 * Numeric entry with a unit suffix and step buttons, plus a draggable slider when the spec has
	 * both bounds. The bounds only size the slider: a typed number is kept as typed and the slider
	 * grows to hold it. Emits the number, or null when cleared.
	 */
	let { spec, onchange }: { spec: SpecOf<'number_input'>; onchange?: OnChange } = $props();

	const id = $props.id();
	let value = $derived(spec.value);
	let reach = $state<number | null>(null);
	const range = $derived(sliderRange(spec.min, spec.max, reach ?? spec.value));

	/** Stores a new value and reports it; a typed or stepped one also sets how far the slider reaches. */
	function set(next: number | null, typed = true) {
		value = next;
		if (typed) reach = next;
		onchange?.(value);
	}
</script>

<Field label={spec.label} {id}>
	<div class="flex items-center gap-2">
		<Button variant="outline" size="icon-touch" aria-label="Decrease {spec.label}" onclick={() => set(stepNumber(value, -spec.step, spec.step, spec.min))}>
			<Minus />
		</Button>
		<div class="flex min-w-0 flex-1 items-center gap-2">
			<Input
				{id}
				type="number"
				inputmode="decimal"
				step={spec.step}
				placeholder="Enter"
				class="h-11 min-w-0 flex-1 text-right text-lg tabular-nums"
				value={value ?? ''}
				oninput={(e) => {
					const text = e.currentTarget.value;
					const n = Number(text);
					if (text === '' || Number.isFinite(n)) set(text === '' ? null : typedNumber(n, spec.step));
				}}
			/>
			{#if spec.unit}<span class="max-w-28 shrink-0 text-sm text-muted">{spec.unit}</span>{/if}
		</div>
		<Button variant="outline" size="icon-touch" aria-label="Increase {spec.label}" onclick={() => set(stepNumber(value, spec.step, spec.step, spec.min))}>
			<Plus />
		</Button>
	</div>
	{#if range}
		<Range label="{spec.label} slider" {value} min={range.min} max={range.max} step={spec.step} oninput={(n) => set(n, false)} />
	{/if}
</Field>
