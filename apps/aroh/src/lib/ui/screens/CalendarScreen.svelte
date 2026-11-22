<script lang="ts">
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import { Button } from '$lib/ui/primitives/button';
	import Segmented from '../components/Segmented.svelte';
	import Render from '../components/Render.svelte';
	import CategoryFilter from '../components/CategoryFilter.svelte';
	import CalendarGrid from './CalendarGrid.svelte';
	import { getClock } from '$lib/ui/clock';
	import { daysBetween, formatDate, formatYmd, mondayOf, shiftDay, shiftMonth } from '$lib/ui/format';
	import type { ActionView } from '$lib/ui/contract';
	import type { BusyView, CategoryView } from './types';

	/**
	 * Calendar: toolbar with Today, previous, next and the Day, Week, Month switch, category chips,
	 * then a week hour grid on wide containers or a week strip over a day grid on phones. Kept device
	 * events (`busy`) show in the hour grids as neutral blocks.
	 */
	let { actions, categories, busy = [] }: { actions: ActionView[]; categories: CategoryView[]; busy?: BusyView[] } = $props();

	const { now, zone } = getClock();
	const today = formatYmd(now, zone);
	const monday = mondayOf(today);
	let view = $state<'day' | 'week' | 'month'>('week');
	let anchor = $state(today);
	let hidden = $state<number[]>([]);
	const used = $derived(categories.filter((c) => actions.some((a) => a.category === c.slot)));
	const visible = $derived(actions.filter((a) => !hidden.includes(a.category)));
	const weekStart = $derived(shiftDay(monday, Math.floor(daysBetween(monday, anchor) / 7) * 7));
	const week = $derived(Array.from({ length: 7 }, (_, i) => shiftDay(weekStart, i)));
	const monthStart = $derived(shiftMonth(anchor, 0));
	const monthGridStart = $derived(mondayOf(monthStart));
	const monthDays = $derived(Array.from({ length: 42 }, (_, i) => shiftDay(monthGridStart, i)));
	const range = $derived(
		view === 'day'
			? formatDate(anchor, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' })
			: view === 'week'
				? `${formatDate(week[0], { day: 'numeric', month: 'short' })} - ${formatDate(week[6], { day: 'numeric', month: 'short', year: 'numeric' })}`
				: formatDate(monthStart, { month: 'long', year: 'numeric' })
	);

	/** Moves the anchor one day, week or month in `dir`. */
	function step(dir: 1 | -1) {
		anchor = view === 'month' ? shiftMonth(anchor, dir) : shiftDay(anchor, view === 'day' ? dir : dir * 7);
	}
</script>

<div class="flex flex-col gap-3 p-4 @3xl:p-6">
	<div class="flex flex-wrap items-center gap-2">
		<Button variant="outline" size="touch" class="@3xl:h-9" onclick={() => (anchor = today)}>Today</Button>
		<Button variant="ghost" size="icon-touch" class="@3xl:size-9" aria-label="Previous" onclick={() => step(-1)}><ChevronLeft /></Button>
		<Button variant="ghost" size="icon-touch" class="@3xl:size-9" aria-label="Next" onclick={() => step(1)}><ChevronRight /></Button>
		<p class="mr-auto text-sm font-semibold" aria-live="polite">{range}</p>
		<Segmented label="Calendar view" bind:value={view} options={[{ value: 'day', label: 'Day' }, { value: 'week', label: 'Week' }, { value: 'month', label: 'Month' }]} />
	</div>
	<CategoryFilter categories={used} bind:hidden />

	{#if view === 'month'}
		<div class="overflow-hidden rounded-md border border-border bg-bg">
			<div class="grid grid-cols-7 border-b border-border">
				{#each week as d (d)}<p class="py-2 text-center text-xs text-muted">{formatDate(d, { weekday: 'short' })}</p>{/each}
			</div>
			<div class="grid grid-cols-7">
				{#each monthDays as d (d)}
					{@const items = visible.filter((a) => formatYmd(a.start, zone) === d)}
					<button
						type="button"
						class="flex min-h-16 flex-col items-stretch gap-0.5 border-t border-l border-border p-1 text-left outline-none first:border-l-0 focus-visible:ring-3 focus-visible:ring-ring/50 @3xl:min-h-24 {d.slice(0, 7) === monthStart.slice(0, 7) ? '' : 'text-muted/60'}"
						aria-label="{d}, {items.length} actions"
						onclick={() => {
							anchor = d;
							view = 'day';
						}}
					>
						<span class="w-fit min-w-5 rounded-sm px-1 text-center text-xs leading-5 font-medium tabular-nums {d === today ? 'bg-accent text-accent-fg' : ''}">{Number(d.slice(8))}</span>
						<span class="flex flex-wrap gap-0.5 @3xl:hidden">
							{#each items.slice(0, 4) as a (a.id)}<span class="size-1.5 rounded-full" style="background: var(--cat-{a.category})"></span>{/each}
						</span>
						<span class="hidden flex-col gap-0.5 @3xl:flex">
							{#each items.slice(0, 3) as a (a.id)}
								<span class="truncate border-l-3 px-1 text-xs leading-4" style="border-left-color: var(--cat-{a.category}); background: color-mix(in oklch, var(--cat-{a.category}) 8%, var(--bg))">{a.title}</span>
							{/each}
							{#if items.length > 3}<span class="px-1 text-xs text-muted">+{items.length - 3} more</span>{/if}
						</span>
					</button>
				{/each}
			</div>
		</div>
	{:else if view === 'week'}
		<div class="hidden @3xl:block"><CalendarGrid days={week} actions={visible} {busy} /></div>
		<div class="flex flex-col gap-3 @3xl:hidden">
			<Render spec={{ kind: 'week_strip', label: 'This week', actions: visible, startDay: week[0] }} onchange={(d) => (anchor = d as string)} />
			<CalendarGrid days={[anchor]} actions={visible} {busy} />
		</div>
	{:else}
		<CalendarGrid days={[anchor]} actions={visible} {busy} />
	{/if}
</div>
