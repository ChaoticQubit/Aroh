<script lang="ts">
	import './calendar.css';
	import { moveAction, type ActionView, type SettableStatus } from './api';
	import { formatDayTitle, formatRange, fromLocalInputs, goalHue, toDateInput, toTimeInput } from './time';

	interface Props {
		action: ActionView | null;
		onclose: () => void;
		onchange: (row: ActionView) => void;
		onstatus: (action: ActionView, status: SettableStatus) => void;
	}

	let { action, onclose, onchange, onstatus }: Props = $props();

	let dialog: HTMLDialogElement | undefined = $state();
	let date = $state('');
	let start = $state('');
	let end = $state('');
	let moveError = $state<string | null>(null);
	let saving = $state(false);
	let lastId: string | null = null;

	function hostOf(u: string): string | null {
		try {
			const url = new URL(u);
			return url.protocol === 'https:' || url.protocol === 'http:' ? url.host : null;
		} catch {
			return null;
		}
	}

	const links = $derived(
		[...new Set((action?.notes ?? '').match(/https?:\/\/[^\s<>"']+/g) ?? [])]
			.map((href) => ({ href, host: hostOf(href) }))
			.filter((l): l is { href: string; host: string } => l.host !== null)
	);
	const open = $derived(action !== null && (action.status === 'planned' || action.status === 'missed'));

	$effect(() => {
		if (!dialog) return;
		if (action && action.id !== lastId) {
			date = toDateInput(action.start_at);
			start = toTimeInput(action.start_at);
			end = toTimeInput(action.end_at);
			moveError = null;
		}
		lastId = action?.id ?? null;
		if (action && !dialog.open) dialog.showModal();
		if (!action && dialog.open) dialog.close();
	});

	async function save(e: SubmitEvent) {
		e.preventDefault();
		if (!action) return;
		const startAt = fromLocalInputs(date, start);
		const endAt = fromLocalInputs(date, end);
		if (!startAt || !endAt) {
			moveError = 'Enter a date and both times.';
			return;
		}
		if (new Date(endAt) <= new Date(startAt)) {
			moveError = 'End must be after start.';
			return;
		}
		saving = true;
		try {
			onchange(await moveAction(action.id, startAt, endAt));
			moveError = null;
			onclose();
		} catch (err) {
			moveError = `Couldn't move it. ${String(err)}`;
		} finally {
			saving = false;
		}
	}

	function mark(status: SettableStatus) {
		if (!action) return;
		onstatus(action, status);
		onclose();
	}
</script>

<dialog
	bind:this={dialog}
	class="sheet"
	aria-labelledby="sheet-title"
	onclose={onclose}
	onclick={(e) => e.target === dialog && onclose()}
>
	{#if action}
		<div class="inner" style:--hue={goalHue(action.goal_color_index)}>
			<span class="grip" aria-hidden="true"></span>
			<header>
				<h2 id="sheet-title">{action.title}</h2>
				<button type="button" class="close" onclick={onclose} aria-label="Close">✕</button>
			</header>
			<p class="meta">
				{#if action.goal_title}<span class="goal"><span class="swatch" aria-hidden="true"></span>{action.goal_title}</span>{/if}
				<span class="num">{formatDayTitle(new Date(action.start_at))}, {formatRange(action.start_at, action.end_at)}</span>
				{#if action.status !== 'planned'}<span class="state">{action.status[0].toUpperCase() + action.status.slice(1)}</span>{/if}
			</p>
			{#if action.notes}<p class="notes">{action.notes}</p>{/if}
			{#if links.length > 0}
				<ul class="links">
					{#each links as link (link.href)}
						<li><a href={link.href} target="_blank" rel="noopener noreferrer">{link.host}</a></li>
					{/each}
				</ul>
			{/if}

			<div class="status">
				{#if open}
					<button type="button" class="primary" onclick={() => mark('done')}>Done</button>
					<button type="button" class="quiet" onclick={() => mark('skipped')}>Skip</button>
				{:else}
					<button type="button" class="quiet" onclick={() => mark('planned')}>Mark as planned</button>
				{/if}
			</div>

			<form class="move" onsubmit={save}>
				<h3>Move</h3>
				<label>
					<span>Date</span>
					<input type="date" bind:value={date} required />
				</label>
				<div class="times">
					<label>
						<span>Start</span>
						<input type="time" bind:value={start} required />
					</label>
					<label>
						<span>End</span>
						<input type="time" bind:value={end} required />
					</label>
				</div>
				{#if moveError}<p class="error" role="alert">{moveError}</p>{/if}
				<button type="submit" class="quiet wide" disabled={saving}>{saving ? 'Saving' : 'Save time'}</button>
			</form>
		</div>
	{/if}
</dialog>

<style>
	.sheet {
		margin: auto 0 0;
		width: 100%;
		max-width: 100%;
		max-height: 88dvh;
		padding: 0;
		border: 0;
		border-radius: var(--sheet-radius) var(--sheet-radius) 0 0;
		background: var(--cal-raised);
		color: var(--cal-ink);
		box-shadow: 0 -8px 32px rgba(0, 0, 0, 0.2);
		overflow-y: auto;
		overscroll-behavior: contain;
	}
	.sheet::backdrop {
		background: rgba(0, 0, 0, 0.36);
	}
	.sheet[open] {
		animation: rise var(--ease);
	}
	@keyframes rise {
		from {
			transform: translateY(24px);
			opacity: 0;
		}
	}
	@media (min-width: 840px) {
		.sheet {
			margin: 0 0 0 auto;
			width: 400px;
			height: 100dvh;
			max-height: 100dvh;
			border-radius: 0;
			box-shadow: -8px 0 32px rgba(0, 0, 0, 0.16);
		}
		.grip {
			display: none;
		}
	}
	.inner {
		display: flex;
		flex-direction: column;
		gap: 16px;
		padding: 12px calc(env(safe-area-inset-right) + 20px) calc(env(safe-area-inset-bottom) + 24px)
			calc(env(safe-area-inset-left) + 20px);
	}
	.grip {
		align-self: center;
		width: 36px;
		height: 5px;
		border-radius: 999px;
		background: var(--cal-line);
	}
	header {
		display: flex;
		align-items: flex-start;
		gap: 12px;
	}
	h2 {
		flex: 1;
		margin: 0;
		font-size: 20px;
		line-height: 26px;
		font-weight: 600;
		overflow-wrap: anywhere;
	}
	h3 {
		margin: 0;
		font-size: 15px;
		font-weight: 600;
	}
	.close {
		width: var(--tap);
		height: var(--tap);
		margin: -8px -8px 0 0;
		background: none;
		border: 0;
		font-size: 18px;
		color: var(--cal-muted);
		cursor: pointer;
	}
	.meta {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin: 0;
		font-size: 15px;
		color: var(--cal-muted);
	}
	.goal {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		color: var(--cal-ink);
	}
	.swatch {
		width: 10px;
		height: 10px;
		border-radius: 2px;
		background: var(--hue);
	}
	.state {
		font-weight: 600;
	}
	.notes {
		margin: 0;
		font-size: 16px;
		line-height: 24px;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.links {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.links a {
		display: inline-flex;
		align-items: center;
		min-height: var(--tap);
		color: var(--cal-ink);
		font-weight: 600;
	}
	.status {
		display: flex;
		gap: 8px;
	}
	.status button {
		flex: 1;
	}
	button.primary,
	button.quiet {
		min-height: var(--tap);
		padding: 0 16px;
		border-radius: 999px;
		font-size: 16px;
		cursor: pointer;
	}
	.primary {
		background: var(--cal-ink);
		color: var(--cal-raised);
		border: 1px solid var(--cal-ink);
		font-weight: 600;
	}
	.quiet {
		background: transparent;
		border: 1px solid var(--cal-line);
	}
	.quiet:disabled {
		opacity: 0.6;
	}
	.move {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding-top: 16px;
		border-top: 1px solid var(--cal-line);
	}
	.times {
		display: flex;
		gap: 12px;
	}
	.times label {
		flex: 1;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 13px;
		color: var(--cal-muted);
	}
	input {
		min-height: var(--tap);
		padding: 0 12px;
		font-size: 16px;
		font-variant-numeric: tabular-nums;
		color: var(--cal-ink);
		background: var(--cal-surface);
		border: 1px solid var(--cal-line);
		border-radius: 8px;
	}
	.error {
		margin: 0;
		font-size: 14px;
		color: var(--cal-danger);
	}
	.wide {
		width: 100%;
	}
	button:focus-visible,
	a:focus-visible,
	input:focus-visible {
		outline: 2px solid var(--cal-accent);
		outline-offset: 2px;
	}
</style>
