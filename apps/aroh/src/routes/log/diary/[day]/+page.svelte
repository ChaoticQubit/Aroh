<script lang="ts">
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import DiaryCard from '$lib/ui/diary/DiaryCard.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDate } from '$lib/ui/format';
	import { data } from '$lib/app/data.svelte';
	import { actionViews } from '$lib/app/views';
	import { diaryDay, type DiaryDayView } from '$lib/app/diary';

	const clock = getClock();
	const day = $derived(page.params.day ?? '');
	let view = $state<DiaryDayView | null | undefined>(undefined);
	let note = $state<string | null>(null);

	$effect(() => {
		const d = day;
		void untrack(() => diaryDay(d, data.snapshot ? actionViews(data.snapshot) : [], clock.zone)).then((v) => {
			view = v?.view ?? null;
			note = v?.note ?? null;
		});
	});
</script>

<div class="p-4">
	{#if view}
		<DiaryCard day={view} zone={clock.zone} title={formatDate(view.day, { weekday: 'long', day: 'numeric', month: 'long' })} {note} onaction={(id) => void goto(`/actions/${id}`)} onsettings={() => void goto('/settings/key')} />
	{:else if view === null}
		<p class="text-sm text-muted">No diary for this day on this phone.</p>
	{/if}
</div>
