<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import PersonasScreen from '$lib/ui/screens/PersonasScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import { engine, type RunSummary } from '$lib/app/engine';
	import { coachesOn, groupByTeam, type PersonaView } from '$lib/app/personas';
	import { categoryList } from '$lib/app/views';

	const s = $derived(data.snapshot);
	const goalId = $derived(page.url.searchParams.get('goal'));
	const goal = $derived(goalId ? s?.goals.find((g) => g.id === goalId) : undefined);
	let views = $state<PersonaView[]>([]);
	let runs = $state<RunSummary[]>([]);
	let status = $state<string | null>('Loading coaches.');

	$effect(() => {
		void engine.personas().then((r) => {
			views = r.ok ? r.value : [];
			status = r.ok ? null : r.message;
		});
	});

	$effect(() => {
		if (!goalId) return;
		void engine.runs({ goalId }).then((r) => (runs = r.ok ? r.value : []));
	});

	const groups = $derived(groupByTeam(views, s ? categoryList(s) : []));
	const coaches = $derived(goal && s ? coachesOn(views, s.personas, runs, goal.id) : []);
	const query = $derived(goal ? `?goal=${goal.id}` : '');
</script>

<PersonasScreen
	{groups}
	{coaches}
	goalTitle={goal?.title ?? null}
	{status}
	backLabel={goal ? goal.title : 'Settings'}
	onopen={(name) => void goto(`/personas/${name}${query}`)}
	onback={() => void goto(goal ? `/goals/${goal.id}` : '/settings')}
/>
