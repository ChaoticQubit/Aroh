<script lang="ts">
	import { Textarea } from '$lib/ui/primitives/textarea';
	import Field from '../Field.svelte';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/** Free text note. Emits the text on every edit. */
	let { spec, onchange }: { spec: SpecOf<'note'>; onchange?: OnChange } = $props();

	const id = $props.id();
	let value = $derived(spec.value);
</script>

<Field label={spec.label} {id}>
	<Textarea
		{id}
		rows={3}
		placeholder={spec.placeholder}
		class="min-h-24 text-lg"
		{value}
		oninput={(e) => {
			value = e.currentTarget.value;
			onchange?.(value);
		}}
	/>
</Field>
