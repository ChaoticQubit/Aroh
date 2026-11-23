<script lang="ts">
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import Minus from '@lucide/svelte/icons/minus';
	import Plus from '@lucide/svelte/icons/plus';
	import Lock from '@lucide/svelte/icons/lock';
	import NotebookText from '@lucide/svelte/icons/notebook-text';
	import { Button } from '$lib/ui/primitives/button';
	import { Switch } from '$lib/ui/primitives/switch';
	import StatusChip from '../StatusChip.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDay, formatDuration, formatTime } from '$lib/ui/format';
	import type { ActionView } from '$lib/ui/contract';
	import type { ActionEdit } from '$lib/ui/screens/types';

	/**
	 * Body of the action edit sheet: move the day and start time, change the duration in 15 minute
	 * steps, toggle Fixed time, save, or delete after a confirm step. Emits `onsave` with
	 * `{ id, start, end }`, `onlock` with the id and the new Fixed time, `ondelete` with the id and
	 * `onclose` on cancel. `confirming` opens on the delete confirm.
	 */
	let {
		action,
		confirming = false,
		onsave,
		onlock,
		ondelete,
		onclose
	}: {
		action: ActionView;
		confirming?: boolean;
		onsave?: (edit: ActionEdit) => void;
		onlock?: (id: string, locked: boolean) => void;
		ondelete?: (id: string) => void;
		onclose?: () => void;
	} = $props();

	const { zone } = getClock();
	const STEP = 15;
	const MINUTE_MS = 60_000;
	const titleId = $props.id();
	let fixed = $derived(action.locked);
	let start = $derived(Date.parse(action.start));
	let minutes = $derived(Math.round((Date.parse(action.end) - Date.parse(action.start)) / MINUTE_MS));
	let asking = $derived(confirming);
	const startIso = $derived(new Date(start).toISOString());
	const endIso = $derived(new Date(start + minutes * MINUTE_MS).toISOString());
	const changed = $derived(startIso !== new Date(action.start).toISOString() || endIso !== new Date(action.end).toISOString());

	/** Moves the start by `delta` minutes, keeping the duration. */
	function move(delta: number) {
		start += delta * MINUTE_MS;
	}

	/** Changes the duration by `delta` minutes, never below one step or above 8 hours. */
	function resize(delta: number) {
		minutes = Math.min(480, Math.max(STEP, minutes + delta));
	}
</script>

<div class="flex flex-col gap-4" role="group" aria-labelledby={titleId}>
	<div class="flex flex-col gap-1">
		<div class="flex items-center gap-2">
			<span class="size-2.5 shrink-0 rounded-full" style="background: var(--cat-{action.category})" aria-hidden="true"></span>
			<h2 id={titleId} class="truncate text-lg font-semibold">{action.title}</h2>
			{#if fixed}<Lock class="size-4 shrink-0 text-muted" aria-label="Fixed time" />{/if}
			{#if action.status === 'missed'}<StatusChip tone="missed" label="Missed" />{/if}
		</div>
		<p class="truncate text-sm text-muted">{action.goalTitle}</p>
	</div>
	<Button variant="outline" size="touch" href="/actions/{action.id}"><NotebookText />Open session card</Button>

	<dl class="flex flex-col divide-y divide-border rounded-sm border border-border">
		{#each [{ name: 'Day', value: formatDay(startIso, zone), less: () => move(-1440), more: () => move(1440), step: 'one day' }, { name: 'Start', value: formatTime(startIso, zone), less: () => move(-STEP), more: () => move(STEP), step: `${STEP} min` }, { name: 'Duration', value: formatDuration(minutes), less: () => resize(-STEP), more: () => resize(STEP), step: `${STEP} min` }] as row (row.name)}
			<div class="flex items-center gap-2 py-0.5 pr-0.5 pl-3">
				<dt class="w-20 shrink-0 text-sm text-muted">{row.name}</dt>
				<dd class="flex flex-1 items-center justify-end gap-1">
					<Button variant="ghost" size="icon-touch" aria-label="{row.name} earlier or shorter by {row.step}" onclick={row.less}>
						{#if row.name === 'Day'}<ChevronLeft />{:else}<Minus />{/if}
					</Button>
					<span class="min-w-24 text-center text-sm font-medium tabular-nums" aria-live="polite">{row.value}</span>
					<Button variant="ghost" size="icon-touch" aria-label="{row.name} later or longer by {row.step}" onclick={row.more}>
						{#if row.name === 'Day'}<ChevronRight />{:else}<Plus />{/if}
					</Button>
				</dd>
			</div>
		{/each}
	</dl>
	<label class="flex min-h-12 cursor-pointer items-center gap-3" for="{titleId}-fixed">
		<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Fixed time</span><span class="text-xs text-muted">Agents and Busy days never move it.</span></span>
		<Switch
			id="{titleId}-fixed"
			checked={fixed}
			onCheckedChange={(v) => {
				fixed = v;
				onlock?.(action.id, v);
			}}
		/>
	</label>
	<p class="text-sm text-muted tabular-nums">
		{formatTime(startIso, zone)} - {formatTime(endIso, zone)}{#if changed}<span class="text-fg"> · was {formatTime(action.start, zone)} - {formatTime(action.end, zone)}</span>{/if}
	</p>

	{#if asking}
		<div class="flex flex-col gap-3 rounded-sm border border-status-missed/40 bg-status-missed/10 p-3" role="alertdialog" aria-label="Confirm delete">
			<p class="text-sm text-fg">Delete <span class="font-medium">{action.title}</span> on {formatDay(action.start, zone)}? It leaves your calendar on every device.</p>
			<div class="flex justify-end gap-2">
				<Button variant="outline" size="touch" onclick={() => (asking = false)}>Keep</Button>
				<Button variant="destructive" size="touch" onclick={() => ondelete?.(action.id)}>Delete</Button>
			</div>
		</div>
	{:else}
		<div class="flex items-center gap-2">
			<Button variant="ghost" size="touch" class="-ml-2 text-status-missed" onclick={() => (asking = true)}>Delete</Button>
			<Button variant="outline" size="touch" class="ml-auto" onclick={() => onclose?.()}>Cancel</Button>
			<Button size="touch" disabled={!changed} onclick={() => onsave?.({ id: action.id, start: startIso, end: endIso })}>Save</Button>
		</div>
	{/if}
</div>
