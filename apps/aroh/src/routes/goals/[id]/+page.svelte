<script lang="ts">
	import { page } from '$app/state';
	import GoalScreen from '$lib/ui/screens/GoalScreen.svelte';
	import { getClock } from '$lib/ui/clock';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { engine, NEEDS_ENGINE } from '$lib/app/engine';
	import { goalHeader, goalView } from '$lib/app/views';
	import type { AutonomyLabel } from '$lib/ui/screens/types';

	const clock = getClock();
	const s = $derived(data.snapshot);
	const goal = $derived(s?.goals.find((g) => g.id === page.params.id));
	let chatStatus = $state<string | null>(null);

	/** Saves the picked autonomy level. */
	function onautonomy(level: AutonomyLabel) {
		if (goal) void data.run(() => client.setAutonomy(goal.id, level.toLowerCase() as client.Goal['autonomy']));
	}

	/** Shows why the persona chat cannot open while the engine is not connected. */
	function onchat() {
		chatStatus = engine.connected ? null : NEEDS_ENGINE;
	}
</script>

{#if s && goal}
	<GoalScreen goal={goalHeader(s, goal, clock.zone)} view={goalView(s, goal, clock)} {chatStatus} {onautonomy} {onchat} />
{:else if s}
	<p class="p-4 text-sm text-muted">This goal no longer exists.</p>
{/if}
