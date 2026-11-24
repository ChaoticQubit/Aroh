<script lang="ts">
	import Render from '../components/Render.svelte';
	import QuickLog from '../components/QuickLog.svelte';
	import type { Speech } from '$lib/app/speech.svelte';
	import type { ComponentSpec, OnChange, SpecOf } from '$lib/ui/contract';

	/**
	 * Log: check-ins due now, entries waiting for the engine to sort, recent entries, and the
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

	/** The change handler of one check-in card, bound to its tracker. */
	function answer(trackerId: string): OnChange {
		return (value) => oncheckin?.(trackerId, value);
	}
</script>

<div class="flex min-h-full flex-col">
	<div class="flex w-full max-w-5xl flex-1 flex-col gap-4 p-4 @3xl:p-6">
		<div class="grid gap-4 @3xl:grid-cols-[minmax(0,1fr)_minmax(0,360px)]">
			<div class="flex flex-col gap-4">
				{#if waiting.entries.length}<Render spec={waiting} />{/if}
				<Render spec={recent} />
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
