<script lang="ts">
	import Smartphone from '@lucide/svelte/icons/smartphone';
	import * as Card from '$lib/ui/primitives/card';
	import { getClock } from '$lib/ui/clock';
	import { formatDay, formatDuration, formatTime, formatYmd } from '$lib/ui/format';
	import type { VisitRow } from '../components/location/types';

	/**
	 * Location log: recent visits to named places from the phone's local store, newest first,
	 * grouped by local day. Raw visits never leave the phone, which the screen says up front.
	 */
	let { visits }: { visits: VisitRow[] } = $props();

	const { now, zone } = getClock();
	const today = formatYmd(now, zone);
	const days = $derived(
		[...visits]
			.sort((a, b) => b.arrived.localeCompare(a.arrived))
			.reduce<{ day: string; label: string; rows: VisitRow[] }[]>((groups, v) => {
				const day = formatYmd(v.arrived, zone);
				const last = groups.at(-1);
				if (last?.day === day) last.rows.push(v);
				else groups.push({ day, label: day === today ? 'Today' : formatDay(v.arrived, zone), rows: [v] });
				return groups;
			}, [])
	);

	/** Minutes between arrival and leaving, or null while still there. */
	function stayed(v: VisitRow): number | null {
		return v.left ? Math.round((Date.parse(v.left) - Date.parse(v.arrived)) / 60000) : null;
	}
</script>

<div class="flex w-full max-w-2xl flex-col gap-6 p-4 @3xl:p-6">
	<p class="flex items-center gap-2 rounded-sm border border-border bg-bg px-3 py-2 text-xs text-muted">
		<Smartphone class="size-4 shrink-0" aria-hidden="true" />On this phone only. Visits never sync and are not shared with agents.
	</p>
	{#each days as d (d.day)}
		<section class="flex flex-col gap-2" aria-label={d.label}>
			<h2 class="text-sm font-semibold text-fg">{d.label}</h2>
			<Card.Root size="sm" class="gap-0 divide-y divide-border px-4 py-0">
				{#each d.rows as v (v.id)}
					{@const mins = stayed(v)}
					<div class="flex min-h-12 items-center gap-3 py-2">
						<span class="w-24 shrink-0 text-xs text-muted tabular-nums">{formatTime(v.arrived, zone)}{v.left ? ` to ${formatTime(v.left, zone)}` : ''}</span>
						<span class="flex-1 text-sm font-medium">{v.place}</span>
						<span class="text-xs text-muted tabular-nums">{mins === null ? 'Here now' : formatDuration(mins)}</span>
					</div>
				{/each}
			</Card.Root>
		</section>
	{:else}
		<p class="text-sm text-muted">No visits yet.</p>
	{/each}
	<p class="text-xs text-muted">Only visits to places you named are listed.</p>
</div>
