<script lang="ts">
	import { getClock } from '$lib/ui/clock';
	import { formatDate, formatYmd, shiftDay } from '$lib/ui/format';
	import type { OnChange, SpecOf } from '$lib/ui/contract';

	/** Seven days from `startDay` with a dot per action; today is tinted. Emits the picked "YYYY-MM-DD". */
	let { spec, onchange }: { spec: SpecOf<'week_strip'>; onchange?: OnChange } = $props();

	const { now, zone } = getClock();
	const today = formatYmd(now, zone);
	const days = $derived(
		Array.from({ length: 7 }, (_, i) => {
			const date = shiftDay(spec.startDay, i);
			return {
				date,
				name: formatDate(date, { weekday: 'short' }),
				num: Number(date.slice(8)),
				actions: spec.actions.filter((a) => formatYmd(a.start, zone) === date)
			};
		})
	);
	let picked = $state(today);
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
				<span class="flex h-1.5 gap-0.5" aria-label="{d.actions.length} actions">
					{#each d.actions.slice(0, 4) as a (a.id)}
						<span class="size-1.5 rounded-full {a.status === 'missed' ? 'border border-status-missed' : ''}" style={a.status === 'missed' ? undefined : `background: var(--cat-${a.category})`}></span>
					{/each}
				</span>
			</button>
		{/each}
	</div>
</div>
