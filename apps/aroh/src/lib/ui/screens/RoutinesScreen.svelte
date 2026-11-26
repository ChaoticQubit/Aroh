<script lang="ts">
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import * as Card from '$lib/ui/primitives/card';
	import StatusChip from '../components/StatusChip.svelte';
	import RoutineSheet from '../components/location/RoutineSheet.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDay } from '$lib/ui/format';
	import type { ViewSpec } from '$lib/ui/contract';
	import GoalScreen from './GoalScreen.svelte';
	import type { RoutineRow } from '../components/location/types';
	import type { GoalHeader } from './types';

	/**
	 * Dashboard of the built-in Routines goal: the goal screen with a table of every routine
	 * (place, streak, last done, next due) above the goal's view sections.
	 * Routines that belong to another goal live on that goal instead.
	 * Tapping a row opens its sheet with Edit and Delete; Delete Routines confirms deleting the goal (Q93).
	 * `ondelete` gets the routine's series id, or null for Routines. `opened` (a series id, or
	 * `'routines'`) and `confirming` open a sheet on first draw; `inline` draws it in place.
	 */
	let {
		goal,
		view,
		routines,
		opened = null,
		confirming = false,
		inline = false,
		busy = false,
		ondelete
	}: {
		goal: GoalHeader;
		view: ViewSpec;
		routines: RoutineRow[];
		opened?: string | null;
		confirming?: boolean;
		inline?: boolean;
		busy?: boolean;
		ondelete?: (seriesId: string | null) => void;
	} = $props();

	const { zone } = getClock();
	const ALL = 'routines';
	let open = $derived<string | null>(opened);
	const shown = $derived(open === ALL ? null : (routines.find((r) => r.id === open) ?? null));
</script>

<GoalScreen {goal} {view}>
	<section class="flex flex-col gap-3" aria-label="Your routines">
		<h3 class="text-sm font-semibold text-fg">Your routines</h3>
		<Card.Root size="sm" class="gap-0 px-0 py-0">
			<table class="w-full text-left text-sm">
				<thead class="text-xs text-muted">
					<tr class="border-b border-border">
						<th scope="col" class="px-4 py-2 font-medium">Routine</th>
						<th scope="col" class="px-2 py-2 text-right font-medium">Streak</th>
						<th scope="col" class="px-2 py-2 font-medium">Last</th>
						<th scope="col" class="py-2 pr-4 pl-2 font-medium">Next</th>
					</tr>
				</thead>
				<tbody class="divide-y divide-border">
					{#each routines as r (r.id)}
						<tr class="align-top">
							<td class="px-4 py-2.5">
								<button type="button" class="-mx-1 flex min-h-11 w-full items-center gap-1 rounded-sm px-1 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50" aria-label="Open {r.title}" onclick={() => (open = r.id)}>
									<span class="flex min-w-0 flex-col gap-0.5"><span class="font-medium">{r.title}</span><span class="text-xs text-muted">{r.cadence}{r.place ? ` · ${r.place}` : ''}</span></span>
									<ChevronRight class="ml-auto size-4 shrink-0 text-muted" aria-hidden="true" />
								</button>
							</td>
							<td class="px-2 py-2.5 text-right tabular-nums">{r.streak}</td>
							<td class="px-2 py-2.5 text-xs whitespace-nowrap text-muted">{r.lastDone ? formatDay(r.lastDone, zone) : 'Not yet'}</td>
							<td class="py-2.5 pr-4 pl-2">
								<span class="flex flex-col items-start gap-1">{#if r.nextDue}<span class="text-xs whitespace-nowrap">{formatDay(r.nextDue, zone)}</span>{/if}<span class="whitespace-nowrap"><StatusChip tone={r.tone} label={r.due} /></span></span>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</Card.Root>
		<p class="text-xs text-muted">Streak counts repeats in a row. Tap a routine to edit or delete it. Routines tied to a goal show on that goal.</p>
		<button type="button" class="min-h-11 w-full rounded-sm border border-border bg-bg text-sm text-muted outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => (open = ALL)}>Delete Routines</button>
	</section>
	{#if open === ALL || shown}
		{#key open}
			<RoutineSheet routine={shown} {confirming} {busy} {inline} ondelete={() => ondelete?.(shown ? shown.id : null)} onclose={() => (open = null)} />
		{/key}
	{/if}
</GoalScreen>
