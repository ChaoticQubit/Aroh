<script lang="ts">
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import Minus from '@lucide/svelte/icons/minus';
	import Plus from '@lucide/svelte/icons/plus';
	import Lock from '@lucide/svelte/icons/lock';
	import NotebookText from '@lucide/svelte/icons/notebook-text';
	import { Button } from '$lib/ui/primitives/button';
	import { Switch } from '$lib/ui/primitives/switch';
	import { Input } from '$lib/ui/primitives/input';
	import * as Select from '$lib/ui/primitives/select';
	import StatusChip from '../StatusChip.svelte';
	import Segmented from '../Segmented.svelte';
	import { logState } from '$lib/ui/status';
	import { getClock } from '$lib/ui/clock';
	import { formatDay, formatDuration, formatTime } from '$lib/ui/format';
	import type { ActionView, DeleteScope } from '$lib/ui/contract';
	import type { ActionEdit, GoalChoice, NewAction } from '$lib/ui/screens/types';

	/**
	 * Body of the action edit sheet: log the status (Not done, Done, Skipped) in any status, move the
	 * day and start time, change the duration in 15 minute steps, toggle Fixed time, save, or delete
	 * after a confirm step (Q78: a status is shown, never a lock). Emits `onstatus` with the id and
	 * the picked status, `onsave` with `{ id, start, end }`, `onlock` with the id and the new Fixed
	 * time, `ondelete` with the id and the scope and `onclose` on cancel. `confirming` opens on the
	 * delete confirm, which asks this session or this and all future for a series session (Q89).
	 * With `oncreate` it adds a new item instead (B121): a focused title, a goal from `goals` or
	 * none, the same day, start and duration rows, and Add emits the `NewAction`.
	 */
	let {
		action,
		confirming = false,
		goals = [],
		onstatus,
		onsave,
		onlock,
		ondelete,
		oncreate,
		onclose
	}: {
		action: ActionView;
		confirming?: boolean;
		goals?: GoalChoice[];
		onstatus?: (id: string, status: LogPick) => void;
		onsave?: (edit: ActionEdit) => void;
		onlock?: (id: string, locked: boolean) => void;
		ondelete?: (id: string, scope: DeleteScope) => void;
		oncreate?: (item: NewAction) => void;
		onclose?: () => void;
	} = $props();

	const NO_GOAL = '';
	let title = $state('');
	let goalId = $state(NO_GOAL);
	const goalLabel = $derived(goals.find((g) => g.id === goalId)?.title ?? 'No goal');

	type LogPick = 'planned' | 'done' | 'skipped';
	const { now, zone } = getClock();
	const LOG: { value: LogPick; label: string }[] = [
		{ value: 'planned', label: 'Not done' },
		{ value: 'done', label: 'Done' },
		{ value: 'skipped', label: 'Skipped' }
	];
	let status = $derived<ActionView['status']>(action.status);
	const chip = $derived(logState({ ...action, status }, now, zone));
	const STEP = 15;
	const MINUTE_MS = 60_000;
	const titleId = $props.id();
	let fixed = $derived(action.locked);
	let start = $derived(Date.parse(action.start));
	let minutes = $derived(Math.round((Date.parse(action.end) - Date.parse(action.start)) / MINUTE_MS));
	let asking = $derived(confirming);
	const startIso = $derived(new Date(start).toISOString());
	const endIso = $derived(new Date(start + minutes * MINUTE_MS).toISOString());
	const changed = $derived(startIso !== new Date(action.start).toISOString() || endIso !== new Date(action.end).toISOString());

	/** Moves the start by `delta` minutes, keeping the duration. */
	function move(delta: number) {
		start += delta * MINUTE_MS;
	}

	/** Changes the duration by `delta` minutes, never below one step or above 8 hours. */
	function resize(delta: number) {
		minutes = Math.min(480, Math.max(STEP, minutes + delta));
	}
</script>

