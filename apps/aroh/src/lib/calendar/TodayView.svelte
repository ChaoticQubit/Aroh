<script lang="ts">
	import './calendar.css';
	import { listen } from '@tauri-apps/api/event';
	import { ensureNotificationPermission, listActions, setStatus, type ActionView, type SettableStatus } from './api';
	import { formatDayTitle, formatTime, localRange } from './time';
	import ActionRow from './ActionRow.svelte';
	import PluginSlot from '../plugins/PluginSlot.svelte';
	import ActionSheet from './ActionSheet.svelte';

	interface Props {
		newGoalHref?: string;
		focusActionId?: string | null;
	}

	let { newGoalHref = '/goals/new', focusActionId = null }: Props = $props();

	const UNDO_MS = 5000;

	let now = $state(new Date());
	let actions = $state<ActionView[]>([]);
	let loaded = $state(false);
	let error = $state<string | null>(null);
	let undo = $state<{ id: string; label: string } | null>(null);
	let sheetId = $state<string | null>(null);
	let undoTimer: ReturnType<typeof setTimeout> | undefined;

	const sorted = $derived([...actions].sort((a, b) => a.start_at.localeCompare(b.start_at) || (a.seq ?? 0) - (b.seq ?? 0)));
	const openList = $derived(sorted.filter((a) => a.status === 'planned' || a.status === 'missed'));
	const closedList = $derived(sorted.filter((a) => a.status === 'done' || a.status === 'skipped'));
	const nowCard = $derived(
		openList.find((a) => new Date(a.start_at) <= now && new Date(a.end_at) > now) ??
			openList.find((a) => new Date(a.start_at) > now) ??
			null
	);
	const rest = $derived(openList.filter((a) => a.id !== nowCard?.id));
	const pivot = $derived(nowCard ? new Date(nowCard.start_at) : now);
	const restBefore = $derived(rest.filter((a) => new Date(a.start_at) < pivot));
	const restAfter = $derived(rest.filter((a) => new Date(a.start_at) >= pivot));
	const doneCount = $derived(actions.filter((a) => a.status === 'done').length);
	const countable = $derived(actions.filter((a) => a.status !== 'skipped').length);
	const progress = $derived(countable === 0 ? 0 : doneCount / countable);
	const sheetAction = $derived(actions.find((a) => a.id === sheetId) ?? null);

	async function load() {
		const day = new Date();
		const { from, to } = localRange(day);
		try {
			actions = await listActions(from, to);
			error = null;
		} catch (e) {
			error = `Couldn't load today. ${String(e)}`;
		} finally {
			loaded = true;
		}
	}

	function replace(row: ActionView) {
		actions = actions.map((a) => (a.id === row.id ? row : a));
	}

	async function changeStatus(action: ActionView, status: SettableStatus) {
		const before = action;
		replace({ ...action, status, done_at: status === 'done' ? new Date().toISOString() : null });
		clearTimeout(undoTimer);
		undo = status === 'planned' ? null : { id: action.id, label: status === 'done' ? `Done: ${action.title}` : `Skipped: ${action.title}` };
		if (undo) undoTimer = setTimeout(() => (undo = null), UNDO_MS);
		try {
			replace(await setStatus(action.id, status));
		} catch (e) {
			replace(before);
			undo = null;
			error = `Couldn't save. ${String(e)}`;
		}
	}

	function undoLast() {
		const target = actions.find((a) => a.id === undo?.id);
		undo = null;
		clearTimeout(undoTimer);
		if (target) changeStatus(target, 'planned');
	}

	$effect(() => {
		load();
		const tick = setInterval(() => {
			const next = new Date();
			if (next.getDate() !== now.getDate()) load();
			now = next;
		}, 60_000);
		const onFocus = () => {
			now = new Date();
			load();
		};
		const onVisible = () => document.visibilityState === 'visible' && onFocus();
		window.addEventListener('focus', onFocus);
		document.addEventListener('visibilitychange', onVisible);
		let unlisten: (() => void) | undefined;
		let stopped = false;
		listen('aroh://actions-changed', () => load())
			.then((u) => (stopped ? u() : (unlisten = u)))
			.catch(() => {});
		return () => {
			stopped = true;
			clearInterval(tick);
			clearTimeout(undoTimer);
			window.removeEventListener('focus', onFocus);
			document.removeEventListener('visibilitychange', onVisible);
			unlisten?.();
		};
	});

	let permissionAsked = false;
	$effect(() => {
		if (permissionAsked || actions.length === 0) return;
		permissionAsked = true;
		ensureNotificationPermission().catch(() => {});
	});

	let focusedOnce = false;
	$effect(() => {
		if (focusedOnce || !focusActionId || !actions.some((a) => a.id === focusActionId)) return;
		focusedOnce = true;
		sheetId = focusActionId;
	});
</script>

