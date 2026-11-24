<script lang="ts">
	import { getClock } from '$lib/ui/clock';
	import { formatDate, formatYmd, shiftDay } from '$lib/ui/format';
	import { allDayOn } from '$lib/ui/screens/edits';
	import type { OnChange, SpecOf } from '$lib/ui/contract';
	import type { AllDayView } from '$lib/ui/screens/types';

	/**
	 * Seven days from `startDay` with a dot per action and a small bar per device all-day event in
	 * `allDay` (filled for a Busy day or Away, outlined for a Reminder, dashed when unchosen); today
	 * is tinted. Emits the picked "YYYY-MM-DD".
	 */
	let { spec, allDay = [], onchange }: { spec: SpecOf<'week_strip'>; allDay?: AllDayView[]; onchange?: OnChange } = $props();

	const { now, zone } = getClock();
	const today = formatYmd(now, zone);
	const days = $derived(
		Array.from({ length: 7 }, (_, i) => {
			const date = shiftDay(spec.startDay, i);
			return {
				date,
				name: formatDate(date, { weekday: 'short' }),
				num: Number(date.slice(8)),
				actions: spec.actions.filter((a) => formatYmd(a.start, zone) === date),
				events: allDayOn(allDay, date)
			};
		})
	);
	let picked = $state(today);

	/** Bar classes for one all-day event: filled for Busy day and Away, outlined for Reminder, dashed when unchosen. */
	function barTone(e: AllDayView): string {
		if (e.kind === 'busy' || e.kind === 'away') return 'border-muted bg-muted';
		return e.kind === 'reminder' ? 'border-muted bg-bg' : 'border-dashed border-muted bg-bg';
	}
</script>

<div class="flex flex-col gap-2">
	<span class="sr-only">{spec.label}</span>
	<div role="radiogroup" aria-label={spec.label} class="grid grid-cols-7 gap-1">
		{#each days as d (d.date)}
			<button
				type="button"
				role="radio"
				aria-checked={picked === d.date}
				class="flex min-h-16 flex-col items-center justify-center gap-1 rounded-sm border text-xs outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {picked === d.date
					? 'border-fg bg-bg'
					: 'border-transparent hover:bg-surface-2'}"
				onclick={() => {
					picked = d.date;
					onchange?.(d.date);
				}}
			>
				<span class="text-muted">{d.name}</span>
				<span class="min-w-6 rounded-sm px-1 text-center text-sm leading-6 font-semibold tabular-nums {d.date === today ? 'bg-accent text-accent-fg' : 'text-fg'}">{d.num}</span>
				{#if d.events.length}
					<span class="flex w-full flex-col gap-0.5 px-1" aria-label="All day: {d.events.map((e) => e.title).join(', ')}">
						{#each d.events.slice(0, 2) as e (e.id)}<span class="h-1 w-full rounded-full border {barTone(e)}"></span>{/each}
					</span>
				{/if}
				<span class="flex h-1.5 gap-0.5" aria-label="{d.actions.length} actions">
					{#each d.actions.slice(0, 4) as a (a.id)}
						<span class="size-1.5 rounded-full {a.status === 'missed' ? 'border border-status-missed' : ''}" style={a.status === 'missed' ? undefined : `background: var(--cat-${a.category})`}></span>
					{/each}
				</span>
			</button>
		{/each}
	</div>
</div>
