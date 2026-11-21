<script lang="ts">
	import Minus from '@lucide/svelte/icons/minus';
	import Plus from '@lucide/svelte/icons/plus';
	import { Button } from '$lib/ui/primitives/button';
	import Field from '../Field.svelte';
	import Range from '../Range.svelte';
	import { formatNumber, snap } from '$lib/ui/format';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/** Tally toward a target with a draggable slider between step buttons. Emits the new count. */
	let { spec, onchange }: { spec: SpecOf<'counter'>; onchange?: OnChange } = $props();

	const id = $props.id();
	const max = $derived(Math.max(spec.target * 2, spec.value));
	let value = $derived(spec.value);
	const over = $derived(value >= spec.target);

	/** Stores a whole count within 0..max and reports it. */
	function set(next: number) {
		value = snap(next, 1, 0, max);
		onchange?.(value);
	}
</script>

<Field label={spec.label} labelId={id} detail={over ? 'Target reached' : `${formatNumber(spec.target - value)} to go`} detailClass={over ? 'font-medium text-status-done' : undefined}>
	<div class="flex flex-col gap-1" role="group" aria-labelledby={id}>
		<p class="text-lg font-semibold tabular-nums" aria-live="polite">
			{formatNumber(value)} <span class="text-sm font-normal text-muted">/ {formatNumber(spec.target, spec.unit)}</span>
		</p>
		<div class="flex items-center gap-3">
			<Button variant="outline" size="icon-touch" aria-label="Remove one" onclick={() => set(value - 1)}><Minus /></Button>
			<div class="flex-1"><Range label={spec.label} {value} min={0} {max} step={1} oninput={set} /></div>
			<Button variant="outline" size="icon-touch" aria-label="Add one" onclick={() => set(value + 1)}><Plus /></Button>
		</div>
	</div>
</Field>
