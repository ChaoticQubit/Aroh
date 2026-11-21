<script lang="ts">
	import './calendar.css';
	import { listen } from '@tauri-apps/api/event';
	import { listActions, setStatus, type ActionView, type SettableStatus } from './api';
	import ActionSheet from './ActionSheet.svelte';
	import TimeGrid from './TimeGrid.svelte';
	import { addDays, formatDayTitle, formatMonth, formatShortDayTitle, formatWeekday, localRange, sameDay, startOfDay, startOfWeek } from './time';

	type Mode = 'day' | 'week';

	const SWIPE_PX = 60;
	const WIDE_QUERY = '(min-width: 840px)';

	let anchor = $state(startOfDay(new Date()));
	let now = $state(new Date());
	let wide = $state(false);
	let mode = $state<Mode>('day');
	let actions = $state<ActionView[]>([]);
	let error = $state<string | null>(null);
	let sheetId = $state<string | null>(null);
	let swipeStart: { x: number; y: number; id: number } | null = null;

	const weekStart = $derived(startOfWeek(anchor));
	const strip = $derived(Array.from({ length: 7 }, (_, i) => addDays(weekStart, i)));
	const days = $derived(mode === 'week' ? strip : [anchor]);
	const rangeKey = $derived(`${days[0].toISOString()}|${days.length}`);
	const title = $derived(mode === 'week' ? formatMonth(weekStart) : formatShortDayTitle(anchor));
	const sheetAction = $derived(actions.find((a) => a.id === sheetId) ?? null);

	let loadSeq = 0;
	async function load(first: Date, count: number) {
		const seq = ++loadSeq;
		const { from, to } = localRange(first, count);
		try {
			const rows = await listActions(from, to);
			if (seq !== loadSeq) return;
			actions = rows;
			error = null;
		} catch (e) {
			if (seq === loadSeq) error = `Couldn't load the calendar. ${String(e)}`;
		}
	}

	function reload() {
		load(days[0], days.length);
	}

	function step(dir: -1 | 1) {
		anchor = addDays(anchor, dir * (mode === 'week' ? 7 : 1));
	}

	function goToday() {
		anchor = startOfDay(new Date());
	}

	function replace(row: ActionView) {
		actions = actions.map((a) => (a.id === row.id ? row : a));
	}

	async function changeStatus(action: ActionView, status: SettableStatus) {
		const before = action;
		replace({ ...action, status });
		try {
			replace(await setStatus(action.id, status));
		} catch (e) {
			replace(before);
			error = `Couldn't save. ${String(e)}`;
		}
	}

	function onPointerDown(e: PointerEvent) {
		if (mode !== 'day' || !e.isPrimary) return;
		swipeStart = { x: e.clientX, y: e.clientY, id: e.pointerId };
	}

	function onPointerUp(e: PointerEvent) {
		const s = swipeStart;
		swipeStart = null;
		if (!s || s.id !== e.pointerId) return;
		const dx = e.clientX - s.x;
		const dy = e.clientY - s.y;
		if (Math.abs(dx) >= SWIPE_PX && Math.abs(dx) > Math.abs(dy) * 1.5) step(dx < 0 ? 1 : -1);
	}

	function onKey(e: KeyboardEvent) {
		if (!wide || e.metaKey || e.ctrlKey || e.altKey) return;
		const t = e.target as HTMLElement | null;
		if (t && (t.closest('input, textarea, select, dialog[open]') || t.isContentEditable)) return;
		const k = e.key.toLowerCase();
		if (k === 'd') mode = 'day';
		else if (k === 'w') mode = 'week';
		else if (k === 't') goToday();
		else if (e.key === 'ArrowLeft') step(-1);
		else if (e.key === 'ArrowRight') step(1);
		else return;
		e.preventDefault();
	}

	$effect(() => {
		const mq = window.matchMedia(WIDE_QUERY);
		const apply = () => {
			wide = mq.matches;
			mode = mq.matches ? 'week' : 'day';
		};
		apply();
		mq.addEventListener('change', apply);
		const tick = setInterval(() => (now = new Date()), 60_000);
		const onFocus = () => {
			now = new Date();
			reload();
		};
		window.addEventListener('focus', onFocus);
		let unlisten: (() => void) | undefined;
		let stopped = false;
		listen('aroh://actions-changed', () => reload())
			.then((u) => (stopped ? u() : (unlisten = u)))
			.catch(() => {});
		return () => {
			stopped = true;
			mq.removeEventListener('change', apply);
			clearInterval(tick);
			window.removeEventListener('focus', onFocus);
			unlisten?.();
		};
	});

	$effect(() => {
		rangeKey;
		load(days[0], days.length);
	});
