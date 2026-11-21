<script lang="ts">
	import NewGoalScreen from '$lib/ui/screens/NewGoalScreen.svelte';
	import type { ChatMessage } from '$lib/ui/contract';
	import { engine, NEEDS_ENGINE } from '$lib/app/engine';

	const greeting: ChatMessage = { type: 'message', from: 'persona', name: 'Aroh', text: 'Tell me the goal you want to reach and by when.' };
	const offline: ChatMessage = { type: 'message', from: 'persona', name: 'Aroh', text: NEEDS_ENGINE };
	let messages = $state<ChatMessage[]>(engine.connected ? [greeting] : [greeting, offline]);

	/** Hands a sent message to the engine; its reply cards join the thread. */
	async function onchange(event: unknown) {
		const e = event as { type: string; text?: string };
		if (e.type !== 'send' || !e.text) return;
		const result = await engine.startGoal(e.text);
		if (result.ok) messages = [...messages, ...result.value];
	}
</script>

<NewGoalScreen {messages} {onchange} />
