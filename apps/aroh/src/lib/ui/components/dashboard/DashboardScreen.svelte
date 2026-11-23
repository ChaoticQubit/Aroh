<script lang="ts">
	import Flame from '@lucide/svelte/icons/flame';
	import Plus from '@lucide/svelte/icons/plus';
	import { Button } from '$lib/ui/primitives/button';
	import { chips, filterGoals, rate, totals, type DashGoal } from '$lib/app/dashboard';
	import GoalPace from './GoalPace.svelte';
	import WeeklyMinutes from './WeeklyMinutes.svelte';
	import TrackerTrends from './TrackerTrends.svelte';

	/**
	 * The Dashboard tab (Q69): filter chips (All, each category, On track, Behind, Ahead) over every
	 * section; combined tiles of the shown goals; each goal's pace; minutes per week; tracker trends.
	 * `weeks` are the minutes chart Mondays, `trendWeeks` the trend length. No goals shows the empty
	 * state with `onnew`; `onopen` opens a goal.
	 */
	let { goals, weeks, trendWeeks, onopen, onnew }: { goals: DashGoal[]; weeks: string[]; trendWeeks: number; onopen?: (id: string) => void; onnew?: () => void } = $props();

	let chip = $state('All');
	const options = $derived(chips(goals));
	const current = $derived(options.includes(chip) ? chip : 'All');
	const shown = $derived(filterGoals(goals, current));
	const t = $derived(totals(shown));
	const pct = (r: number | null) => (r === null ? 'None due' : `${r}%`);
</script>

{#if goals.length === 0}
	<div class="flex flex-col items-start gap-3 p-4">
		<h2 class="text-md font-semibold">No goals yet</h2>
		<p class="text-sm text-muted">Start a goal and the dashboard shows how each one keeps pace, the minutes you put in each week, streaks and tracker trends.</p>
		<Button variant="outline" size="touch" onclick={() => onnew?.()}><Plus />New goal</Button>
	</div>
{:else}
	<div class="flex flex-col gap-5 p-4">
		<div class="-mx-4 flex gap-2 overflow-x-auto px-4" role="radiogroup" aria-label="Filter goals">
			{#each options as c (c)}
				<button
					type="button"
					role="radio"
					aria-checked={current === c}
					class="min-h-9 shrink-0 rounded-sm border px-3 text-sm font-medium whitespace-nowrap outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {current === c ? 'border-fg bg-fg text-bg' : 'border-border bg-bg text-fg'}"
					onclick={() => (chip = c)}>{c}</button
				>
			{/each}
		</div>

		<dl class="grid grid-cols-2 gap-px overflow-hidden rounded-sm border border-border bg-border">
			<div class="flex min-w-0 flex-col gap-0.5 bg-bg px-3 py-2"><dt class="text-xs text-muted">On track</dt><dd class="text-lg font-semibold tabular-nums">{t.keeping} of {t.count}</dd></div>
			<div class="flex min-w-0 flex-col gap-0.5 bg-bg px-3 py-2">
				<dt class="text-xs text-muted">Done to date</dt>
				<dd class="text-lg font-semibold tabular-nums">{t.done} of {t.due}{#if t.due}<span class="ml-1 text-sm font-normal text-muted">{rate(t)}%</span>{/if}</dd>
			</div>
			<div class="flex min-w-0 flex-col gap-0.5 bg-bg px-3 py-2">
				<dt class="text-xs text-muted">Completion</dt>
				<dd class="text-lg font-semibold tabular-nums">{pct(rate(t.week))}<span class="ml-1 text-sm font-normal text-muted">this week · {pct(rate(t.month)).toLowerCase()} this month</span></dd>
			</div>
			<div class="flex min-w-0 flex-col gap-0.5 bg-bg px-3 py-2">
				<dt class="text-xs text-muted">Streak</dt>
				<dd class="flex flex-wrap items-center gap-x-1 text-lg font-semibold tabular-nums"><Flame class="size-4 text-muted" aria-hidden="true" />{t.streak} {t.streak === 1 ? 'day' : 'days'}<span class="text-sm font-normal text-muted">best {t.best}</span></dd>
			</div>
		</dl>

		<section class="flex flex-col gap-2" aria-labelledby="dash-goals">
			<h2 id="dash-goals" class="text-sm font-semibold">Goals</h2>
			<GoalPace goals={shown} {onopen} />
		</section>

		<WeeklyMinutes goals={shown} {weeks} />
		<TrackerTrends goals={shown} weeks={trendWeeks} />
	</div>
{/if}
