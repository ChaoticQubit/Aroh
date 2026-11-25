<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import LogScreen from '$lib/ui/screens/LogScreen.svelte';
	import DiarySection from '$lib/ui/diary/DiarySection.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatYmd } from '$lib/ui/format';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { speech } from '$lib/app/speech.svelte';
	import { actionViews, checkInData, logView } from '$lib/app/views';
	import { diaryAppend, diaryLoad, diaryStructure, needsStructure, type DiaryLoad } from '$lib/app/diary';

	const PAST_DAYS = 7;
	const clock = getClock();
	const s = $derived(data.snapshot);
	const view = $derived(s ? logView(s, clock) : null);
	const today = $derived(formatYmd(clock.now, clock.zone));
	let diary = $state<DiaryLoad | null>(null);
	let busy = $state(false);

	onMount(async () => {
		const actions = s ? actionViews(s) : [];
		diary = await diaryLoad(today, PAST_DAYS, actions, clock.zone);
		if (!needsStructure(diary)) return;
		busy = true;
		diary = await diaryStructure(today, PAST_DAYS, actions, clock.zone);
		busy = false;
	});

	/** Seals one diary entry for today, then shows the day as Rust structured it. */
	async function onsend(text: string, spoken: boolean) {
		busy = true;
		diary = await diaryAppend(today, text, spoken, PAST_DAYS, s ? actionViews(s) : [], clock.zone);
		busy = false;
		void data.load();
	}

	/** Writes a check-in answer for its tracker. */
	function oncheckin(trackerId: string, answer: unknown) {
		const due = view?.checkIns.find((c) => c.trackerId === trackerId);
		const entry = due && checkInData(due, answer);
		if (entry) void data.run(() => client.addEntry(trackerId, entry));
	}
</script>

<DiarySection
	load={diary}
	zone={clock.zone}
	{busy}
	voice={speech}
	{onsend}
	onopen={(day) => void goto(`/log/diary/${day}`)}
	onaction={(id) => void goto(`/actions/${id}`)}
	onsettings={() => void goto('/settings/key')}
/>
{#if view}
	<LogScreen {...view} {oncheckin} onlog={(text) => data.run(() => client.logText(text))} voice={speech} />
{/if}
