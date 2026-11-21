<script lang="ts">
	import type { PropValue } from './types';
	import { textProp } from './resolve';

	let { props }: { props: Record<string, PropValue> } = $props();

	const text = $derived(textProp(props.text));
</script>

<section class="card">
	<p class="note">{text}</p>
</section>

<style>
	.note {
		margin: 0;
		white-space: pre-line;
		overflow-wrap: anywhere;
	}
</style>
