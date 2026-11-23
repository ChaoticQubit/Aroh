<script lang="ts">
	import CalendarClock from '@lucide/svelte/icons/calendar-clock';
	import Check from '@lucide/svelte/icons/check';
	import Clock from '@lucide/svelte/icons/clock';
	import Target from '@lucide/svelte/icons/target';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import Persona from '../agent/Persona.svelte';
	import StatusChip from '../StatusChip.svelte';
	import Render from '../Render.svelte';
	import { limitsView } from '$lib/ui/plan/limits';
	import type { ChatMessage, OnChange } from '$lib/ui/contract';

	/**
	 * The card shown instead of a plan when the goal does not fit the daily minutes by its date (Q64):
	 * the real finish date, a today, target and finish bar when the server sends the dates, and the
	 * three choices. Picking one answers `limit_choice` with the server's text; Raise opens the minutes
	 * input, the others skip it. Emits the question group's `answer`, `skip` and `submit` events.
	 */
	let { group, onchange }: { group: Extract<ChatMessage, { type: 'questions' }>; onchange?: OnChange } = $props();

	const ICONS = { move: CalendarClock, raise: Clock, lower: Target, other: Check };
	const choice = $derived(group.items.find((i) => i.id === 'limit_choice'));
	const minutes = $derived(group.items.find((i) => i.id === 'new_minutes'));
	const options = $derived(choice?.input.kind === 'choice' ? choice.input.options : []);
	const view = $derived(limitsView(choice?.text ?? group.prompt, options, group.card ?? null));
	const picked = $derived(view.choices.find((c) => c.value === group.values.limit_choice) ?? null);

	/** Answers the choice; Raise un-skips the minutes and prefills the server's number, the others skip them. */
	function pick(c: (typeof view.choices)[number]) {
		onchange?.({ type: 'answer', id: 'limit_choice', value: c.value });
		if (!minutes) return;
		if (c.kind === 'raise') {
			onchange?.({ type: 'skip', id: 'new_minutes', skipped: false });
			if (c.minutes && !group.values.new_minutes) onchange?.({ type: 'answer', id: 'new_minutes', value: c.minutes });
		} else onchange?.({ type: 'skip', id: 'new_minutes', skipped: true });
	}
</script>

<Card.Root size="sm" class="gap-4 px-4">
	<div class="flex items-center justify-between gap-2">
		<Persona name={group.persona} note="Not planned yet" />
		<StatusChip tone="warn" label="Does not fit" />
	</div>
	<p class="text-md font-semibold tabular-nums">{view.headline}</p>

	{#if view.bar}
		<div class="flex flex-col gap-1.5" role="img" aria-label="Today {view.bar.today}, target {view.bar.target}, finish {view.bar.finish}">
			<div class="relative h-3">
				<div class="absolute inset-y-0 left-0 rounded-l-sm bg-status-planned" style="width: calc({view.bar.at}% - 1px)"></div>
				<div class="absolute inset-y-0 right-0 rounded-r-sm bg-status-warn" style="left: calc({view.bar.at}% + 1px)"></div>
				<div class="absolute -inset-y-1 w-0.5 bg-fg" style="left: calc({view.bar.at}% - 1px)"></div>
			</div>
			<div class="relative h-8 text-xs tabular-nums">
				<span class="absolute left-0 flex flex-col"><span class="text-muted">Today</span>{view.bar.today}</span>
				<span class="absolute flex -translate-x-1/2 flex-col items-center font-medium" style="left: clamp(2.5rem, {view.bar.at}%, calc(100% - 2.5rem))"><span class="text-muted">Target</span>{view.bar.target}</span>
				<span class="absolute right-0 flex flex-col items-end"><span class="text-muted">{view.bar.pace}</span>{view.bar.finish}</span>
			</div>
		</div>
	{/if}

	<div class="flex flex-col gap-2">
		<p class="text-sm font-medium">Pick one and Aroh plans it</p>
		<div role="radiogroup" aria-label="What should change" class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
			{#each view.choices as c (c.value)}
				{@const Icon = ICONS[c.kind]}
				<button type="button" role="radio" aria-checked={picked?.value === c.value} class="flex min-h-14 w-full items-center gap-3 px-3 py-2 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset {picked?.value === c.value ? 'bg-surface-2' : ''}" onclick={() => pick(c)}>
					<Icon class="size-4 shrink-0 text-muted" aria-hidden="true" />
					<span class="flex min-w-0 flex-1 flex-col"><span class="text-sm font-medium tabular-nums">{c.title}</span>{#if c.detail}<span class="text-xs text-muted tabular-nums">{c.detail}</span>{/if}</span>
					<span class="flex size-5 shrink-0 items-center justify-center rounded-full border {picked?.value === c.value ? 'border-fg bg-fg text-bg' : 'border-border-strong'}" aria-hidden="true">{#if picked?.value === c.value}<Check class="size-3" />{/if}</span>
				</button>
			{/each}
		</div>
		{#if picked?.kind === 'raise' && minutes}
			<div class="rounded-sm border border-border bg-bg p-3"><Render spec={minutes.input} onchange={(value) => onchange?.({ type: 'answer', id: 'new_minutes', value })} /></div>
		{/if}
		{#if group.error}<p class="text-sm text-status-missed" role="alert">{group.error}</p>{/if}
		<Button size="touch" class="w-full @3xl:h-8" disabled={!picked || group.missing.length > 0 || group.sending || group.submitted} onclick={() => onchange?.({ type: 'submit' })}>{group.submitted ? 'Planning' : 'Plan with this'}</Button>
		<p class="text-xs text-muted">Aroh never plans more than your daily minutes or adds catch-up sessions. Or tell it something else in the chat.</p>
	</div>
</Card.Root>
