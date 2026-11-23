<script lang="ts">
	import Bell from '@lucide/svelte/icons/bell';
	import MapPin from '@lucide/svelte/icons/map-pin';
	import Smartphone from '@lucide/svelte/icons/smartphone';
	import { Button } from '$lib/ui/primitives/button';

	/**
	 * Full-screen explainer shown before iOS asks for location. Says what Aroh does with it,
	 * what stays on the phone, and that iOS asks twice on the way to Always.
	 * `oncontinue` starts the iOS request; `onlater` closes it with location off.
	 */
	let { oncontinue, onlater }: { oncontinue?: () => void; onlater?: () => void } = $props();

	const POINTS = [
		{ icon: MapPin, title: 'Learns places you go often', body: 'After a few visits Aroh asks what a place is, like your gym. Nothing is named without you.' },
		{ icon: Bell, title: 'Nudges only when it helps', body: 'Near a linked place, when an action is due today or was missed. One nudge per place a day, never from 22:00 to 06:00.' },
		{ icon: Smartphone, title: 'Visits stay on this phone', body: 'Only the places you name and the routines you approve sync. Aroh keeps no GPS trail.' }
	];
</script>

<div class="flex h-full flex-col gap-6 bg-bg px-6 pt-16 pb-8 text-fg">
	<div class="flex size-12 items-center justify-center rounded-sm border border-border">
		<MapPin class="size-5" aria-hidden="true" />
	</div>
	<div class="flex flex-col gap-2">
		<h1 class="text-xl font-semibold">Let Aroh notice your places</h1>
		<p class="text-sm text-muted">Aroh can remind you near the places where your plans happen, and spot routines you already keep.</p>
	</div>
	<ul class="flex flex-col divide-y divide-border border-y border-border">
		{#each POINTS as p (p.title)}
			<li class="flex gap-3 py-3">
				<p.icon class="mt-0.5 size-4 shrink-0 text-muted" aria-hidden="true" />
				<span class="flex flex-col gap-0.5"><span class="text-sm font-medium">{p.title}</span><span class="text-sm text-muted">{p.body}</span></span>
			</li>
		{/each}
	</ul>
	<p class="text-xs text-muted">iOS asks twice. Choose Allow While Using App first. Later, when iOS asks again, choose Change to Always Allow so Aroh can notice arrivals while it is closed.</p>
	<div class="mt-auto flex flex-col gap-2">
		<Button size="touch" onclick={() => oncontinue?.()}>Continue</Button>
		<Button size="touch" variant="ghost" onclick={() => onlater?.()}>Not now</Button>
		<p class="text-center text-xs text-muted">Pause tracking or clear history any time in Settings.</p>
	</div>
</div>
