<script lang="ts">
	import Render from '../components/Render.svelte';
	import QuickLog from '../components/QuickLog.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDate, formatDuration, formatTime, formatYmd } from '$lib/ui/format';
	import type { ActionView, ComponentSpec, OnChange } from '$lib/ui/contract';

	/**
	 * Today: date header, next action, check-in, today's actions, one insight and the quick-log bar.
	 * `onaction` gets `{ id, status }`, `oncheckin` the answer, `onlog` the quick-log text.
	 */
	let {
		actions,
		checkIn = null,
		insight = null,
		onaction,
		oncheckin,
		onlog
	}: { actions: ActionView[]; checkIn?: ComponentSpec | null; insight?: ComponentSpec | null; onaction?: OnChange; oncheckin?: OnChange; onlog?: (text: string) => void } = $props();

	const { now, zone } = getClock();
	const todayYmd = formatYmd(now, zone);
	const today = $derived(actions.filter((a) => formatYmd(a.start, zone) === todayYmd));
	const next = $derived(today.find((a) => a.status === 'planned' && Date.parse(a.start) > Date.parse(now)));
	const left = $derived(today.filter((a) => a.status === 'planned').length);
	const heading = formatDate(todayYmd, { weekday: 'long', day: 'numeric', month: 'long' });
	const minutesTo = $derived(next ? Math.round((Date.parse(next.start) - Date.parse(now)) / 60_000) : 0);
</script>

<div class="flex min-h-full flex-col">
	<div class="flex w-full max-w-5xl flex-1 flex-col gap-4 p-4 @3xl:p-6">
		<div>
			<h2 class="text-xl font-semibold">{heading}</h2>
			<p class="text-sm text-muted">
				{left} {left === 1 ? 'action' : 'actions'} left today{#if next}. Next: <span class="font-medium text-fg">{next.title}</span> at {formatTime(next.start, zone)}, in {formatDuration(minutesTo)}{/if}
			</p>
		</div>
		<div class="grid gap-4 @3xl:grid-cols-[minmax(0,1fr)_minmax(0,360px)]">
			<div class="flex flex-col gap-4">
				<Render spec={{ kind: 'action_list', label: "Today's actions", actions: today }} onchange={onaction} />
			</div>
			<div class="flex flex-col gap-4">
				{#if checkIn}<Render spec={checkIn} onchange={oncheckin} />{/if}
				{#if insight}<Render spec={insight} />{/if}
			</div>
		</div>
	</div>
	<QuickLog {onlog} />
</div>
