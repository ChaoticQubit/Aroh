<script lang="ts">
	import { page } from '$app/state';
	import NewGoalScreen from '$lib/ui/screens/NewGoalScreen.svelte';
	import { GoalChat } from '$lib/app/chat.svelte';

	const chat = $derived(new GoalChat(page.params.id ?? null, 'What changed? Tell me and I will look at the plan.'));

	$effect(() => void chat.resume());
</script>

{#key chat}
	<NewGoalScreen messages={chat.messages} onchange={(e) => chat.handle(e)} echo={false} />
{/key}
