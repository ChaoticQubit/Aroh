<script lang="ts">
	import { page } from '$app/state';
	import NewGoalScreen from '$lib/ui/screens/NewGoalScreen.svelte';
	import { GoalChat } from '$lib/app/chat.svelte';

	const goalId = $derived(page.url.searchParams.get('goal'));
	const chat = $derived(new GoalChat(goalId, goalId ? 'What changed? Tell me and I will look at the plan.' : 'Tell me the goal you want to reach and by when.'));

	$effect(() => void chat.resume());

	/** Hands the chat's send, approve and change events to the goal chat. */
	function onchange(event: unknown) {
		const e = event as { type: string; text?: string };
		if (e.type === 'send' && e.text) void chat.send(e.text);
		if (e.type === 'approve') void chat.decide(true);
		if (e.type === 'change') void chat.decide(false);
	}
</script>

{#key chat}
	<NewGoalScreen messages={chat.messages} {onchange} echo={false} />
{/key}
