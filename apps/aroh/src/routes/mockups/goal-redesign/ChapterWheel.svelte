<script lang="ts">
	import { onMount } from 'svelte';
	import { Input } from '$lib/ui/primitives/input';

	/**
	 * Scroll wheel of chapters 1 to `count`, with "Not started" as 0, beside a typed number field;
	 * both stay in step. The wheel snaps to whole rows and opens scrolled to `value`.
	 */
	let { count, value, label }: { count: number; value: number; label: string } = $props();

	const ROW = 44;
	const id = $props.id();
	const rows = $derived(Array.from({ length: count + 1 }, (_, i) => i));
	let at = $derived(value);
	let wheel = $state<HTMLDivElement>();

	/** Scrolls the wheel so row `n` sits in the middle band. */
	function show(n: number, behavior: ScrollBehavior) {
		wheel?.scrollTo({ top: n * ROW, behavior });
	}

	/** Takes the row under the middle band as the value once scrolling settles. */
	function settle() {
		if (wheel) at = Math.max(0, Math.min(count, Math.round(wheel.scrollTop / ROW)));
	}

	onMount(() => show(at, 'instant'));
</script>

<div class="flex flex-col gap-2">
	<label for="{id}-typed" class="text-sm font-medium text-fg">{label}</label>
	<div class="flex items-center gap-3">
		<div class="relative h-[132px] flex-1 overflow-hidden rounded-sm border border-border bg-bg">
			<div class="pointer-events-none absolute inset-x-0 top-11 h-11 border-y border-border-strong bg-surface-2" aria-hidden="true"></div>
			<div bind:this={wheel} role="listbox" aria-label="{label} wheel" tabindex="0" class="relative h-full snap-y snap-mandatory overflow-y-scroll py-11 [scrollbar-width:none]" onscrollend={settle}>
				{#each rows as n (n)}
					<button type="button" role="option" aria-selected={at === n} class="flex h-11 w-full snap-center items-center justify-center text-lg tabular-nums outline-none {at === n ? 'font-semibold text-fg' : 'text-muted'}" onclick={() => ((at = n), show(n, 'smooth'))}>
						{n === 0 ? 'Not started' : `Chapter ${n}`}
					</button>
				{/each}
			</div>
		</div>
		<div class="flex w-24 flex-col gap-1">
			<Input id="{id}-typed" type="number" inputmode="numeric" min={0} max={count} class="h-11 text-center text-lg tabular-nums" value={at} oninput={(e) => {
				const n = Number(e.currentTarget.value);
				if (Number.isInteger(n) && n >= 0 && n <= count) (at = n), show(n, 'smooth');
			}} />
			<span class="text-center text-xs text-muted">of {count}</span>
		</div>
	</div>
</div>
