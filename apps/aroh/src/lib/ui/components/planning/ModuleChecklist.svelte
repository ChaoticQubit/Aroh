<script lang="ts">
	import { Checkbox } from '$lib/ui/primitives/checkbox';
	import { Progress } from '$lib/ui/primitives/progress';
	import Panel from '../Panel.svelte';
	import { formatDuration } from '$lib/ui/format';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/** Course modules with durations and a running total. Emits `{ title, done }` per tick. */
	let { spec, onchange }: { spec: SpecOf<'module_checklist'>; onchange?: OnChange } = $props();

	const id = $props.id();
	let overrides = $state<Record<string, boolean>>({});
	const items = $derived(spec.modules.map((m) => ({ ...m, done: overrides[m.title] ?? m.done })));
	const total = $derived(items.reduce((s, m) => s + m.minutes, 0));
	const left = $derived(items.filter((m) => !m.done).reduce((s, m) => s + m.minutes, 0));
</script>

<Panel title={spec.label}>
	{#snippet aside()}<span>{left === 0 ? 'All done' : `${formatDuration(left)} left`}</span>{/snippet}
	<Progress value={total - left} max={Math.max(1, total)} />
	<ul class="-my-1 divide-y divide-border">
		{#each items as m, i (m.title)}
			<li>
				<label for="{id}-{i}" class="flex min-h-11 cursor-pointer items-center gap-3 py-1.5">
					<Checkbox
						id="{id}-{i}"
						class="size-4"
						checked={m.done}
						onCheckedChange={(next) => {
							overrides[m.title] = next;
							onchange?.({ title: m.title, done: next });
						}}
					/>
					<span class="flex-1 text-sm {m.done ? 'text-muted line-through' : 'text-fg'}">{m.title}</span>
					<span class="text-xs text-muted tabular-nums">{formatDuration(m.minutes)}</span>
				</label>
			</li>
		{/each}
	</ul>
</Panel>
