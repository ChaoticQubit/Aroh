<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import PluginScreen from '$lib/ui/screens/PluginScreen.svelte';
	import { getPlugin, type PluginDetail } from '$lib/plugins/api';

	const id = $derived(page.params.id ?? '');
	const goalId = $derived(page.url.searchParams.get('goal'));
	const back = $derived(goalId ? `/goals/${encodeURIComponent(goalId)}` : '/personas');
	let plugin = $state<PluginDetail | null>(null);
	let status = $state<string | null>('Loading the plugin.');

	$effect(() => {
		const want = id;
		plugin = null;
		status = 'Loading the plugin.';
		getPlugin(want).then(
			(p) => {
				if (page.params.id !== want) return;
				plugin = p;
				status = null;
			},
			(e) => {
				if (page.params.id === want) status = `This plugin could not be opened: ${String(e)}`;
			}
		);
	});
</script>

{#if plugin}
	<PluginScreen {plugin} backLabel={goalId ? 'Back to the goal' : 'Back to coaches'} onback={() => void goto(back)} />
{:else}
	<div class="flex flex-col gap-3 p-4">
		<button type="button" class="flex h-11 w-fit items-center text-sm text-muted hover:text-fg" onclick={() => void goto(back)}>{goalId ? 'Back to the goal' : 'Back to coaches'}</button>
		{#if status}<p class="text-sm text-muted" role="status">{status}</p>{/if}
	</div>
{/if}
