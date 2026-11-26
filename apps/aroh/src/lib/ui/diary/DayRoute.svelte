<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import DayScreen from './DayScreen.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDate, formatYmd } from '$lib/ui/format';
	import { data } from '$lib/app/data.svelte';
	import { speech } from '$lib/app/speech.svelte';
	import { actionViews } from '$lib/app/views';
	import { diaryAppend, diaryDay, diaryDayOf, diaryLoad, diaryStructure, needsStructure, type DiaryLoad } from '$lib/app/diary';

	/**
	 * Loads and runs the day screen of local `day` (B126): the Log tab, a past day and
	 * /notes/day/<day> all mount this, keyed by the date. The diary's current day (B127) gets the
	 * composer, structuring on open and the Past days; any other day reads its card only.
	 */
	let { day }: { day: string } = $props();

	const PAST_DAYS = 7;
	const clock = getClock();
	const today = diaryDayOf(clock.now, clock.zone);
	const current = $derived(day === today);
	const title = today === formatYmd(clock.now, clock.zone) ? 'Today' : formatDate(today, { weekday: 'long', day: 'numeric', month: 'long' });
	let load = $state<DiaryLoad | null>(null);
	let busy = $state(false);

	/** The done sessions the card lists, from the current snapshot. */
	function actions() {
		return data.snapshot ? actionViews(data.snapshot) : [];
	}

	onMount(async () => {
		if (!current) {
			const v = await diaryDay(day, actions(), clock.zone);
			load = { kind: 'ready', today: v?.view ?? { day, entries: [], structure: null, done: [] }, past: [], note: v?.note ?? null };
			return;
		}
		load = await diaryLoad(today, PAST_DAYS, actions(), clock.zone);
		if (!needsStructure(load)) return;
		busy = true;
		load = await diaryStructure(today, PAST_DAYS, actions(), clock.zone);
		busy = false;
	});

	/** Seals one diary entry for today, then shows the day as Rust structured it. */
	async function onsend(text: string, spoken: boolean) {
		busy = true;
		load = await diaryAppend(today, text, spoken, PAST_DAYS, actions(), clock.zone);
		busy = false;
		void data.load();
	}
</script>

<DayScreen
	{day}
	{load}
	{current}
	{title}
	zone={clock.zone}
	{busy}
	voice={speech}
	{onsend}
	onopen={(d) => void goto(`/log/diary/${d}`)}
	onaction={(id) => void goto(`/actions/${id}`)}
	onsettings={() => void goto('/settings/key')}
	onchange={(r) => {
		if (load?.kind === 'ready') load = { ...load, today: r.view, note: r.note };
	}}
/>
