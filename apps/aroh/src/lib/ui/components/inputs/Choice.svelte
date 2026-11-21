<script lang="ts">
	import Field from '../Field.svelte';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/** Option chips, single or multiple pick. Emits the selected options as a string array. */
	let { spec, onchange }: { spec: SpecOf<'choice'>; onchange?: OnChange } = $props();

	const id = $props.id();
	let value = $derived(spec.value);

	/** Toggles one option, keeping at most one selected when the spec is single pick. */
	function toggle(option: string) {
		const on = value.includes(option);
		value = spec.multi ? (on ? value.filter((v) => v !== option) : spec.options.filter((o) => o === option || value.includes(o))) : [option];
		onchange?.(value);
	}
</script>

<Field label={spec.label} labelId={id} detail={spec.multi ? `${value.length} selected` : undefined}>
	<div role="group" aria-labelledby={id} class="flex flex-wrap gap-1">
		{#each spec.options as option (option)}
			{@const on = value.includes(option)}
			<button
				type="button"
				aria-pressed={on}
				class="h-11 min-w-11 rounded-sm border px-3 text-sm transition-colors outline-none focus-visible:ring-3 focus-visible:ring-ring/50 @3xl:h-8 {on
					? 'border-fg bg-surface-2 font-medium text-fg'
					: 'border-border bg-bg text-fg hover:bg-surface'}"
				onclick={() => toggle(option)}
			>
				{option}
			</button>
		{/each}
	</div>
</Field>