<section class="today" aria-labelledby="today-title">
	<header class="head">
		<h1 id="today-title">{formatDayTitle(now)}</h1>
		{#if actions.length > 0}
			<p class="count num">{doneCount} of {countable} done</p>
		{/if}
	</header>
	{#if actions.length > 0}
		<div
			class="bar"
			role="progressbar"
			aria-label="Done today"
			aria-valuemin="0"
			aria-valuemax={countable}
			aria-valuenow={doneCount}
		>
			<span style:width="{progress * 100}%"></span>
		</div>
	{/if}

	<PluginSlot name="today.top" />

	{#if error}
		<p class="error" role="alert">{error} <button type="button" class="link" onclick={load}>Retry</button></p>
	{/if}

	{#if !loaded}
		<ul class="list" aria-busy="true">
			{#each [0, 1, 2] as i (i)}<li class="skeleton"></li>{/each}
		</ul>
	{:else if actions.length === 0 && !error}
		<p class="empty">Nothing planned today. <a href={newGoalHref}>Set a goal</a></p>
	{:else}
		<ul class="list">
			{#each restBefore as a (a.id)}
				<ActionRow action={a} onstatus={changeStatus} onopen={(x) => (sheetId = x.id)} />
			{/each}
			{#if nowCard}
				<ActionRow action={nowCard} now onstatus={changeStatus} onopen={(x) => (sheetId = x.id)} />
			{/if}
			<li class="now-line" aria-label="Now {formatTime(now)}"><span class="num">{formatTime(now)}</span></li>
			{#each restAfter as a (a.id)}
				<ActionRow action={a} onstatus={changeStatus} onopen={(x) => (sheetId = x.id)} />
			{/each}
			{#if openList.length === 0}
				<li class="all-done">All done for today.</li>
			{/if}
		</ul>
		{#if closedList.length > 0}
			<details class="closed">
				<summary>Done and skipped <span class="num">({closedList.length})</span></summary>
				<ul class="list">
					{#each closedList as a (a.id)}
						<ActionRow action={a} onstatus={changeStatus} onopen={(x) => (sheetId = x.id)} />
					{/each}
				</ul>
			</details>
		{/if}
	{/if}

	{#if undo}
		<div class="toast" role="status">
			<span class="toast-text">{undo.label}</span>
			<button type="button" onclick={undoLast}>Undo</button>
		</div>
	{/if}
</section>

<ActionSheet
	action={sheetAction}
	onclose={() => (sheetId = null)}
	onchange={(row) => replace(row)}
	onstatus={(a, s) => changeStatus(a, s)}
/>

<style>
	.today {
		max-width: 640px;
		margin: 0 auto;
		color: var(--cal-ink);
	}
	.head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		flex-wrap: wrap;
	}
	h1 {
		font-size: 24px;
		line-height: 30px;
		font-weight: 600;
		letter-spacing: -0.01em;
		margin: 0;
	}
	.count {
		margin: 0;
		font-size: 15px;
		color: var(--cal-muted);
	}
	.bar {
		height: 6px;
		margin: 12px 0 16px;
		background: var(--cal-line);
		border-radius: 999px;
		overflow: hidden;
	}
	.bar span {
		display: block;
		height: 100%;
		background: var(--cal-ink);
		border-radius: inherit;
		transition: width var(--ease);
	}
	.list {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin: 0;
		padding: 0;
	}
	.now-line {
		list-style: none;
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--now);
		font-size: 12px;
		font-weight: 600;
	}
	.now-line::after {
		content: '';
		flex: 1;
		height: 2px;
		background: var(--now);
	}
	.skeleton {
		list-style: none;
		height: 64px;
		border-radius: 8px;
		background: var(--cal-line);
	}
	.empty,
	.all-done {
		list-style: none;
		font-size: 16px;
		color: var(--cal-muted);
		padding: 24px 0;
	}
	.empty a {
		color: var(--cal-ink);
		font-weight: 600;
		min-height: var(--tap);
		display: inline-flex;
		align-items: center;
	}
	.error {
		font-size: 15px;
		color: var(--cal-ink);
		padding: 12px;
		border: 1px solid var(--cal-danger);
		border-radius: 8px;
	}
	.link {
		background: none;
		border: 0;
		padding: 0 8px;
		min-height: var(--tap);
		font-weight: 600;
		text-decoration: underline;
		cursor: pointer;
	}
	.closed {
		margin-top: 24px;
	}
	.closed summary {
		min-height: var(--tap);
		display: flex;
		align-items: center;
		font-size: 15px;
		font-weight: 600;
		color: var(--cal-muted);
		cursor: pointer;
	}
	.closed .list {
		margin-top: 8px;
	}
	.toast {
		position: fixed;
		left: 50%;
		bottom: calc(env(safe-area-inset-bottom) + var(--tabbar, 56px) + 16px);
		transform: translateX(-50%);
		width: min(560px, calc(100% - 32px));
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 4px 4px 4px 16px;
		background: var(--cal-ink);
		color: var(--cal-raised);
		border-radius: 12px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18);
		font-size: 15px;
	}
	.toast-text {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.toast button {
		min-height: var(--tap);
		padding: 0 16px;
		background: none;
		border: 0;
		color: inherit;
		font-weight: 600;
		cursor: pointer;
	}
	button:focus-visible,
	summary:focus-visible,
	a:focus-visible {
		outline: 2px solid var(--cal-accent);
		outline-offset: 2px;
	}
</style>