</script>

<svelte:window onkeydown={onKey} />

<section class="cal" aria-labelledby="cal-title">
	<header class="bar">
		<h1 id="cal-title">{title}</h1>
		<div class="tools">
			{#if wide}
				<div class="toggle" role="group" aria-label="View">
					<button type="button" aria-pressed={mode === 'day'} onclick={() => (mode = 'day')}>Day</button>
					<button type="button" aria-pressed={mode === 'week'} onclick={() => (mode = 'week')}>Week</button>
				</div>
			{/if}
			<button type="button" class="nav" onclick={() => step(-1)} aria-label="Previous {mode}">‹</button>
			<button type="button" class="today-btn" onclick={goToday}>Today</button>
			<button type="button" class="nav" onclick={() => step(1)} aria-label="Next {mode}">›</button>
		</div>
	</header>

	{#if mode === 'day'}
		<nav class="strip" aria-label="Week">
			{#each strip as d (d.toDateString())}
				<button
					type="button"
					class:selected={sameDay(d, anchor)}
					class:today={sameDay(d, now)}
					aria-current={sameDay(d, anchor) ? 'date' : undefined}
					aria-label={formatDayTitle(d)}
					onclick={() => (anchor = d)}
				>
					<span class="wd">{formatWeekday(d)}</span>
					<span class="num dn">{d.getDate()}</span>
				</button>
			{/each}
		</nav>
	{/if}

	{#if error}
		<p class="error" role="alert">{error} <button type="button" class="link" onclick={reload}>Retry</button></p>
	{/if}

	<div
		class="surface"
		onpointerdown={onPointerDown}
		onpointerup={onPointerUp}
		onpointercancel={() => (swipeStart = null)}
		role="presentation"
	>
		<TimeGrid {days} {actions} {now} showHeader={mode === 'week'} onopen={(a) => (sheetId = a.id)} />
	</div>
</section>

<ActionSheet action={sheetAction} onclose={() => (sheetId = null)} onchange={replace} onstatus={changeStatus} />

<style>
	.cal {
		display: flex;
		flex-direction: column;
		height: 100%;
		color: var(--cal-ink);
		background: var(--cal-surface);
	}
	.bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		min-height: 48px;
	}
	h1 {
		margin: 0;
		font-size: 20px;
		line-height: 26px;
		font-weight: 600;
		letter-spacing: -0.01em;
	}
	@media (min-width: 840px) {
		h1 {
			font-size: 24px;
		}
	}
	.tools {
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.tools button {
		min-height: var(--tap);
		min-width: var(--tap);
		background: transparent;
		border: 1px solid var(--cal-line);
		border-radius: 999px;
		font-size: 15px;
		cursor: pointer;
	}
	.nav {
		font-size: 22px;
		line-height: 1;
	}
	.today-btn {
		padding: 0 14px;
		font-weight: 600;
	}
	.toggle {
		display: flex;
		margin-right: 8px;
		border: 1px solid var(--cal-line);
		border-radius: 999px;
		padding: 2px;
	}
	.toggle button {
		border: 0;
		padding: 0 14px;
		min-height: 40px;
	}
	.toggle button[aria-pressed='true'] {
		background: var(--cal-ink);
		color: var(--cal-raised);
		font-weight: 600;
	}
	.strip {
		display: grid;
		grid-template-columns: repeat(7, 1fr);
		gap: 4px;
		padding: 8px 0;
	}
	.strip button {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 2px;
		min-height: 52px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: 12px;
		cursor: pointer;
	}
	.strip .wd {
		font-size: 12px;
		color: var(--cal-muted);
	}
	.strip .dn {
		font-size: 17px;
		font-weight: 600;
	}
	.strip button.today .dn {
		color: var(--now);
	}
	.strip button.selected {
		background: var(--cal-ink);
		border-color: var(--cal-ink);
	}
	.strip button.selected .wd,
	.strip button.selected .dn {
		color: var(--cal-raised);
	}
	.surface {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		touch-action: pan-y;
	}
	.error {
		font-size: 15px;
		margin: 4px 0 8px;
		padding: 8px 12px;
		border: 1px solid var(--cal-danger);
		border-radius: 8px;
	}
	.link {
		background: none;
		border: 0;
		min-height: var(--tap);
		padding: 0 8px;
		font-weight: 600;
		text-decoration: underline;
		cursor: pointer;
	}
	button:focus-visible {
		outline: 2px solid var(--cal-accent);
		outline-offset: 2px;
	}
</style>
