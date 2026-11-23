<script lang="ts">
	import PhoneShell from '$lib/ui/shell/PhoneShell.svelte';
	import type { Place } from '$lib/ui/shell/nav';
	import type { Back } from '$lib/ui/shell/back';
	import ActionCard from '$lib/ui/components/action/ActionCard.svelte';
	import CardChat, { type CardChatEntry } from '$lib/ui/components/action/CardChat.svelte';
	import ScopeSheet from '$lib/ui/components/action/ScopeSheet.svelte';
	import { cardDiff } from '$lib/ui/action/card';
	import { emptyLog, laterWeeks, legDay, legDayAction, legDayLog, legDayOutline, legDayShort, legDayWeek3, mealPrep, mealPrepAction, mealPrepLog } from './data';

	/**
	 * The "Action cards" mock view (Q71) on the real components: Leg day full card, mid-workout logging,
	 * the card chat with an applied diff and Undo, the scope choice, a meal prep card and a week 3 outline.
	 * `solo` renders one frame full screen for simulator screenshots.
	 */
	let { inboxCount, solo = null }: { inboxCount: number; solo?: number | null } = $props();

	const REQUEST = 'swap lunges for split squats, 45 min';
	const applied: CardChatEntry[] = [
		{ kind: 'owner', text: REQUEST },
		{ kind: 'coach', text: 'Done for Thursday. Split squats train the same muscles with less knee travel. To fit 45 minutes I dropped calf raises and cut rest on squats and split squats.' },
		{ kind: 'applied', scope: 'This session only', lines: cardDiff({ card: legDay, minutes: 60 }, { card: legDayShort, minutes: 45 }), undone: false }
	];
	const NOTES = 'Left knee felt tight on lunges last week: keep the front knee over the ankle.\nGym is busy after 18:30, start with squats.';

	type Frame = { name: string; title: string; tab: Place; back: Back; body: 'card' | 'log' | 'chat' | 'scope' | 'meal' | 'outline' };
	const TODAY: Back = { label: 'Today', history: true, path: '/today' };
	const LEG: Back = { label: 'Leg day', history: true, path: '/today' };
	const FRAMES: Frame[] = [
		{ name: '1. Leg day, full card', title: 'Action', tab: 'today', back: TODAY, body: 'card' },
		{ name: '2. Logging sets, one edited', title: 'Action', tab: 'today', back: TODAY, body: 'log' },
		{ name: '3. Card chat, applied with Undo', title: 'Card chat', tab: 'today', back: LEG, body: 'chat' },
		{ name: '4. This session or all future', title: 'Card chat', tab: 'today', back: LEG, body: 'scope' },
		{ name: '5. Meal prep, recipe and checklist', title: 'Action', tab: 'today', back: TODAY, body: 'meal' },
		{ name: '6. Week 3, outline', title: 'Action', tab: 'calendar', back: { label: 'Calendar', history: true, path: '/calendar' }, body: 'outline' }
	];
	const list = $derived(solo === null ? FRAMES : FRAMES.slice(solo, solo + 1));
</script>

{#snippet frame(f: Frame)}
	<div class="relative h-full">
		<PhoneShell title={f.title} tab={f.tab} back={f.back} {inboxCount}>
			{#if f.body === 'card'}
				<ActionCard meta={legDayAction} card={legDay} log={emptyLog} notes={NOTES} />
			{:else if f.body === 'log'}
				<ActionCard meta={legDayAction} card={legDay} log={legDayLog} notes={NOTES} mode="log" editing={{ row: 0, set: 3 }} />
			{:else if f.body === 'chat'}
				<CardChat coach={legDayAction.coach} entries={applied} />
			{:else if f.body === 'scope'}
				<CardChat coach={legDayAction.coach} entries={applied.slice(0, 1)} />
			{:else if f.body === 'meal'}
				<ActionCard meta={mealPrepAction} card={mealPrep} log={mealPrepLog} notes="Double the rice if you train that evening." />
			{:else}
				<ActionCard meta={legDayWeek3} card={legDayOutline} log={emptyLog} weeks={laterWeeks} fillOn="2026-12-06" />
			{/if}
		</PhoneShell>
		{#if f.body === 'scope'}
			<ScopeSheet request={REQUEST} session="Thu 26 Nov, 18:00 to 18:45. The next Leg day stays as planned." future="Every Leg day to 14 Jan. Each next card still follows your log." />
		{/if}
	</div>
{/snippet}

{#if solo !== null}
	<div class="h-dvh w-full overflow-hidden bg-bg">{@render frame(list[0])}</div>
{:else}
	<ul class="flex gap-6 overflow-x-auto pb-4">
		{#each list as f (f.name)}
			<li class="flex shrink-0 flex-col items-center gap-2">
				<div class="box-content h-[844px] w-[390px] [transform:translateZ(0)] overflow-hidden rounded-md border border-border bg-bg">{@render frame(f)}</div>
				<p class="text-sm font-medium">{f.name}</p>
			</li>
		{/each}
	</ul>
{/if}
