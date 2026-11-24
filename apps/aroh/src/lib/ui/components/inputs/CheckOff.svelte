<script lang="ts">
	import { Checkbox } from '$lib/ui/primitives/checkbox';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/** One tickable item on a full-width 44 px row. Emits the new done flag. */
	let { spec, onchange }: { spec: SpecOf<'check_off'>; onchange?: OnChange } = $props();

	const id = $props.id();
	let done = $derived(spec.done);
</script>

<label for={id} class="flex min-h-11 cursor-pointer items-center gap-3 rounded-sm border border-border bg-bg px-3 hover:bg-surface">
	<Checkbox
		{id}
		class="size-4"
		checked={done}
		onCheckedChange={(next) => {
			done = next;
			onchange?.(next);
		}}
	/>
	<span class="text-sm {done ? 'text-muted line-through' : 'text-fg'}">{spec.label}</span>
</label>
