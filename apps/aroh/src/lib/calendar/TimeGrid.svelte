<script lang="ts">
	import type { ActionView } from './api';
	import ActionBlock from './ActionBlock.svelte';
	import { DAY_MINUTES, formatTime, formatWeekday, layoutDay, minutesInDay, sameDay } from './time';

	interface Props {
		days: Date[];
		actions: ActionView[];
		now: Date;
		onopen: (action: ActionView) => void;
		showHeader?: boolean;
	}

	let { days, actions, now, onopen, showHeader = false }: Props = $props();

	const HOUR_PX = 64;
	const PX_PER_MIN = HOUR_PX / 60;
	const SCROLL_HOUR = 7;
	const hours = Array.from({ length: 24 }, (_, h) => h);

	let scroller: HTMLDivElement | undefined = $state();
	let scrolledFor = '';

	const columns = $derived(
		days.map((day) => ({
			day,
			key: day.toDateString(),
			today: sameDay(day, now),
			placed: layoutDay(actions, day)
		}))
	);
	const nowTop = $derived(minutesInDay(now, now) * PX_PER_MIN);
	const showsToday = $derived(columns.some((c) => c.today));

	$effect(() => {
		const key = days.map((d) => d.toDateString()).join('|');
		if (!scroller || key === scrolledFor) return;
		scrolledFor = key;
		const target = showsToday ? Math.max(0, nowTop - HOUR_PX * 2) : SCROLL_HOUR * HOUR_PX;
		scroller.scrollTop = target;
	});

	console.assert(DAY_MINUTES * PX_PER_MIN === 24 * HOUR_PX, 'grid height matches 24 hours');
</script>

<div class="grid" style:--cols={days.length}>
	{#if showHeader}
		<div class="head" aria-hidden="true">
			<span class="gutter"></span>
			{#each columns as c (c.key)}
				<span class="day-head" class:today={c.today}>
					<span>{formatWeekday(c.day)}</span>
					<span class="num date">{c.day.getDate()}</span>
				</span>
			{/each}
		</div>
	{/if}
	<div class="scroll" bind:this={scroller}>
		<div class="body" style:height="{24 * HOUR_PX}px">
			<div class="gutter labels" aria-hidden="true">
				{#each hours as h (h)}
					{#if h > 0}<span class="num" style:top="{h * HOUR_PX}px">{String(h).padStart(2, '0')}:00</span>{/if}
				{/each}
			</div>
			{#each columns as c (c.key)}
				<div class="col" class:today={c.today} role="list" aria-label={c.day.toDateString()}>
					{#each c.placed as p (p.item.id)}
						<div role="listitem">
							<ActionBlock
								action={p.item}
								top={p.startMin * PX_PER_MIN}
								height={(p.endMin - p.startMin) * PX_PER_MIN - 2}
								col={p.col}
								cols={p.cols}
								{onopen}
							/>
						</div>
					{/each}
					{#if c.today}
						<div class="now" style:top="{nowTop}px" aria-label="Now {formatTime(now)}"></div>
					{/if}
				</div>
			{/each}
		</div>
	</div>
</div>

<style>
	.grid {
		--gutter: 48px;
		display: flex;
		flex-direction: column;
		min-height: 0;
		flex: 1;
	}
	.head {
		display: grid;
		grid-template-columns: var(--gutter) repeat(var(--cols), 1fr);
		padding-bottom: 8px;
		border-bottom: 1px solid var(--cal-line);
	}
	.day-head {
		display: flex;
		flex-direction: column;
		align-items: center;
		font-size: 12px;
		color: var(--cal-muted);
	}
	.day-head .date {
		font-size: 18px;
		font-weight: 600;
		color: var(--cal-ink);
	}
	.day-head.today .date {
		color: var(--now);
	}
	.scroll {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		overscroll-behavior: contain;
	}
	.body {
		position: relative;
		display: grid;
		grid-template-columns: var(--gutter) repeat(var(--cols), 1fr);
		background-image: repeating-linear-gradient(
			to bottom,
			var(--cal-line) 0 1px,
			transparent 1px 64px
		);
		background-position: var(--gutter) 0;
	}
	.labels {
		position: relative;
	}
	.labels span {
		position: absolute;
		right: 8px;
		transform: translateY(-50%);
		font-size: 11px;
		color: var(--cal-muted);
	}
	.col {
		position: relative;
		border-left: 1px solid var(--cal-line);
	}
	.col.today {
		background: color-mix(in srgb, var(--now) 4%, transparent);
	}
	.now {
		position: absolute;
		left: -4px;
		right: 0;
		height: 2px;
		background: var(--now);
		pointer-events: none;
		z-index: 3;
	}
	.now::before {
		content: '';
		position: absolute;
		left: 0;
		top: -3px;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--now);
	}
</style>
