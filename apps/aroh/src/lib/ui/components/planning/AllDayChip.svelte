<script lang="ts">
	import Bell from '@lucide/svelte/icons/bell';
	import CalendarOff from '@lucide/svelte/icons/calendar-off';
	import CircleHelp from '@lucide/svelte/icons/circle-question-mark';
	import Plane from '@lucide/svelte/icons/plane';
	import { DAY_KIND_LABEL } from '$lib/ui/screens/edits';
	import type { AllDayView } from '$lib/ui/screens/types';

	/**
	 * One device all-day event as a neutral chip: filled for a Busy day or Away, a plain outline for
	 * a Reminder, dashed while the owner has not chosen yet. `detail` false hides the trailing word
	 * for narrow week columns. With `onpick` the chip is a button that reopens the kind choice.
	 */
	let { event, detail = true, onpick }: { event: AllDayView; detail?: boolean; onpick?: (id: string) => void } = $props();

	const word = $derived(event.kind ? DAY_KIND_LABEL[event.kind] : 'Choose');
	const tone = $derived(
		event.kind === 'busy' || event.kind === 'away'
			? 'border-border-strong bg-surface-2 text-fg'
			: event.kind === 'reminder'
				? 'border-border bg-bg text-muted'
				: 'border-dashed border-border-strong bg-bg text-fg'
	);
</script>

{#snippet body()}
	{#if event.kind === 'busy'}<CalendarOff class="size-3.5 shrink-0" aria-hidden="true" />{:else if event.kind === 'away'}<Plane class="size-3.5 shrink-0" aria-hidden="true" />{:else if event.kind === 'reminder'}<Bell class="size-3.5 shrink-0" aria-hidden="true" />{:else}<CircleHelp class="size-3.5 shrink-0" aria-hidden="true" />{/if}
	<span class="truncate">{event.title}</span>
	{#if detail}<span class="ml-auto shrink-0 font-normal text-muted">{word}</span>{/if}
{/snippet}

{#if onpick}
	<button
		type="button"
		class="flex h-6 w-full min-w-0 items-center gap-1.5 rounded-sm border px-1.5 text-left text-xs font-medium outline-none hover:border-fg focus-visible:ring-3 focus-visible:ring-ring/50 {tone}"
		title="{event.title}, {word}"
		aria-label="{event.title}, {word}. Change"
		onclick={() => onpick(event.id)}
	>
		{@render body()}
	</button>
{:else}
	<span class="flex h-6 min-w-0 items-center gap-1.5 rounded-sm border px-1.5 text-xs font-medium {tone}" title="{event.title}, {word}">
		{@render body()}
	</span>
{/if}
