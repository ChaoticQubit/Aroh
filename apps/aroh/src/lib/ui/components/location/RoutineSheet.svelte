<script lang="ts">
	import Pencil from '@lucide/svelte/icons/pencil';
	import * as Sheet from '$lib/ui/primitives/sheet';
	import { Button } from '$lib/ui/primitives/button';
	import { getClock } from '$lib/ui/clock';
	import { formatDay } from '$lib/ui/format';
	import type { RoutineRow } from './types';

	/**
	 * Bottom sheet for one routine of the Routines card (Q93): Edit opens its next session's card,
	 * where edits ask this session or all future; Delete asks once, then `ondelete` ends it from now.
	 * With `routine` null it confirms deleting Routines itself. `confirming` opens on the confirm
	 * step, `busy` disables the choices, `inline` draws it in place for mockups and tests.
	 */
	let {
		routine,
		confirming = false,
		busy = false,
		inline = false,
		ondelete,
		onclose
	}: { routine: RoutineRow | null; confirming?: boolean; busy?: boolean; inline?: boolean; ondelete: () => void; onclose: () => void } = $props();

	const { zone } = getClock();
	let asking = $derived(confirming || routine === null);
	const title = $derived(routine ? routine.title : 'Routines');
	const PANEL = 'flex flex-col gap-3 rounded-t-[var(--radius-dialog)] border-t border-border bg-bg pt-2 pr-[max(1rem,env(safe-area-inset-right))] pb-[max(1rem,env(safe-area-inset-bottom))] pl-[max(1rem,env(safe-area-inset-left))]';
</script>

{#snippet body()}
	<span class="mx-auto h-1 w-9 rounded-full bg-border-strong" aria-hidden="true"></span>
	<div class="flex flex-col gap-1">
		<h2 class="truncate text-lg font-semibold">{title}</h2>
		{#if routine}
			<p class="truncate text-sm text-muted">{routine.cadence}{routine.place ? ` · ${routine.place}` : ''}</p>
			<p class="text-sm text-muted">{routine.nextDue ? `Next ${formatDay(routine.nextDue, zone)}` : 'No session planned yet'}</p>
		{/if}
	</div>
	{#if asking}
		<div class="flex flex-col gap-3 rounded-sm border border-status-missed/40 bg-status-missed/10 p-3" role="alertdialog" aria-label="Confirm delete">
			{#if routine}
				<p class="text-sm text-fg">Delete <span class="font-medium">{routine.title}</span> from now? Its future sessions leave your calendar on every device. Done and logged sessions stay.</p>
			{:else}
				<p class="text-sm text-fg">Delete <span class="font-medium">Routines</span>? Every routine ends from now and Routines leaves your goals until you approve a new routine. Done and logged sessions stay.</p>
			{/if}
			<div class="flex justify-end gap-2">
				<Button variant="outline" size="touch" disabled={busy} onclick={() => (routine ? (asking = false) : onclose())}>Keep</Button>
				<Button variant="destructive" size="touch" disabled={busy} onclick={ondelete}>{routine ? 'Delete routine' : 'Delete Routines'}</Button>
			</div>
		</div>
	{:else if routine}
		{#if routine.nextId}<Button variant="outline" size="touch" href="/actions/{routine.nextId}"><Pencil />Edit next session</Button>{/if}
		<div class="flex items-center gap-2">
			<Button variant="ghost" size="touch" class="-ml-2 text-status-missed" onclick={() => (asking = true)}>Delete</Button>
			<Button variant="outline" size="touch" class="ml-auto" onclick={onclose}>Close</Button>
		</div>
	{/if}
{/snippet}

{#if inline}
	<div class="fixed inset-0 z-40 flex flex-col justify-end">
		<button type="button" class="flex-1 bg-scrim outline-none" aria-label="Close" tabindex="-1" onclick={onclose}></button>
		<div role="dialog" aria-label={title} class={PANEL}>{@render body()}</div>
	</div>
{:else}
	<Sheet.Root open={true} onOpenChange={(open) => !open && onclose()}>
		<Sheet.Content side="bottom" showCloseButton={false} aria-describedby={undefined} class="{PANEL} text-fg shadow-none">
			<Sheet.Title class="sr-only">{title}</Sheet.Title>
			{@render body()}
		</Sheet.Content>
	</Sheet.Root>
{/if}
