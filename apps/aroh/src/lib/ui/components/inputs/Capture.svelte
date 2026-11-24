<script lang="ts">
	import Camera from '@lucide/svelte/icons/camera';
	import Mic from '@lucide/svelte/icons/mic';
	import Video from '@lucide/svelte/icons/video';
	import Field from '../Field.svelte';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/** Photo, audio or video capture through the device picker. Emits the picked File. */
	let { spec, onchange }: { spec: SpecOf<'capture'>; onchange?: OnChange } = $props();

	const id = $props.id();
	let picked = $state<string | null>(null);
	const media = {
		photo: { icon: Camera, action: 'Take a photo', accept: 'image/*' },
		audio: { icon: Mic, action: 'Record audio', accept: 'audio/*' },
		video: { icon: Video, action: 'Record a video', accept: 'video/*' }
	} as const;
	const m = $derived(media[spec.media]);
</script>

<Field label={spec.label} {id}>
	<label
		for={id}
		class="flex min-h-11 cursor-pointer items-center gap-2 rounded-sm border border-dashed border-border-strong bg-bg px-3 text-sm text-muted hover:bg-surface has-[:focus-visible]:ring-3 has-[:focus-visible]:ring-ring/50"
	>
		<m.icon class="size-4" />
		<span class={picked ? 'text-fg' : ''}>{picked ?? m.action}</span>
		<input
			{id}
			type="file"
			accept={m.accept}
			capture="environment"
			class="sr-only"
			onchange={(e) => {
				const file = e.currentTarget.files?.[0];
				if (!file) return;
				picked = file.name;
				onchange?.(file);
			}}
		/>
	</label>
</Field>
