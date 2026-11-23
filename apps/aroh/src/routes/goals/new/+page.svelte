<script lang="ts">
	import NewGoalScreen from '$lib/ui/screens/NewGoalScreen.svelte';
	import { GoalChat } from '$lib/app/chat.svelte';
	import { speech } from '$lib/app/speech.svelte';

	const chat = new GoalChat(null, 'Tell me the goal you want to reach and by when.');
</script>

<NewGoalScreen messages={chat.messages} onchange={(e) => chat.handle(e)} echo={false} voice={speech} />
