<script lang="ts">
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import DiaryCard from './DiaryCard.svelte';
	import { diaryDay, type DiaryDayView } from '$lib/app/diary';
	import type { ActionView } from '$lib/ui/contract';

	/**
	 * The diary row above the calendar's day view (Q85): the compact card of local `day` when it
	 * has entries on this phone, opening the full card. Shows nothing off iPhone or on a day
	 * without entries.
	 */
	let { day, actions, zone }: { day: string; actions: ActionView[]; zone: string } = $props();
	let view = $state<DiaryDayView | null>(null);

	$effect(() => {
		const d = day;
		view = null;
		void untrack(() => diaryDay(d, actions, zone)).then((v) => {
			if (d === day) view = v?.view ?? null;
		});
	});
</script>

{#if view}
	<div class="pb-2"><DiaryCard day={view} mode="compact" {zone} onopen={() => void goto(`/log/diary/${day}`)} /></div>
{/if}
