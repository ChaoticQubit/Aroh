<script lang="ts">
	import Check from '@lucide/svelte/icons/check';
	import Lock from '@lucide/svelte/icons/lock';
	import { Button } from '$lib/ui/primitives/button';
	import Render from '../components/Render.svelte';
	import QuickLog from '../components/QuickLog.svelte';
	import StatusChip from '../components/StatusChip.svelte';
	import type { Speech } from '$lib/app/speech.svelte';
	import AllDayChip from '../components/planning/AllDayChip.svelte';
	import AllDayChoice from '../components/planning/AllDayChoice.svelte';
	import ActionSheet from './ActionSheet.svelte';
	import { allDayOn, applyEdits } from './edits';
	import { getClock } from '$lib/ui/clock';
	import { formatDate, formatDuration, formatTime, formatYmd } from '$lib/ui/format';
	import { actionStyle } from '$lib/ui/status';
	import type { ActionView, ComponentSpec, OnChange } from '$lib/ui/contract';
	import type { ActionEdit, AllDayView, DayKind, DayPick } from './types';

	/**
	 * Today: date title with done count and next action, today's all-day events (a chip tap reopens
	 * its kind), the kind question for the next unanswered all-day event, a Next up card (Mark done,
	 * Move), today's actions with tinted edge and checkbox, check-in, one insight and the quick-log bar.
	 * Tapping an action opens the edit sheet (`inline` draws it in place for mockups). `onaction` gets
	 * `{ id, status }`, `oncheckin` the answer, `onlog` the quick-log text (`voice` adds its mic),
	 * `onedit`, `onlock`, `ondelete` and `onchoose` the calendar changes.
	 */
	let {
		actions,
		checkIn = null,
		insight = null,
		allDay = [],
		editing: openId = null,
		confirming = false,
		inline = false,
		onaction,
		oncheckin,
		onlog,
		voice,
		onedit,
		onlock,
		ondelete,
		onchoose
	}: {
		actions: ActionView[];
		checkIn?: ComponentSpec | null;
		insight?: ComponentSpec | null;
		allDay?: AllDayView[];
		editing?: string | null;
		confirming?: boolean;
		inline?: boolean;
		onaction?: OnChange;
		oncheckin?: OnChange;
		onlog?: (text: string) => void;
		voice?: Speech;
		onedit?: (edit: ActionEdit) => void;
		onlock?: (id: string, locked: boolean) => void;
		ondelete?: (id: string) => void;
		onchoose?: (pick: DayPick) => void;
	} = $props();

	const { now, zone } = getClock();
	const todayYmd = formatYmd(now, zone);
	let edits = $state<Record<string, ActionEdit>>({});
	let deleted = $state<string[]>([]);
	let picks = $state<Record<string, DayKind>>({});
	let ticks = $state<Record<string, ActionView['status']>>({});
	let choosing = $state<string | null>(null);
	let editing = $derived(openId);
	$effect.pre(() => {
		void actions;
		ticks = {};
	});
	const events = $derived(allDayOn(allDay, todayYmd, picks));
	const busyDay = $derived(events.find((e) => e.kind === 'busy'));
	const away = $derived(events.find((e) => e.kind === 'away'));
	const today = $derived(
		applyEdits(actions, edits, deleted)
			.filter((a) => formatYmd(a.start, zone) === todayYmd)
			.map((a) => ({ ...a, status: ticks[a.id] ?? a.status }))
	);
	const reopened = $derived(choosing ? allDay.map((e) => (picks[e.id] ? { ...e, kind: picks[e.id] } : e)).find((e) => e.id === choosing) : undefined);
	const asking = $derived(reopened ?? allDay.find((e) => e.kind === null && !picks[e.id] && e.end >= todayYmd));
	const open = $derived(editing ? today.find((a) => a.id === editing) : undefined);
	const next = $derived(today.find((a) => a.status === 'planned' && Date.parse(a.start) > Date.parse(now)));
	const done = $derived(today.filter((a) => a.status === 'done').length);
	const heading = formatDate(todayYmd, { weekday: 'long', day: 'numeric', month: 'long' });
	const minutesTo = $derived(next ? Math.round((Date.parse(next.start) - Date.parse(now)) / 60_000) : 0);
	const minutesLong = (a: ActionView) => Math.max(0, Math.round((Date.parse(a.end) - Date.parse(a.start)) / 60_000));

	/** Flips one action between planned and done, keeps it until new actions arrive, and reports it. */
	function toggle(a: ActionView) {
		const status = a.status === 'done' ? 'planned' : 'done';
		ticks[a.id] = status;
		onaction?.({ id: a.id, status });
	}

	/** Applies a move or resize locally, closes the sheet and reports it. */
	function edit(e: ActionEdit) {
		edits[e.id] = e;
		editing = null;
		onedit?.(e);
	}

	/** Removes an action locally, closes the sheet and reports it. */
	function remove(id: string) {
		deleted = [...deleted, id];
		editing = null;
		ondelete?.(id);
	}

	/** Stores the owner's kind for an all-day event, closes a reopened card and reports it. */
	function choose(pick: DayPick) {
		picks[pick.id] = pick.kind;
		choosing = null;
		onchoose?.(pick);
	}
</script>

