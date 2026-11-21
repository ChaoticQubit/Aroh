<script lang="ts">
	import { snap } from '$lib/ui/format';

	/**
	 * Draggable slider on a native range input: pointer drag, touch drag and arrow keys all move it.
	 * Draws a hairline track with a near-black fill up to the value. Emits the snapped number.
	 */
	let { value, min, max, step, label, oninput }: { value: number | null; min: number; max: number; step: number; label: string; oninput: (value: number) => void } = $props();

	const THUMB = 16;
	const at = $derived(snap(value ?? min, step, min, max));
	const ratio = $derived(max > min ? (at - min) / (max - min) : 0);
</script>

<div class="relative flex h-11 items-center">
	<div class="absolute inset-x-0 h-0.5 bg-border" aria-hidden="true"></div>
	<div class="absolute left-0 h-0.5 bg-fg" style="width: calc({ratio * 100}% + {(0.5 - ratio) * THUMB}px)" aria-hidden="true"></div>
	<input
		type="range"
		class="range relative w-full"
		aria-label={label}
		{min}
		{max}
		{step}
		value={at}
		oninput={(e) => oninput(snap(Number(e.currentTarget.value), step, min, max))}
	/>
</div>
