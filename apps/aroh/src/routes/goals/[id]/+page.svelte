<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import GoalScreen from '$lib/ui/screens/GoalScreen.svelte';
	import { getClock } from '$lib/ui/clock';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { goalHeader, goalView } from '$lib/app/views';
	import type { AutonomyLabel } from '$lib/ui/screens/types';

	const clock = getClock();
	const s = $derived(data.snapshot);
	const goal = $derived(s?.goals.find((g) => g.id === page.params.id));

	/** Saves the picked autonomy level. */
	function onautonomy(level: AutonomyLabel) {
		if (goal) void data.run(() => client.setAutonomy(goal.id, level.toLowerCase() as client.Goal['autonomy']));
	}

	/** Opens the goal's persona chat, which answers its waiting run or starts a replan. */
	function onchat() {
		if (goal) void goto(`/goals/new?goal=${goal.id}`);
	}
</script>

{#if s && goal}
	<GoalScreen goal={goalHeader(s, goal, clock.zone)} view={goalView(s, goal, clock)} {onautonomy} {onchat} />
{:else if s}
	<p class="p-4 text-sm text-muted">This goal no longer exists.</p>
{/if}
