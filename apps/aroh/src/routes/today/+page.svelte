<script lang="ts">
	import { page } from '$app/state';
	import { invoke } from '@tauri-apps/api/core';
	import { emit } from '@tauri-apps/api/event';
	import TodayView from '$lib/calendar/TodayView.svelte';
	import PluginSlot from '$lib/plugins/PluginSlot.svelte';
	import FirstRun from '$lib/firstrun/FirstRun.svelte';

	const focusActionId = $derived(page.url.searchParams.get('action'));
	const dev = import.meta.env.DEV;
	let devStatus = $state('');

	/** Dev-only: creates a test goal and a 15 min action starting in 2 min through foundation's core commands, then a no-op status write so reminders resync. */
	async function addTestAction() {
		const now = new Date();
		const stamp = now.toISOString();
		try {
			const goal = await invoke<{ id: string }>('create_goal', {
				goal: {
					id: '',
					title: 'Test goal',
					kind: 'custom',
					source_url: null,
					target_date: new Date(now.getTime() + 7 * 86_400_000).toISOString().slice(0, 10),
					status: 'active',
					answers_json: null,
					created_at: stamp,
					updated_at: '',
					deleted: false
				}
			});
			const start = new Date(now.getTime() + 2 * 60_000);
			const end = new Date(start.getTime() + 15 * 60_000);
			const action = await invoke<{ id: string }>('create_action', {
				action: {
					id: '',
					goal_id: goal.id,
					title: 'Test action',
					notes: null,
					start_at: start.toISOString(),
					end_at: end.toISOString(),
					est_minutes: 15,
					status: 'planned',
					seq: 0,
					done_at: null,
					updated_at: '',
					deleted: false
				}
			});
			await invoke('calendar_set_status', { id: action.id, status: 'planned' });
			await emit('aroh://actions-changed');
			devStatus = 'Test action added';
		} catch (e) {
			devStatus = String(e);
		}
	}
</script>

<FirstRun />
<TodayView {focusActionId} />
<PluginSlot name="today.sidebar" />

{#if dev}
	<p class="dev">
		<button type="button" onclick={addTestAction}>Dev: add test action in 2 min</button>
		{devStatus}
	</p>
{/if}

<style>
	.dev {
		margin-top: var(--space-4);
		font-size: 0.8125rem;
		color: var(--ink-muted);
	}
	.dev button {
		min-height: 44px;
		padding: 0 var(--space-3);
	}
</style>
