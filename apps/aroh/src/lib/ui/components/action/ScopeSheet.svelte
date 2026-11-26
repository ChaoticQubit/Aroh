<script lang="ts">
	import CalendarDays from '@lucide/svelte/icons/calendar-days';
	import Repeat from '@lucide/svelte/icons/repeat';
	import { Button } from '$lib/ui/primitives/button';

	/**
	 * Bottom sheet that asks the scope of an owner edit (Q71): this session only, or this and all future
	 * sessions of the series. `session` and `future` are the two option details; `onpick` gets the choice,
	 * `oncancel` closes it. Drawn inside its container so it also works inside mock frames.
	 */
	let {
		request,
		session,
		future,
		onpick,
		oncancel
	}: { request: string; session: string; future: string; onpick?: (scope: 'this' | 'future') => void; oncancel?: () => void } = $props();

	const option = 'flex min-h-16 w-full items-start gap-3 rounded-md border border-border bg-bg px-3 py-3 text-left outline-none hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50';
</script>

<div class="absolute inset-0 z-20 flex flex-col justify-end bg-black/40 pt-[env(safe-area-inset-top)]">
	<div role="dialog" aria-modal="true" aria-labelledby="scope-title" class="flex flex-col gap-3 rounded-t-[var(--radius-dialog)] border-t border-border bg-surface pt-3 pr-[max(1rem,env(safe-area-inset-right))] pb-[max(1rem,env(safe-area-inset-bottom))] pl-[max(1rem,env(safe-area-inset-left))]">
		<span class="mx-auto h-1 w-9 rounded-full bg-border-strong" aria-hidden="true"></span>
		<div class="flex flex-col gap-1">
			<h2 id="scope-title" class="text-md font-semibold">Change which sessions?</h2>
			<p class="text-sm text-muted">"{request}"</p>
		</div>
		<button type="button" class={option} onclick={() => onpick?.('this')}>
			<CalendarDays class="mt-0.5 size-5 shrink-0 text-muted" aria-hidden="true" />
			<span class="flex flex-col gap-0.5"><span class="text-md font-medium">This session only</span><span class="text-sm text-muted">{session}</span></span>
		</button>
		<button type="button" class={option} onclick={() => onpick?.('future')}>
			<Repeat class="mt-0.5 size-5 shrink-0 text-muted" aria-hidden="true" />
			<span class="flex flex-col gap-0.5"><span class="text-md font-medium">This and all future sessions</span><span class="text-sm text-muted">{future}</span></span>
		</button>
		<Button variant="ghost" size="touch" onclick={() => oncancel?.()}>Cancel</Button>
	</div>
</div>
