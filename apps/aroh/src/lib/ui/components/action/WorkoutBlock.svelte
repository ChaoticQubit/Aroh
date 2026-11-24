<script lang="ts">
	import Check from '@lucide/svelte/icons/check';
	import Minus from '@lucide/svelte/icons/minus';
	import Plus from '@lucide/svelte/icons/plus';
	import TrendingUp from '@lucide/svelte/icons/trending-up';
	import { restText, setState, setsLine, type Card, type CardLog } from '$lib/ui/action/card';

	type Workout = Extract<Card['blocks'][number], { kind: 'workout' }>;

	/**
	 * Workout block of an action card (Q71). `plan` shows the table (exercise, sets, reps, weight, rest);
	 * `log` shows one row per set with a tick that accepts the planned numbers, tappable numbers,
	 * and `editing` opens the steppers of one set. Ticks and edits call `ontick` and `onedit`.
	 */
	let {
		block,
		index,
		log,
		mode = 'plan',
		editing = null,
		ontick,
		onedit,
		onstep
	}: {
		block: Workout;
		index: number;
		log: CardLog;
		mode?: 'plan' | 'log';
		editing?: { row: number; set: number } | null;
		ontick?: (row: number, set: number) => void;
		onedit?: (row: number, set: number) => void;
		onstep?: (row: number, set: number, reps: number, weight: number) => void;
	} = $props();

	const logged = (row: number, set: number) => log.sets.find((s) => s.block === index && s.row === row && s.set === set && s.done);
	const num = 'flex h-11 min-w-0 items-center justify-center rounded-sm text-md tabular-nums outline-none focus-visible:ring-3 focus-visible:ring-ring/50';
</script>

