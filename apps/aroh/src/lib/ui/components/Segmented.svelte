<script lang="ts" generics="T extends string">
	/**
	 * Segmented control: a radio group drawn as joined buttons. Binds `value` to the picked option.
	 * `compact` drops the height to 36 px for dense desktop toolbars; the default is the 44 px touch size.
	 */
	let { options, value = $bindable(), label, compact = false }: { options: { value: T; label: string }[]; value: T; label: string; compact?: boolean } = $props();
</script>

<div role="radiogroup" aria-label={label} class="inline-flex rounded-sm border border-border bg-surface p-0.5">
	{#each options as o (o.value)}
		<button
			type="button"
			role="radio"
			aria-checked={value === o.value}
			class="{compact ? 'h-8' : 'h-11 @3xl:h-8'} rounded-[3px] px-3 text-sm font-medium whitespace-nowrap outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {value === o.value ? 'bg-bg text-fg ring-1 ring-border-strong' : 'text-muted hover:text-fg'}"
			onclick={() => (value = o.value)}
		>
			{o.label}
		</button>
	{/each}
</div>
