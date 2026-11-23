<script lang="ts">
	import { Button } from '$lib/ui/primitives/button';
	import { Switch } from '$lib/ui/primitives/switch';
	import NumberInput from '$lib/ui/components/inputs/NumberInput.svelte';
	import { AUTONOMY_LEVELS, type AgentSettings } from '$lib/app/newgoal';

	/**
	 * Agent controls for one goal or for the Settings defaults: Ask, Balanced or Auto with each meaning,
	 * then the reminders and calendars switches, bound through `value`. `limit` 'settings' always shows
	 * the run time limit field; 'goal' shows the Settings default (`defaultLimit` minutes) with Change,
	 * and the field only once the goal overrides it; 'none' hides the limit.
	 */
	let {
		value = $bindable(),
		limit = 'none',
		defaultLimit = 5,
		onchange
	}: { value: AgentSettings; limit?: 'none' | 'settings' | 'goal'; defaultLimit?: number; onchange?: (value: AgentSettings) => void } = $props();

	const id = $props.id();

	/** Replaces part of the settings and reports the result. */
	function set(patch: Partial<AgentSettings>) {
		value = { ...value, ...patch };
		onchange?.(value);
	}
</script>

<div class="flex flex-col gap-3">
	<div role="radiogroup" aria-labelledby="{id}-level" class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
		<span id="{id}-level" class="sr-only">How much agents change on their own</span>
		{#each AUTONOMY_LEVELS as a (a.value)}
			<button type="button" role="radio" aria-checked={value.autonomy === a.value} class="flex min-h-14 items-center gap-3 px-3 py-2 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset" onclick={() => set({ autonomy: a.value })}>
				<span class="flex size-4 shrink-0 items-center justify-center rounded-full border {value.autonomy === a.value ? 'border-fg' : 'border-border-strong'}" aria-hidden="true">
					{#if value.autonomy === a.value}<span class="size-2 rounded-full bg-fg"></span>{/if}
				</span>
				<span class="flex flex-col">
					<span class="text-sm font-medium text-fg">{a.label}</span>
					<span class="text-xs text-muted">{a.meaning}</span>
				</span>
			</button>
		{/each}
	</div>
	<div class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg px-3">
		<label class="flex min-h-14 cursor-pointer items-center gap-3 py-2" for="{id}-remind">
			<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Reminders</span><span class="text-xs text-muted">A notification before each session.</span></span>
			<Switch id="{id}-remind" checked={value.reminders} onCheckedChange={(on) => set({ reminders: on })} />
		</label>
		<label class="flex min-h-14 cursor-pointer items-center gap-3 py-2" for="{id}-cal">
			<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Use my calendars</span><span class="text-xs text-muted">Plan around busy time. Nothing is written back.</span></span>
			<Switch id="{id}-cal" checked={value.useCalendars} onCheckedChange={(on) => set({ useCalendars: on })} />
		</label>
	</div>
	{#if limit === 'settings' || (limit === 'goal' && value.runLimit !== null)}
		<div class="flex flex-col gap-2 rounded-sm border border-border bg-bg p-3">
			<NumberInput
				spec={{ kind: 'number_input', label: 'Run time limit', unit: 'min', value: value.runLimit ?? defaultLimit, step: 1, min: 1, max: 120 }}
				onchange={(v) => set({ runLimit: typeof v === 'number' ? v : limit === 'settings' ? defaultLimit : null })}
			/>
			<p class="text-xs text-muted">Counts active agent work only. Waiting for you does not count. A run that reaches the limit stops and spends nothing more.</p>
			{#if limit === 'goal'}
				<div class="flex"><Button variant="ghost" size="touch" onclick={() => set({ runLimit: null })}>Use the Settings default</Button></div>
			{/if}
		</div>
	{:else if limit === 'goal'}
		<div class="flex min-h-14 items-center gap-3 rounded-sm border border-border bg-bg px-3 py-2">
			<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Run time limit</span><span class="text-xs text-muted">{defaultLimit} min, from Settings</span></span>
			<Button variant="ghost" size="touch" onclick={() => set({ runLimit: defaultLimit })}>Change</Button>
		</div>
	{/if}
</div>
