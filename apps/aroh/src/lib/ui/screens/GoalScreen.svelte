<script lang="ts">
	import MessageSquare from '@lucide/svelte/icons/message-square';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import Persona from '../components/agent/Persona.svelte';
	import Render from '../components/Render.svelte';
	import Segmented from '../components/Segmented.svelte';
	import { getClock } from '$lib/ui/clock';
	import { formatDay, formatNumber } from '$lib/ui/format';
	import { sixpackGoal, sixpackView } from '$lib/ui/mockdata/sixpack';

	/** Goal dashboard built from the goal's view spec, with the persona chat entry and autonomy level. */
	const { zone } = getClock();
	let autonomy = $state(sixpackGoal.autonomy);
	const AUTONOMY = {
		Ask: 'Every change waits for your approval.',
		Balanced: 'Small changes apply on their own with undo. Big ones ask first.',
		Auto: 'All changes apply with undo, plus a daily digest.'
	} as const;
	const weight = sixpackGoal.trackers[0];
</script>

<div class="flex w-full max-w-5xl flex-col gap-4 p-4 @3xl:p-6">
	<div class="flex flex-col gap-1">
		<p class="flex items-center gap-2 text-xs font-medium text-muted">
			<span class="size-2 rounded-full" style="background: var(--cat-{sixpackGoal.slot})"></span>{sixpackGoal.category}
		</p>
		<h2 class="text-xl font-semibold">{sixpackView.title}</h2>
		<p class="text-sm text-muted">Due {formatDay(sixpackGoal.deadline, zone)} · {formatNumber(weight.now, weight.unit)} now, {formatNumber(weight.target, weight.unit)} target</p>
	</div>

	<div class="grid gap-4 @3xl:grid-cols-2">
		<Card.Root size="sm" class="gap-3 px-4">
			<div class="flex items-center justify-between gap-2">
				<p class="text-sm font-medium">Your personas</p>
				<MessageSquare class="size-4 text-muted" aria-hidden="true" />
			</div>
			<div class="flex flex-wrap gap-x-4 gap-y-2">
				{#each sixpackGoal.personas as p (p)}<Persona name={p} />{/each}
			</div>
			<div><Button variant="outline" size="touch" class="@3xl:h-8">Chat with your personas</Button></div>
		</Card.Root>
		<Card.Root size="sm" class="gap-3 px-4">
			<p class="text-sm font-medium">How much can agents change on their own?</p>
			<Segmented label="Autonomy level" bind:value={autonomy} options={[{ value: 'Ask', label: 'Ask' }, { value: 'Balanced', label: 'Balanced' }, { value: 'Auto', label: 'Auto' }]} />
			<p class="text-sm text-muted">{AUTONOMY[autonomy]} Locked and hand-edited actions are never moved without asking.</p>
		</Card.Root>
	</div>

	{#each sixpackView.sections as section (section.title)}
		<section class="flex flex-col gap-3" aria-label={section.title}>
			<h3 class="text-sm font-semibold text-fg">{section.title}</h3>
			<div class="grid gap-4 @3xl:grid-cols-2">
				{#each section.items as item, i (i)}
					<div class={item.kind === 'line_chart' || item.kind === 'action_list' ? '@3xl:col-span-2' : ''}><Render spec={item} /></div>
				{/each}
			</div>
		</section>
	{/each}
</div>