<div class="flex min-h-full flex-col">
	<div class="flex w-full max-w-5xl flex-1 flex-col gap-4 p-4 @3xl:p-6">
		<div class="flex flex-col gap-2">
			<header class="flex flex-col gap-1 pb-1">
				<h2 class="text-2xl leading-tight font-semibold">{heading}</h2>
				<p class="text-sm text-muted">
					{#if busyDay}{busyDay.title}: busy day. {/if}{#if away}{away.title}: away. {/if}{done} of {today.length} done{#if next}. Next: {next.title} at {formatTime(next.start, zone)}, in {formatDuration(minutesTo)}{/if}.
				</p>
			</header>
			{#if events.length}
				<ul class="flex flex-col gap-1" aria-label="All day">
					{#each events as e (e.id)}<li><AllDayChip event={e} onpick={onchoose ? (id) => (choosing = id) : undefined} /></li>{/each}
				</ul>
			{/if}
		</div>
		{#if asking && onchoose}<AllDayChoice event={asking} onchoose={choose} onclose={reopened ? () => (choosing = null) : undefined} />{/if}
		<div class="grid grid-cols-1 gap-4 @3xl:grid-cols-[minmax(0,1fr)_minmax(0,360px)]">
			<div class="flex flex-col gap-4">
				{#if next}
					<section class="flex flex-col gap-3 rounded-sm border border-border bg-bg p-4" aria-label="Next up">
						<div class="flex items-center justify-between gap-2">
							<p class="text-xs font-medium text-muted tabular-nums">Next up · {formatTime(next.start, zone)}</p>
							<StatusChip tone="planned" label="In {formatDuration(minutesTo)}" />
						</div>
						<div class="flex flex-col gap-0.5 border-l-2 pl-3" style="border-color: var(--cat-{next.category})">
							<p class="text-lg font-semibold wrap-anywhere">{next.title}</p>
							<p class="text-sm text-muted wrap-anywhere">{next.goalTitle} · {formatDuration(minutesLong(next))}</p>
						</div>
						<div class="flex gap-2">
							<Button size="touch" class="flex-1" onclick={() => next && toggle(next)}>Mark done</Button>
							{#if onedit}<Button variant="outline" size="touch" class="flex-1" onclick={() => (editing = next?.id ?? null)}>Move</Button>{/if}
						</div>
					</section>
				{/if}
				<section class="flex flex-col gap-2" aria-labelledby="today-list">
					<div class="flex items-baseline justify-between px-1">
						<h3 id="today-list" class="text-sm font-semibold">Today's actions</h3>
						<p class="text-sm text-muted tabular-nums">{done} of {today.length} done</p>
					</div>
					{#if today.length}
						<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
							{#each today as a (a.id)}
								{@const look = actionStyle(a.status)}
								<li class="flex min-h-15 items-center gap-3 py-1 pr-1 pl-4 {a.id === next?.id ? 'bg-surface' : ''}">
									<svelte:element
										this={onedit ? 'button' : 'div'}
										type={onedit ? 'button' : undefined}
										role={onedit ? 'button' : undefined}
										aria-label={onedit ? `Edit ${a.title}` : undefined}
										class="flex min-w-0 flex-1 items-center gap-3 self-stretch py-1.5 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
										onclick={onedit ? () => (editing = a.id) : undefined}
									>
										<span class="w-11 shrink-0 text-sm tabular-nums {a.status === 'planned' ? '' : 'text-muted'}">{formatTime(a.start, zone)}<span class="block text-xs text-muted">{formatTime(a.end, zone)}</span></span>
										<span class="flex min-w-0 flex-1 flex-col border-l-2 pl-3" style="border-color: var(--cat-{a.category})">
											<span class="line-clamp-2 text-base font-medium wrap-anywhere {look.title}">{a.title}</span>
											<span class="line-clamp-1 text-xs text-muted wrap-anywhere">{a.goalTitle}</span>
										</span>
									</svelte:element>
									{#if a.locked}<Lock class="size-4 shrink-0 text-muted" aria-label="Locked" />{/if}
									{#if a.status === 'missed' || a.status === 'skipped'}
										<span class="mr-3"><StatusChip tone={a.status} label={look.label ?? ''} /></span>
									{:else}
										<button
											type="button"
											aria-pressed={a.status === 'done'}
											aria-label="{a.status === 'done' ? 'Mark not done' : 'Mark done'}: {a.title}"
											class="flex size-11 shrink-0 items-center justify-center rounded-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
											onclick={() => toggle(a)}
										>
											<span class="flex size-6 items-center justify-center rounded-sm border {a.status === 'done' ? 'border-fg bg-fg text-bg' : 'border-border-strong text-transparent hover:border-fg'}">
												<Check class="size-4" strokeWidth={3} />
											</span>
										</button>
									{/if}
								</li>
							{/each}
						</ul>
					{:else}
						<p class="rounded-sm border border-border bg-bg px-4 py-3 text-sm text-muted">Nothing planned today.</p>
					{/if}
				</section>
			</div>
			<div class="flex flex-col gap-4">
				{#if checkIn}<Render spec={checkIn} onchange={oncheckin} />{/if}
				{#if insight}<Render spec={insight} />{/if}
			</div>
		</div>
	</div>
	<QuickLog {onlog} {voice} />
</div>
{#if open}
	<ActionSheet action={open} {confirming} {inline} onsave={edit} {onlock} ondelete={remove} onclose={() => (editing = null)} />
{/if}
