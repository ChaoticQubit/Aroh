<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import GoalScreen from '$lib/ui/screens/GoalScreen.svelte';
	import RoutinesScreen from '$lib/ui/screens/RoutinesScreen.svelte';
	import * as location from '$lib/app/location';
	import { onMount } from 'svelte';
	import { getClock } from '$lib/ui/clock';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { goalHeader, goalView } from '$lib/app/views';
	import { engine } from '$lib/app/engine';
	import { activeVersion, enablePlugin, listPlugins, type PluginVersion } from '$lib/plugins/api';
	import { parseTree, resolve } from '$lib/plugins/resolve';
	import type { AutonomyLabel, CoachView } from '$lib/ui/screens/types';
	import { loadCoaches } from '$lib/app/personas';

	const DERIVED = 'derived';
	const SHOWN_VERSIONS = 3;

	const clock = getClock();
	const s = $derived(data.snapshot);
	const goal = $derived(s?.goals.find((g) => g.id === page.params.id));
	let plugins = $state<PluginVersion[]>([]);
	let places = $state<location.LocationView | null>(null);
	const routines = $derived(!!goal && !!places && goal.id === places.routines_goal);

	onMount(() => location.follow((v) => (places = v), (e) => (data.error = String(e))));
	let chatStatus = $state<string | null>(null);
	let coaches = $state<CoachView[]>([]);

	$effect(() => {
		const id = page.params.id;
		if (!id) return;
		void engine.runs({ goalId: id, state: 'waiting_human' }).then(async (r) => {
			const waits = r.ok ? await engine.waits(r.value) : {};
			const label = r.ok ? r.value.map((x) => waits[x.run_id]).find(Boolean) : undefined;
			chatStatus = label ? `Waiting for you: ${label}. Open the chat to answer.` : null;
		});
	});

	$effect(() => {
		const id = goal?.id;
		if (s && id) void loadCoaches(s, id).then((r) => (coaches = r.coaches));
	});

	$effect(() => {
		const id = goal?.id;
		if (!s || !id) return;
		listPlugins(id).then(
			(list) => (plugins = list),
			(e) => (data.error = String(e))
		);
	});

	const shown = $derived(activeVersion(plugins));
	const view = $derived.by(() => {
		if (!s || !goal) return null;
		if (!shown) return goalView(s, goal, clock);
		try {
			return resolve(parseTree(shown.tree), s, goal, clock);
		} catch {
			return goalView(s, goal, clock);
		}
	});
	const versions = $derived(
		plugins.length
			? [
					{ value: DERIVED, label: 'Derived' },
					...plugins.filter((p, i) => p.tree !== null && (i < SHOWN_VERSIONS || p.active)).map((p) => ({ value: p.id, label: `v${p.version}` }))
				]
			: []
	);

	/** Saves the picked autonomy level. */
	function onautonomy(level: AutonomyLabel) {
		if (goal) void data.run(() => client.setAutonomy(goal.id, level.toLowerCase() as client.Goal['autonomy']));
	}

	/** Opens the goal's persona chat, which answers its waiting run or starts a replan. */
	function onchat() {
		if (goal) void goto(`/goals/${goal.id}/chat`);
	}

	/** Switches the dashboard to a plugin version, or to the derived view. */
	function onversion(value: string) {
		if (goal) void data.run(async () => (plugins = await enablePlugin(goal.id, value === DERIVED ? null : value)));
	}
</script>

{#if s && goal && view && routines && places}
	<RoutinesScreen goal={goalHeader(s, goal, clock.zone)} view={goalView(s, goal, clock)} routines={location.routineRows(places, clock.now, clock.zone)} />
{:else if s && goal && view}
	<GoalScreen goal={goalHeader(s, goal, clock.zone)} {view} {chatStatus} {versions} version={shown?.id ?? DERIVED} {coaches} {onautonomy} {onchat} {onversion} oncoaches={() => void goto(`/personas?goal=${goal.id}`)} />
{:else if s}
	<p class="p-4 text-sm text-muted">This goal no longer exists.</p>
{/if}
