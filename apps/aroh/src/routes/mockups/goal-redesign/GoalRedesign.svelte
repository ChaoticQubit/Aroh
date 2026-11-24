<script lang="ts">
	import PhoneShell from '$lib/ui/shell/PhoneShell.svelte';
	import SettingsScreen from '$lib/ui/screens/SettingsScreen.svelte';
	import { sixpackSettings } from '$lib/ui/mockdata/sixpack';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import AgentControls from '$lib/ui/components/goal/AgentControls.svelte';
	import { blankDraft } from '$lib/app/newgoal';
	import ProfileScreen from '$lib/ui/screens/ProfileScreen.svelte';
	import GoalCard from './GoalCard.svelte';
	import FormFrame from './FormFrame.svelte';
	import { profile, type Phase } from './data';

	/**
	 * The "Goals redesign" view: every step of the new goal flow as phone frames, from the empty
	 * New Goal form through research, questions, the stopped state and plan approval to the goal
	 * after planning, its agents sheet, Settings with the run time limit, and the profile screen.
	 */
	let { inboxCount }: { inboxCount: number } = $props();

	type Frame = { name: string; title: string; form?: boolean; filled?: boolean; adding?: boolean; phase?: Phase; sheet?: boolean; known?: boolean; settings?: boolean; profile?: boolean };
	const FRAMES: Frame[] = [
		{ name: '1. New goal, empty', title: 'New goal', form: true },
		{ name: '2. New goal, filled', title: 'New goal', form: true, filled: true },
		{ name: '2b. New goal, new category', title: 'New goal', form: true, filled: true, adding: true },
		{ name: '3. Researching, with Stop', title: 'Goal', phase: 'researching' },
		{ name: '4. Waiting for your answers', title: 'Goal', phase: 'questions' },
		{ name: '4b. Still true? from your profile', title: 'Goal', phase: 'questions', known: true },
		{ name: '5. Stopped after the time limit', title: 'Goal', phase: 'stopped' },
		{ name: '6. Plan ready to approve', title: 'Goal', phase: 'plan' },
		{ name: '7. Goal after planning', title: 'Goal', phase: 'active' },
		{ name: '8. Agents and coaches sheet', title: 'Goal', phase: 'active', sheet: true },
		{ name: '9. Settings, run time limit', title: 'Settings', settings: true },
		{ name: '10. What Aroh knows about you', title: 'What Aroh knows', settings: true, profile: true }
	];
</script>

<ul class="flex gap-6 overflow-x-auto pb-4">
	{#each FRAMES as f (f.name)}
		<li class="flex shrink-0 flex-col items-center gap-2">
			<div class="box-content h-[844px] w-[390px] [transform:translateZ(0)] overflow-hidden rounded-md border border-border bg-bg">
				<PhoneShell title={f.title} tab={f.settings ? 'settings' : 'goals'} {inboxCount}>
					{#if f.form}
						<FormFrame filled={f.filled} adding={f.adding} newName={f.adding ? 'Reading club' : ''} />
					{:else if f.phase}
						<GoalCard phase={f.phase} sheet={f.sheet} known={f.known} />
					{:else if f.profile}
						<ProfileScreen {profile} open="f1" />
					{:else}
						<section class="flex flex-col gap-3 px-4 pt-4" aria-labelledby="agents-defaults">
							<h2 id="agents-defaults" class="text-sm font-semibold text-fg">Agents</h2>
							<p class="text-xs text-muted">Defaults for new goals. Each goal can change them in its Agents and coaches row.</p>
							<AgentControls value={{ ...blankDraft(), runLimit: 5 }} limit="settings" />
							<button type="button" class="flex min-h-14 items-center gap-3 rounded-sm border border-border bg-bg px-3 py-2 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50">
								<span class="flex flex-1 flex-col"><span class="text-sm font-medium">What Aroh knows about you</span><span class="text-xs text-muted">5 facts from your answers and edits</span></span>
								<ChevronRight class="size-4 text-muted" aria-hidden="true" />
							</button>
						</section>
						<SettingsScreen settings={sixpackSettings} />
					{/if}
				</PhoneShell>
			</div>
			<p class="text-sm font-medium">{f.name}</p>
		</li>
	{/each}
</ul>
