<script lang="ts">
	import StatusChip from '$lib/ui/components/StatusChip.svelte';
	import { actionStyle } from '$lib/ui/status';
	import { daysBetween, formatDate } from '$lib/ui/format';
	import { PACE_LABEL, type DashGoal } from '$lib/app/dashboard';

	/**
	 * Each shown goal's pace: category dot, title and pace chip; done against due to date as a bar;
	 * missed and skipped counts in the shared status colours; and the projected finish against
	 * the deadline, "Ongoing" for an open-ended plan. `onopen` opens a goal.
	 */
	let { goals, onopen }: { goals: DashGoal[]; onopen?: (id: string) => void } = $props();

	const day = (ymd: string) => formatDate(ymd, { day: 'numeric', month: 'short' });
	const missed = actionStyle('missed');
	const skipped = actionStyle('skipped');

	/** The finish words of a goal: date and days early or late, "Ongoing", or nothing planned. */
	function finishText(g: DashGoal): string {
		if (!g.finish) return g.open_ended ? 'Ongoing' : 'Nothing planned';
		if (!g.deadline) return `Finish ${day(g.finish)}`;
		const gap = daysBetween(g.deadline, g.finish);
		return `Finish ${day(g.finish)}${gap > 0 ? `, ${gap} ${gap === 1 ? 'day' : 'days'} late` : gap < 0 ? `, ${-gap} ${gap === -1 ? 'day' : 'days'} early` : ''}`;
	}
</script>

<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
	{#each goals as g (g.id)}
		<li>
			<button type="button" class="flex w-full flex-col gap-1.5 px-3 py-2.5 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => onopen?.(g.id)}>
				<span class="flex w-full items-center gap-2">
					<span class="size-2 shrink-0 rounded-full" style="background: var(--cat-{g.slot})" aria-hidden="true"></span>
					<span class="min-w-0 flex-1 truncate text-sm font-medium">{g.title}</span>
					<StatusChip tone={g.pace === 'behind' ? 'warn' : 'done'} label={PACE_LABEL[g.pace]} />
				</span>
				<span class="block h-1.5 w-full overflow-hidden rounded-full bg-surface-2" role="img" aria-label="{g.done} of {g.due} due sessions done">
					<span class="block h-full rounded-full bg-fg" style="width: {g.due ? (g.done / g.due) * 100 : 0}%"></span>
				</span>
				<span class="flex w-full flex-wrap justify-between gap-x-3 text-xs text-muted tabular-nums">
					<span>{g.done} of {g.due} done to date{#if g.missed}<span class="ml-1.5 {missed.labelClass}">{g.missed} {missed.label?.toLowerCase()}</span>{/if}{#if g.skipped}<span class="ml-1.5 {skipped.labelClass}">{g.skipped} {skipped.label?.toLowerCase()}</span>{/if}</span>
					<span>{finishText(g)}</span>
				</span>
			</button>
		</li>
	{:else}
		<li class="px-3 py-3 text-sm text-muted">No goals match this filter.</li>
	{/each}
</ul>
