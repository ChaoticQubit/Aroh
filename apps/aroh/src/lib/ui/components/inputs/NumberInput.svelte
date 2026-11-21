<script lang="ts">
	import Minus from '@lucide/svelte/icons/minus';
	import Plus from '@lucide/svelte/icons/plus';
	import { Button } from '$lib/ui/primitives/button';
	import { Input } from '$lib/ui/primitives/input';
	import Field from '../Field.svelte';
	import Range from '../Range.svelte';
	import { snap } from '$lib/ui/format';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/**
	 * Numeric entry with a unit suffix and step buttons, plus a draggable slider when the spec has
	 * both bounds. Emits the number, or null when cleared.
	 */
	let { spec, onchange }: { spec: SpecOf<'number_input'>; onchange?: OnChange } = $props();

	const id = $props.id();
	let value = $derived(spec.value);

	/** Stores a new value within the spec bounds and step precision, and reports it. */
	function set(next: number | null) {
		value = next === null ? null : snap(next, spec.step, spec.min, spec.max);
		onchange?.(value);
	}
</script>

<Field label={spec.label} {id}>
	<div class="flex items-center gap-2">
		<Button variant="outline" size="icon-touch" aria-label="Decrease {spec.label}" onclick={() => set((value ?? spec.min ?? 0) - spec.step)}>
			<Minus />
		</Button>
		<div class="relative flex-1">
			<Input
				{id}
				type="number"
				inputmode="decimal"
				step={spec.step}
				min={spec.min}
				max={spec.max}
				placeholder="Enter"
				class="h-11 pr-12 text-right text-lg tabular-nums"
				value={value ?? ''}
				oninput={(e) => {
					const text = e.currentTarget.value;
					set(text === '' ? null : Number(text));
				}}
			/>
			<span class="pointer-events-none absolute inset-y-0 right-3 flex items-center text-sm text-muted">{spec.unit}</span>
		</div>
		<Button variant="outline" size="icon-touch" aria-label="Increase {spec.label}" onclick={() => set((value ?? spec.min ?? 0) + spec.step)}>
			<Plus />
		</Button>
	</div>
	{#if spec.min !== undefined && spec.max !== undefined}
		<Range label="{spec.label} slider" {value} min={spec.min} max={spec.max} step={spec.step} oninput={set} />
	{/if}
</Field>
