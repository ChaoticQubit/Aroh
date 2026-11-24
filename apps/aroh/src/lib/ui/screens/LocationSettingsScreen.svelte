<script lang="ts">
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import { Switch } from '$lib/ui/primitives/switch';
	import StatusChip from '../components/StatusChip.svelte';
	import type { LocationSettingsView } from '../components/location/types';

	/**
	 * Settings > Location: iOS access, pause tracking, nudges, quiet hours, links to Places and the
	 * location log, ignored places and clearing the raw visit history on this phone.
	 * `onsetting` reports a switch; `onunignore`, `onclear`, `onplaces`, `onlog` and `onopensettings`
	 * (Open iOS Settings) report taps.
	 */
	let {
		settings,
		onsetting,
		onunignore,
		onclear,
		onplaces,
		onlog,
		onopensettings
	}: {
		settings: LocationSettingsView;
		onsetting?: (key: 'paused' | 'nudges', on: boolean) => void;
		onunignore?: (placeId: string) => void;
		onclear?: () => void;
		onplaces?: () => void;
		onlog?: () => void;
		onopensettings?: () => void;
	} = $props();

	const id = $props.id();
	let paused = $derived(settings.paused);
	let nudges = $derived(settings.nudges);
	let confirming = $state(false);
	let cleared = $state(false);
	const ACCESS = {
		always: { tone: 'done', label: 'Always' },
		while_using: { tone: 'warn', label: 'While using' },
		off: { tone: 'skipped', label: 'Off' }
	} as const;
	const access = $derived(ACCESS[settings.permission]);
	const row = 'flex min-h-14 w-full items-center gap-3 px-4 py-2 text-left outline-none hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset';

	/** Clears on the second tap, after the confirm line is shown. */
	function clear() {
		if (!confirming) return void (confirming = true);
		confirming = false;
		cleared = true;
		onclear?.();
	}
</script>

<div class="flex w-full max-w-2xl flex-col gap-6 p-4 @3xl:p-6">
	<section class="flex flex-col gap-3" aria-labelledby="{id}-access">
		<h2 id="{id}-access" class="text-sm font-semibold text-fg">Access</h2>
		<Card.Root size="sm" class="gap-3 px-4">
			<div class="flex items-center gap-3">
				<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Location</span><span class="text-xs text-muted">Always lets Aroh notice arrivals while it is closed.</span></span>
				<StatusChip tone={access.tone} label={access.label} />
			</div>
			<div><Button variant="outline" size="touch" onclick={() => onopensettings?.()}>Open iOS Settings</Button></div>
		</Card.Root>
	</section>

	<section class="flex flex-col gap-3" aria-labelledby="{id}-tracking">
		<h2 id="{id}-tracking" class="text-sm font-semibold text-fg">Tracking</h2>
		<Card.Root size="sm" class="gap-0 divide-y divide-border px-4 py-0">
			<label class="flex min-h-14 cursor-pointer items-center gap-3 py-2" for="{id}-pause">
				<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Pause tracking</span><span class="text-xs text-muted">Stops noticing visits and nudges. Places and routines stay.</span></span>
				<Switch id="{id}-pause" bind:checked={paused} onCheckedChange={(v) => onsetting?.('paused', v)} />
			</label>
			<label class="flex min-h-14 cursor-pointer items-center gap-3 py-2" for="{id}-nudge">
				<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Nudges near places</span><span class="text-xs text-muted">When a linked action is due today or missed. One per place a day.</span></span>
				<Switch id="{id}-nudge" bind:checked={nudges} disabled={paused} onCheckedChange={(v) => onsetting?.('nudges', v)} />
			</label>
			<div class="flex min-h-14 items-center gap-3 py-2">
				<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Quiet hours</span><span class="text-xs text-muted">No nudges at night.</span></span>
				<span class="text-sm tabular-nums">22:00 to 06:00</span>
			</div>
		</Card.Root>
	</section>

	<section class="flex flex-col gap-3" aria-labelledby="{id}-places">
		<h2 id="{id}-places" class="text-sm font-semibold text-fg">Places</h2>
		<Card.Root size="sm" class="gap-0 divide-y divide-border px-0 py-0">
			<button type="button" class={row} onclick={() => onplaces?.()}>
				<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Places</span><span class="text-xs text-muted">{settings.watched} of 20 watched. Add, rename or link.</span></span>
				<ChevronRight class="size-4 shrink-0 text-muted" aria-hidden="true" />
			</button>
			<button type="button" class={row} onclick={() => onlog?.()}>
				<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Location log</span><span class="text-xs text-muted">Recent visits, on this phone only.</span></span>
				<ChevronRight class="size-4 shrink-0 text-muted" aria-hidden="true" />
			</button>
		</Card.Root>
	</section>

	<section class="flex flex-col gap-3" aria-labelledby="{id}-ignored">
		<h2 id="{id}-ignored" class="text-sm font-semibold text-fg">Ignored places <span class="text-xs font-normal text-muted tabular-nums">{settings.ignored.length}</span></h2>
		{#if settings.ignored.length}
			<Card.Root size="sm" class="gap-0 divide-y divide-border px-4 py-0">
				{#each settings.ignored as p (p.id)}
					<div class="flex min-h-14 items-center gap-3 py-2">
						<span class="flex flex-1 flex-col"><span class="text-sm font-medium">{p.name}</span><span class="text-xs text-muted">{p.address}</span></span>
						<Button variant="outline" size="touch" onclick={() => onunignore?.(p.id)}>Stop ignoring</Button>
					</div>
				{/each}
			</Card.Root>
		{:else}
			<p class="text-sm text-muted">No ignored places.</p>
		{/if}
	</section>

	<section class="flex flex-col gap-3" aria-labelledby="{id}-history">
		<h2 id="{id}-history" class="text-sm font-semibold text-fg">History</h2>
		<Card.Root size="sm" class="gap-3 px-4">
			<p class="text-sm text-muted">{cleared ? 'Visit history cleared on this phone.' : `${settings.visits} raw visits are kept on this phone. They never sync.`}</p>
			{#if confirming}<p class="text-sm text-status-missed" role="alert">Delete all raw visits? Named places and routines stay.</p>{/if}
			<div class="flex flex-wrap gap-2">
				<Button variant="destructive" size="touch" disabled={cleared} onclick={clear}>{confirming ? 'Delete visits' : 'Clear visit history'}</Button>
				{#if confirming}<Button variant="outline" size="touch" onclick={() => (confirming = false)}>Cancel</Button>{/if}
			</div>
		</Card.Root>
	</section>
</div>
