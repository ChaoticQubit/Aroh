<script module lang="ts">
	/**
	 * One file, photo or link attached to a goal for the agents to read; `failed` marks a file whose
	 * text could not be read or whose upload failed, `retry` one the owner can send again.
	 */
	export type Attachment = { id: string; kind: 'photo' | 'pdf' | 'file' | 'link'; name: string; detail: string; failed?: boolean; retry?: boolean };
</script>

<script lang="ts">
	import Camera from '@lucide/svelte/icons/camera';
	import FileIcon from '@lucide/svelte/icons/file';
	import FileText from '@lucide/svelte/icons/file-text';
	import ImageIcon from '@lucide/svelte/icons/image';
	import LinkIcon from '@lucide/svelte/icons/link';
	import X from '@lucide/svelte/icons/x';
	import { Button } from '$lib/ui/primitives/button';
	import { Input } from '$lib/ui/primitives/input';

	/**
	 * The attachments of a goal: each file, photo or link with its detail and Remove, a link field,
	 * and Photo and File pickers only when `upload` is true (Q61). Emits `onfiles(files)` for picked
	 * files, `onlink(url)` for a typed link, `onretry(id)` for a failed upload and `onremove(id)`.
	 * `busy` disables the buttons while saving.
	 */
	let {
		files,
		upload = false,
		busy = false,
		onfiles,
		onlink,
		onretry,
		onremove
	}: {
		files: Attachment[];
		upload?: boolean;
		busy?: boolean;
		onfiles?: (files: File[]) => void;
		onlink?: (url: string) => void;
		onretry?: (id: string) => void;
		onremove?: (id: string) => void;
	} = $props();

	const id = $props.id();
	const ICON = { photo: ImageIcon, pdf: FileText, file: FileIcon, link: LinkIcon };
	let linking = $state(false);
	let link = $state('');
	let photos = $state<HTMLInputElement>();
	let documents = $state<HTMLInputElement>();

	/** Adds the typed link and closes the link field; an empty link is ignored. */
	function addLink() {
		const url = link.trim();
		if (url) onlink?.(url);
		link = '';
		linking = false;
	}

	/** Emits the files picked in `input`, then clears it so the same file can be picked again. */
	function picked(input: HTMLInputElement) {
		const list = Array.from(input.files ?? []);
		input.value = '';
		if (list.length) onfiles?.(list);
	}
</script>

<section class="flex flex-col gap-2" aria-labelledby="{id}-att">
	<div class="flex items-baseline justify-between">
		<h2 id="{id}-att" class="text-sm font-semibold">Attachments</h2>
		<span class="text-xs text-muted">Agents read these first</span>
	</div>
	{#if files.length}
		<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
			{#each files as f (f.id)}
				{@const Icon = ICON[f.kind]}
				<li class="flex min-h-14 items-center gap-3 pl-3">
					<Icon class="size-4 shrink-0 text-muted" aria-hidden="true" />
					<span class="flex min-w-0 flex-1 flex-col">
						<span class="truncate text-sm font-medium">{f.name}</span>
						<span class="truncate text-xs {f.failed ? 'text-danger' : 'text-muted'}">{f.detail}</span>
					</span>
					{#if f.retry}<Button variant="outline" size="touch" disabled={busy} onclick={() => onretry?.(f.id)}>Retry</Button>{/if}
					<Button variant="ghost" size="icon-touch" aria-label="Remove {f.name}" disabled={busy} onclick={() => onremove?.(f.id)}><X /></Button>
				</li>
			{/each}
		</ul>
	{/if}
	{#if linking}
		<div class="flex gap-2">
			<Input type="url" inputmode="url" class="h-11 flex-1 text-lg" placeholder="https://" aria-label="Link" bind:value={link} />
			<Button size="touch" disabled={busy} onclick={addLink}>Add</Button>
		</div>
	{/if}
	<div class="grid gap-2 {upload ? 'grid-cols-3' : 'grid-cols-1'}">
		{#if upload}
			<Button variant="outline" size="touch" disabled={busy} onclick={() => photos?.click()}><Camera />Photo</Button>
			<Button variant="outline" size="touch" disabled={busy} onclick={() => documents?.click()}><FileIcon />File</Button>
		{/if}
		<Button variant="outline" size="touch" disabled={busy} onclick={() => (linking = true)}><LinkIcon />Link</Button>
	</div>
	{#if upload}
		<input bind:this={photos} class="hidden" type="file" accept="image/*" multiple aria-label="Pick photos" onchange={(e) => picked(e.currentTarget)} />
		<input bind:this={documents} class="hidden" type="file" accept=".pdf,.txt,.md,application/pdf,text/plain,text/markdown" multiple aria-label="Pick files" onchange={(e) => picked(e.currentTarget)} />
	{/if}
	<p class="text-xs text-muted">{upload ? 'Photos, PDFs, documents and links.' : 'Links.'} Synced to your account for this goal's agents only.</p>
</section>
