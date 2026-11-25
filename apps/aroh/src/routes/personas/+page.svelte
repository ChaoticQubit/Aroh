<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import PersonasScreen from '$lib/ui/screens/PersonasScreen.svelte';
	import { engine } from '$lib/app/engine';
	import { data } from '$lib/app/data.svelte';
	import { groupByTeam, loadCoaches, type PersonaView } from '$lib/app/personas';
	import type { CoachView } from '$lib/ui/screens/types';
	import { categoryList } from '$lib/app/views';
	import { allPlugins } from '$lib/plugins/api';
	import { pluginRows, type PluginRow } from '$lib/plugins/describe';

	const s = $derived(data.snapshot);
	const goalId = $derived(page.url.searchParams.get('goal'));
	const goal = $derived(goalId ? s?.goals.find((g) => g.id === goalId) : undefined);
	let views = $state<PersonaView[]>([]);
	let coaches = $state<CoachView[]>([]);
	let status = $state<string | null>('Loading coaches.');
	let plugins = $state<PluginRow[] | null>(null);
	let pluginStatus = $state<string | null>(null);

	$effect(() => {
		allPlugins().then(
			(list) => (plugins = pluginRows(list)),
			(e) => {
				plugins = [];
				pluginStatus = `Plugins could not be loaded: ${String(e)}`;
			}
		);
	});

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

	const cached = engine.cachedPersonas();
	if (cached) {
		views = cached;
		status = null;
	}
</script>

<PersonasScreen
	{groups}
	{coaches}
	goalTitle={goal?.title ?? null}
	{status}
	{plugins}
	{pluginStatus}
	backLabel={goal ? goal.title : 'Settings'}
	onopen={(name) => void goto(`/personas/${name}${query}`)}
	onback={() => void goto(goal ? `/goals/${goal.id}` : '/settings')}
/>
