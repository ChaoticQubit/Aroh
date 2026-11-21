<script lang="ts">
	import Camera from '@lucide/svelte/icons/camera';
	import Mic from '@lucide/svelte/icons/mic';
	import Send from '@lucide/svelte/icons/send';
	import { Button } from '$lib/ui/primitives/button';
	import Render from '../components/Render.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDate, formatDuration, formatTime, formatYmd } from '$lib/ui/format';
	import { sixpackActions, sixpackInbox, weightCheckIn } from '$lib/ui/mockdata/sixpack';

	/** Today: date header, next action, check-in, today's actions, one insight and the quick-log bar. */
	const { now, zone } = getClock();
	const todayYmd = formatYmd(now, zone);
	const today = sixpackActions.filter((a) => formatYmd(a.start, zone) === todayYmd);
	const next = today.find((a) => a.status === 'planned' && Date.parse(a.start) > Date.parse(now));
	const left = today.filter((a) => a.status === 'planned').length;
	const heading = formatDate(todayYmd, { weekday: 'long', day: 'numeric', month: 'long' });
	const minutesTo = next ? Math.round((Date.parse(next.start) - Date.parse(now)) / 60_000) : 0;
	const id = $props.id();
	let draft = $state("");
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
				<Render spec={{ kind: 'action_list', label: "Today's actions", actions: today }} />
			</div>
			<div class="flex flex-col gap-4">
				<Render spec={weightCheckIn} />
				<Render spec={sixpackInbox.insights[0]} />
			</div>
		</div>
	</div>
	<form class="sticky bottom-0 border-t border-border bg-bg p-2" onsubmit={(e) => { e.preventDefault(); draft = ''; }}>
		<div class="flex max-w-5xl items-center gap-1">
			<label for={id} class="sr-only">Quick log</label>
			<input {id} bind:value={draft} placeholder="Log a meal, weight or note" class="h-11 min-w-0 flex-1 rounded-sm border border-input bg-transparent px-3 text-lg outline-none placeholder:text-muted focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50" />
			<Button variant="ghost" size="icon-touch" type="button" aria-label="Speak a log entry"><Mic /></Button>
			<Button variant="ghost" size="icon-touch" type="button" aria-label="Log with a photo"><Camera /></Button>
			<Button size="icon-touch" type="submit" aria-label="Log" disabled={!draft.trim()}><Send /></Button>
		</div>
	</form>
</div>
