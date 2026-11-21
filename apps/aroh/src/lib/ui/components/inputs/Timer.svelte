<script lang="ts">
	import Pause from '@lucide/svelte/icons/pause';
	import Play from '@lucide/svelte/icons/play';
	import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
	import { Button } from '$lib/ui/primitives/button';
	import { Progress } from '$lib/ui/primitives/progress';
	import Field from '../Field.svelte';
	import { formatDuration } from '$lib/ui/format';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/** Stopwatch toward a target duration. Emits elapsed whole seconds on pause and reset. */
	let { spec, onchange }: { spec: SpecOf<'timer'>; onchange?: OnChange } = $props();

	const id = $props.id();
	let seconds = $state(0);
	let running = $state(false);
	const target = $derived(spec.targetMinutes * 60);
	const clock = $derived(`${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`);

	$effect(() => {
		if (!running) return;
		const tick = setInterval(() => (seconds += 1), 1000);
		return () => clearInterval(tick);
	});
</script>

<Field label={spec.label} labelId={id} detail="Target {formatDuration(spec.targetMinutes)}">
	<div class="flex items-center gap-3" role="group" aria-labelledby={id}>
		<div class="flex flex-1 flex-col gap-2">
			<p class="text-lg font-semibold tabular-nums" role="timer">{clock}</p>
			<Progress value={Math.min(seconds, target)} max={target} />
		</div>
		<Button
			size="icon-touch"
			aria-label={running ? 'Pause' : 'Start'}
			onclick={() => {
				running = !running;
				if (!running) onchange?.(seconds);
			}}
		>
			{#if running}<Pause />{:else}<Play />{/if}
		</Button>
		<Button
			variant="outline"
			size="icon-touch"
			aria-label="Reset"
			onclick={() => {
				running = false;
				seconds = 0;
				onchange?.(0);
			}}
		>
			<RotateCcw />
		</Button>
	</div>
</Field>