{#if mode === 'plan'}
	<table class="w-full table-fixed border-collapse text-sm">
		<colgroup><col /><col class="w-10" /><col class="w-11" /><col class="w-13" /><col class="w-13" /></colgroup>
		<thead>
			<tr class="border-b border-border text-left text-xs text-muted">
				<th scope="col" class="pb-2 font-medium">Exercise</th>
				<th scope="col" class="pb-2 text-right font-medium">Sets</th>
				<th scope="col" class="pb-2 text-right font-medium">Reps</th>
				<th scope="col" class="pb-2 text-right font-medium">kg</th>
				<th scope="col" class="pb-2 text-right font-medium">Rest</th>
			</tr>
		</thead>
		<tbody>
			{#each block.exercises as e, row (row)}
				{@const reps = e.sets.map((s) => s.reps)}
				<tr class="border-b border-border/70 align-top last:border-b-0">
					<th scope="row" class="py-2.5 pr-2 text-left font-medium">
						<span class="block break-words">{e.name}</span>
						{#if e.cue}
							<span class="mt-0.5 flex items-center gap-1 text-xs font-normal text-muted">
								{#if e.cue.startsWith('Up')}<TrendingUp class="size-3 shrink-0 text-status-done" aria-hidden="true" />{/if}{e.cue}
							</span>
						{/if}
					</th>
					<td class="py-2.5 text-right tabular-nums">{e.sets.length}</td>
					<td class="py-2.5 text-right tabular-nums">{reps.every((r) => r === reps[0]) ? reps[0] : reps.join('/')}</td>
					<td class="py-2.5 text-right tabular-nums">{e.sets[0].weight ?? 'BW'}</td>
					<td class="py-2.5 text-right text-muted tabular-nums">{restText(e.rest_s)}</td>
				</tr>
			{/each}
		</tbody>
	</table>
{:else}
	<ol class="flex flex-col divide-y divide-border">
		{#each block.exercises as e, row (row)}
			{@const ticked = e.sets.filter((_, s) => setState(e, log, index, row, s) !== 'planned').length}
			<li class="flex flex-col gap-1 py-3 first:pt-0 last:pb-0">
				<div class="flex items-baseline justify-between gap-3">
					<p class="min-w-0 font-medium break-words">{e.name}</p>
					<p class="shrink-0 text-xs text-muted tabular-nums">{ticked} of {e.sets.length} · rest {restText(e.rest_s)}</p>
				</div>
				<p class="text-xs text-muted tabular-nums">{setsLine(e.sets, e.unit)}</p>
				<div class="grid grid-cols-[3rem_1fr_1fr_2.75rem] items-center gap-x-1 text-xs text-muted">
					<span>Set</span><span class="text-center">{e.cue === 'Seconds, not reps' ? 'Sec' : 'Reps'}</span><span class="text-center">{e.unit ?? 'Load'}</span><span></span>
				</div>
				{#each e.sets as plan, set (set)}
					{@const state = setState(e, log, index, row, set)}
					{@const hit = logged(row, set)}
					{@const open = editing?.row === row && editing?.set === set}
					<div class="grid grid-cols-[3rem_1fr_1fr_2.75rem] items-center gap-x-1 rounded-sm {open ? 'bg-bg ring-1 ring-border-strong' : ''}">
						<span class="pl-1 text-sm tabular-nums {state === 'planned' ? 'text-muted' : 'text-fg'}">{set + 1}</span>
						{#if open}
							<span class="flex items-center justify-center gap-0.5">
								<button type="button" class="{num} size-9 text-muted" aria-label="One rep less" onclick={() => onstep?.(row, set, -1, 0)}><Minus class="size-4" /></button>
								<span class="w-6 text-center text-md font-semibold tabular-nums">{hit?.reps ?? plan.reps}</span>
								<button type="button" class="{num} size-9 text-muted" aria-label="One rep more" onclick={() => onstep?.(row, set, 1, 0)}><Plus class="size-4" /></button>
							</span>
							<span class="flex items-center justify-center gap-0.5">
								<button type="button" class="{num} size-9 text-muted" aria-label="2.5 less" onclick={() => onstep?.(row, set, 0, -2.5)}><Minus class="size-4" /></button>
								<span class="w-9 text-center text-md font-semibold tabular-nums">{hit?.weight ?? plan.weight ?? 'BW'}</span>
								<button type="button" class="{num} size-9 text-muted" aria-label="2.5 more" onclick={() => onstep?.(row, set, 0, 2.5)}><Plus class="size-4" /></button>
							</span>
						{:else}
							<button type="button" class="{num} {state === 'edited' ? 'font-semibold text-status-warn' : state === 'done' ? 'text-fg' : 'text-muted'}" aria-label="Set {set + 1} reps {hit?.reps ?? plan.reps}, tap to change" onclick={() => onedit?.(row, set)}>{hit?.reps ?? plan.reps}</button>
							<button type="button" class="{num} {state === 'done' || state === 'edited' ? 'text-fg' : 'text-muted'}" aria-label="Set {set + 1} load {hit?.weight ?? plan.weight ?? 'bodyweight'}, tap to change" onclick={() => onedit?.(row, set)}>{hit?.weight ?? plan.weight ?? 'BW'}</button>
						{/if}
						<button
							type="button"
							class="flex size-11 items-center justify-center rounded-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
							aria-label="Set {set + 1} {state === 'planned' ? 'not done, tick to log as planned' : 'done'}"
							aria-pressed={state !== 'planned'}
							onclick={() => ontick?.(row, set)}
						>
							<span class="flex size-7 items-center justify-center rounded-full border {state === 'planned' ? 'border-border-strong' : state === 'edited' ? 'border-status-warn bg-status-warn text-bg' : 'border-status-done bg-status-done text-bg'}">
								{#if state !== 'planned'}<Check class="size-4" strokeWidth={2.5} />{/if}
							</span>
						</button>
					</div>
					{#if open && hit && state === 'edited'}<p class="pl-1 text-xs text-muted">Logged {hit.reps} of {plan.reps}. The coach plans next week from this.</p>{/if}
				{/each}
			</li>
		{/each}
	</ol>
{/if}
