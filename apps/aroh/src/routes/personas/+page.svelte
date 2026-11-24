<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import PersonasScreen from '$lib/ui/screens/PersonasScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import { groupByTeam, loadCoaches, type PersonaView } from '$lib/app/personas';
	import type { CoachView } from '$lib/ui/screens/types';
	import { categoryList } from '$lib/app/views';

	const s = $derived(data.snapshot);
	const goalId = $derived(page.url.searchParams.get('goal'));
	const goal = $derived(goalId ? s?.goals.find((g) => g.id === goalId) : undefined);
	let views = $state<PersonaView[]>([]);
	let coaches = $state<CoachView[]>([]);
	let status = $state<string | null>('Loading coaches.');

	$effect(() => {
		if (!s) return;
		void loadCoaches(s, goal?.id ?? null).then((r) => {
			views = r.views;
			coaches = r.coaches;
			status = r.status;
		});
	});

	const groups = $derived(groupByTeam(views, s ? categoryList(s) : []));
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
