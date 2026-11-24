<script lang="ts" module>
	/**
	 * One row of a grouped settings list: `label` with an optional muted `detail` under it and a
	 * `value` on the right. `onclick` makes it a button (`go` adds a chevron), `toggle` a switch
	 * reporting through `ontoggle`; `danger` paints the label red.
	 */
	export type SettingsRow = {
		label: string;
		detail?: string;
		value?: string;
		go?: boolean;
		danger?: boolean;
		disabled?: boolean;
		onclick?: () => void;
		toggle?: boolean;
		ontoggle?: (on: boolean) => void;
	};

	/** A titled group of rows; `note` is a muted line under the card. */
	export type SettingsSection = { title: string; note?: string; rows: SettingsRow[] };
</script>

<script lang="ts">
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import { Switch } from '$lib/ui/primitives/switch';

	/**
	 * The grouped list every Settings screen uses (Q72): each section is a small heading over one
	 * bordered card of rows with dividers. Rows are at least 52 px high; buttons, switches and plain
	 * text rows share one layout so screens stay consistent.
	 */
	let { sections }: { sections: SettingsSection[] } = $props();
	const id = $props.id();
	const row = 'flex min-h-13 w-full items-center gap-3 px-4 py-2 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset';
</script>

{#snippet text(r: SettingsRow)}
	<span class="flex min-w-0 flex-1 flex-col">
		<span class="text-base break-words {r.danger ? 'text-danger' : ''}">{r.label}</span>
		{#if r.detail}<span class="text-sm text-muted">{r.detail}</span>{/if}
	</span>
	{#if r.value}<span class="max-w-[50%] shrink-0 truncate text-right text-sm text-muted tabular-nums">{r.value}</span>{/if}
{/snippet}

<div class="flex flex-col gap-6">
	{#each sections as sec, s (sec.title)}
		<section class="flex flex-col gap-2" aria-labelledby="{id}-{s}">
			<h2 id="{id}-{s}" class="px-1 text-sm font-semibold">{sec.title}</h2>
			<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
				{#each sec.rows as r, i (i)}
					<li>
						{#if r.toggle !== undefined && r.ontoggle}
							<label class="{row} cursor-pointer" for="{id}-{s}-{i}">
								{@render text(r)}
								<Switch id="{id}-{s}-{i}" checked={r.toggle} disabled={r.disabled} onCheckedChange={(on) => r.ontoggle?.(on)} />
							</label>
						{:else if r.onclick}
							<button type="button" class="{row} hover:bg-surface-2 disabled:opacity-50" disabled={r.disabled} onclick={() => r.onclick?.()}>
								{@render text(r)}
								{#if r.go}<ChevronRight class="size-4 shrink-0 text-muted" aria-hidden="true" />{/if}
							</button>
						{:else}
							<div class={row}>{@render text(r)}</div>
						{/if}
					</li>
				{/each}
			</ul>
			{#if sec.note}<p class="px-1 text-xs text-muted">{sec.note}</p>{/if}
		</section>
	{/each}
</div>