<div class="flex flex-col gap-4" role="group" aria-labelledby={titleId}>
	{#if oncreate}
	<h2 id={titleId} class="text-lg font-semibold">New item</h2>
	<Input aria-label="Title" placeholder="Title" class="h-11 text-lg" maxlength={200} autofocus bind:value={title} onkeydown={(e) => e.key === 'Enter' && title.trim() && oncreate({ title: title.trim(), start: startIso, minutes, goalId: goalId || null })} />
	{#if goals.length}
		<Select.Root type="single" bind:value={goalId}>
			<Select.Trigger class="h-11! w-full text-lg" aria-label="Goal">{goalLabel}</Select.Trigger>
			<Select.Content>
				<Select.Item value={NO_GOAL} label="No goal" class="min-h-11">No goal</Select.Item>
				{#each goals as g (g.id)}<Select.Item value={g.id} label={g.title} class="min-h-11">{g.title}</Select.Item>{/each}
			</Select.Content>
		</Select.Root>
	{/if}
	{:else}
	<div class="flex flex-col gap-1">
		<div class="flex items-center gap-2">
			<span class="size-2.5 shrink-0 rounded-full" style="background: var(--cat-{action.category})" aria-hidden="true"></span>
			<h2 id={titleId} class="truncate text-lg font-semibold">{action.title}</h2>
			{#if fixed}<Lock class="size-4 shrink-0 text-muted" aria-label="Fixed time" />{/if}
			{#if chip && status !== 'done'}<StatusChip tone={chip.tone} label={chip.label} />{/if}
		</div>
		<p class="truncate text-sm text-muted">{action.goalTitle}</p>
	</div>
	<Button variant="outline" size="touch" href="/actions/{action.id}"><NotebookText />Open session card</Button>
	{#if onstatus}
		<div class="flex items-center justify-between gap-3">
			<span class="text-sm text-muted">Logged</span>
			<Segmented
				label="Log {action.title}"
				options={LOG}
				bind:value={
					() => (status === 'missed' ? ('' as LogPick) : (status as LogPick)),
					(v: LogPick) => {
						status = v;
						onstatus(action.id, v);
					}
				}
			/>
		</div>
	{/if}
	{/if}

	<dl class="flex flex-col divide-y divide-border rounded-sm border border-border">
		{#each [{ name: 'Day', value: formatDay(startIso, zone), less: () => move(-1440), more: () => move(1440), step: 'one day' }, { name: 'Start', value: formatTime(startIso, zone), less: () => move(-STEP), more: () => move(STEP), step: `${STEP} min` }, { name: 'Duration', value: formatDuration(minutes), less: () => resize(-STEP), more: () => resize(STEP), step: `${STEP} min` }] as row (row.name)}
			<div class="flex items-center gap-2 py-0.5 pr-0.5 pl-3">
				<dt class="w-20 shrink-0 text-sm text-muted">{row.name}</dt>
				<dd class="flex flex-1 items-center justify-end gap-1">
					<Button variant="ghost" size="icon-touch" aria-label="{row.name} earlier or shorter by {row.step}" onclick={row.less}>
						{#if row.name === 'Day'}<ChevronLeft />{:else}<Minus />{/if}
					</Button>
					<span class="min-w-24 text-center text-sm font-medium tabular-nums" aria-live="polite">{row.value}</span>
					<Button variant="ghost" size="icon-touch" aria-label="{row.name} later or longer by {row.step}" onclick={row.more}>
						{#if row.name === 'Day'}<ChevronRight />{:else}<Plus />{/if}
					</Button>
				</dd>
			</div>
		{/each}
	</dl>
	{#if oncreate}
		<p class="text-sm text-muted tabular-nums">{formatTime(startIso, zone)} - {formatTime(endIso, zone)}</p>
		<div class="flex items-center justify-end gap-2">
			<Button variant="outline" size="touch" onclick={() => onclose?.()}>Cancel</Button>
			<Button size="touch" disabled={!title.trim()} onclick={() => oncreate({ title: title.trim(), start: startIso, minutes, goalId: goalId || null })}>Add</Button>
		</div>
	{:else}
	<label class="flex min-h-12 cursor-pointer items-center gap-3" for="{titleId}-fixed">
		<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Fixed time</span><span class="text-xs text-muted">Agents and Busy days never move it.</span></span>
		<Switch
			id="{titleId}-fixed"
			checked={fixed}
			onCheckedChange={(v) => {
				fixed = v;
				onlock?.(action.id, v);
			}}
		/>
	</label>
	<p class="text-sm text-muted tabular-nums">
		{formatTime(startIso, zone)} - {formatTime(endIso, zone)}{#if changed}<span class="text-fg"> · was {formatTime(action.start, zone)} - {formatTime(action.end, zone)}</span>{/if}
	</p>

	{#if asking}
		<div class="flex flex-col gap-3 rounded-sm border border-status-missed/40 bg-status-missed/10 p-3" role="alertdialog" aria-label="Confirm delete">
			<p class="text-sm text-fg">Delete <span class="font-medium">{action.title}</span> on {formatDay(action.start, zone)}? It leaves your calendar on every device.</p>
			{#if action.series}
				<div class="flex flex-col gap-2">
					<Button variant="destructive" size="touch" onclick={() => ondelete?.(action.id, 'this')}>This session only</Button>
					<Button variant="outline" size="touch" class="text-status-missed" onclick={() => ondelete?.(action.id, 'future')}>This and all future sessions</Button>
					<Button variant="ghost" size="touch" onclick={() => (asking = false)}>Keep</Button>
				</div>
			{:else}
				<div class="flex justify-end gap-2">
					<Button variant="outline" size="touch" onclick={() => (asking = false)}>Keep</Button>
					<Button variant="destructive" size="touch" onclick={() => ondelete?.(action.id, 'this')}>Delete</Button>
				</div>
			{/if}
		</div>
	{:else}
		<div class="flex items-center gap-2">
			<Button variant="ghost" size="touch" class="-ml-2 text-status-missed" onclick={() => (asking = true)}>Delete</Button>
			<Button variant="outline" size="touch" class="ml-auto" onclick={() => onclose?.()}>Cancel</Button>
			<Button size="touch" disabled={!changed} onclick={() => onsave?.({ id: action.id, start: startIso, end: endIso })}>Save</Button>
		</div>
	{/if}
	{/if}
</div>
