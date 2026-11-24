<script lang="ts">
	import Render from '../components/Render.svelte';
	import QuickLog from '../components/QuickLog.svelte';
	import type { Speech } from '$lib/app/speech.svelte';
	import { getClock } from '$lib/ui/clock';
	import { dayLabels, formatNumber, formatTime, formatYmd } from '$lib/ui/format';
	import type { ComponentSpec, OnChange, SpecOf } from '$lib/ui/contract';

	/**
	 * Log: title with today's entry count, check-ins due now, entries waiting for the engine to sort,
	 * recent entries in one card (time, text, value; a day label once they span days), and the
	 * quick-log bar. `oncheckin` gets the tracker id and the answer, `onlog` the quick-log text, `voice` adds its mic.
	 */
	let {
		checkIns,
		waiting,
		recent,
		oncheckin,
		onlog,
		voice
	}: {
		checkIns: { trackerId: string; spec: ComponentSpec }[];
		waiting: SpecOf<'log_list'>;
		recent: SpecOf<'log_list'>;
		oncheckin?: (trackerId: string, answer: unknown) => void;
		onlog?: (text: string) => void;
		voice?: Speech;
	} = $props();

	const { now, zone } = getClock();
	const today = formatYmd(now, zone);
	const todayCount = $derived(recent.entries.filter((e) => formatYmd(e.at, zone) === today).length);

	/** The change handler of one check-in card, bound to its tracker. */
	function answer(trackerId: string): OnChange {
		return (value) => oncheckin?.(trackerId, value);
	}
</script>

{#snippet entries(spec: SpecOf<'log_list'>, empty: string)}
	<section class="flex flex-col gap-2" aria-label={spec.label}>
		<div class="flex items-baseline justify-between px-1">
			<h3 class="text-sm font-semibold">{spec.label}</h3>
			<p class="text-sm text-muted tabular-nums">{spec.entries.length} {spec.entries.length === 1 ? 'entry' : 'entries'}</p>
		</div>
		{#if spec.entries.length}
			{@const days = dayLabels(spec.entries.map((e) => e.at), zone, today)}
			<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
				{#each spec.entries as e, i (e.at + e.text)}
					<li class="flex min-h-13 items-center gap-3 px-4 py-2">
						<time datetime={e.at} class="w-12 shrink-0 text-sm text-muted tabular-nums">{#if days[i]}<span class="block text-xs">{days[i]}</span>{/if}{formatTime(e.at, zone)}</time>
						<span class="min-w-0 flex-1 text-base wrap-anywhere">{e.text}</span>
						{#if e.value !== undefined}<span class="shrink-0 text-sm tabular-nums">{formatNumber(e.value, e.unit)}</span>{/if}
					</li>
				{/each}
			</ul>
		{:else}
			<p class="rounded-sm border border-border bg-bg px-4 py-3 text-sm text-muted">{empty}</p>
		{/if}
	</section>
{/snippet}

<div class="flex min-h-full flex-col">
	<div class="flex w-full max-w-5xl flex-1 flex-col gap-4 p-4 @3xl:p-6">
		<header class="flex flex-col gap-1 pb-1">
			<h2 class="text-2xl leading-tight font-semibold">Log</h2>
			<p class="text-sm text-muted">{todayCount ? `Today: ${todayCount} ${todayCount === 1 ? 'entry' : 'entries'}` : 'Nothing logged today yet'}{waiting.entries.length ? `, ${waiting.entries.length} waiting to be sorted` : ''}.</p>
		</header>
		<div class="grid gap-4 @3xl:grid-cols-[minmax(0,1fr)_minmax(0,360px)]">
			<div class="flex flex-col gap-4">
				{#if waiting.entries.length}{@render entries(waiting, '')}{/if}
				{@render entries(recent, 'Nothing logged yet. Say or type what you did below.')}
			</div>
			{#if checkIns.length}
				<div class="order-first flex flex-col gap-4 @3xl:order-none">
					{#each checkIns as c (c.trackerId)}<Render spec={c.spec} onchange={answer(c.trackerId)} />{/each}
				</div>
			{/if}
		</div>
	</div>
	<QuickLog {onlog} {voice} />
</div>
