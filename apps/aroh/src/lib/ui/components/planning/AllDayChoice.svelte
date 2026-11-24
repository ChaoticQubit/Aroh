<script lang="ts">
	import Bell from '@lucide/svelte/icons/bell';
	import CalendarOff from '@lucide/svelte/icons/calendar-off';
	import Plane from '@lucide/svelte/icons/plane';
	import { Button } from '$lib/ui/primitives/button';
	import Panel from '../Panel.svelte';
	import { formatDate } from '$lib/ui/format';
	import { DAY_KIND_LABEL } from '$lib/ui/screens/edits';
	import type { AllDayView, DayKind, DayPick } from '$lib/ui/screens/types';

	/**
	 * The kind question for one device all-day event: Reminder (plan as usual), Busy day (its
	 * actions move to the next free day) or Away (the plan adapts, asked in the Inbox). The current
	 * kind is marked and the triage proposal shows "Suggested". Emits `{ id, kind }`; `onclose` hides a reopened card.
	 */
	let { event, onchoose, onclose }: { event: AllDayView; onchoose?: (pick: DayPick) => void; onclose?: () => void } = $props();

	const days = $derived(
		event.start === event.end
			? formatDate(event.start, { weekday: 'short', day: 'numeric', month: 'short' })
			: `${formatDate(event.start, { weekday: 'short', day: 'numeric', month: 'short' })} - ${formatDate(event.end, { weekday: 'short', day: 'numeric', month: 'short' })}`
	);
	const OPTIONS: { kind: DayKind; detail: string }[] = [
		{ kind: 'reminder', detail: 'Plan as usual' },
		{ kind: 'busy', detail: 'Actions move to a free day' },
		{ kind: 'away', detail: 'The plan adapts, you approve' }
	];
</script>

<Panel title={event.kind ? 'All-day event' : 'New all-day event'}>
	{#snippet aside()}
		{#if onclose}<Button variant="ghost" size="sm" onclick={() => onclose?.()}>Close</Button>{:else}<span>Asked once</span>{/if}
	{/snippet}
	<div class="flex flex-col gap-0.5">
		<p class="text-sm font-medium text-fg">{event.title}</p>
		<p class="text-xs text-muted">{days}. What does it do to the day?</p>
	</div>
	<div class="grid grid-cols-3 gap-2" role="group" aria-label="Kind of {event.title}">
		{#each OPTIONS as o (o.kind)}
			<button
				type="button"
				aria-pressed={event.kind === o.kind}
				class="flex min-h-16 flex-col items-start gap-1 rounded-sm border bg-bg p-2.5 text-left outline-none hover:border-border-strong hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50 {event.kind === o.kind ? 'border-fg' : 'border-border'}"
				onclick={() => onchoose?.({ id: event.id, kind: o.kind })}
			>
				<span class="flex items-center gap-1.5">
					{#if o.kind === 'busy'}<CalendarOff class="size-4 shrink-0 text-fg" aria-hidden="true" />{:else if o.kind === 'away'}<Plane class="size-4 shrink-0 text-fg" aria-hidden="true" />{:else}<Bell class="size-4 shrink-0 text-muted" aria-hidden="true" />{/if}
					<span class="text-sm font-medium text-fg">{DAY_KIND_LABEL[o.kind]}</span>
				</span>
				<span class="text-xs text-muted">{o.detail}</span>
				{#if event.suggested === o.kind && event.kind !== o.kind}<span class="text-xs font-medium text-accent">Suggested</span>{/if}
			</button>
		{/each}
	</div>
</Panel>
