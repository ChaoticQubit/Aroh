<script lang="ts">
	import * as Card from '$lib/ui/primitives/card';
	import StatusChip from '../components/StatusChip.svelte';
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
	 */
	let { goal, view, routines }: { goal: GoalHeader; view: ViewSpec; routines: RoutineRow[] } = $props();

	const { zone } = getClock();
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
								<span class="flex flex-col gap-0.5"><span class="font-medium">{r.title}</span><span class="text-xs text-muted">{r.cadence}{r.place ? ` · ${r.place}` : ''}</span></span>
							</td>
							<td class="px-2 py-2.5 text-right tabular-nums">{r.streak}</td>
							<td class="px-2 py-2.5 text-xs whitespace-nowrap text-muted">{r.lastDone ? formatDay(r.lastDone, zone) : 'Not yet'}</td>
							<td class="py-2.5 pr-4 pl-2">
								<span class="flex flex-col items-start gap-1"><span class="text-xs whitespace-nowrap">{formatDay(r.nextDue, zone)}</span><span class="whitespace-nowrap"><StatusChip tone={r.tone} label={r.due} /></span></span>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</Card.Root>
		<p class="text-xs text-muted">Streak counts repeats in a row. Routines tied to a goal show on that goal.</p>
	</section>
</GoalScreen>
