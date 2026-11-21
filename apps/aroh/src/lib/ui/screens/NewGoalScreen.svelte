<script lang="ts">
	import PersonaChat from '../components/agent/PersonaChat.svelte';
	import type { ChatMessage, OnChange } from '$lib/ui/contract';

	/** New goal: the persona chat thread. `onchange` gets the chat's send, approve and change events. */
	let { messages, onchange }: { messages: ChatMessage[]; onchange?: OnChange } = $props();
</script>

<div class="flex h-full w-full max-w-3xl flex-col @3xl:p-6">
	<div class="flex min-h-0 flex-1 flex-col">
		<PersonaChat {messages} {onchange} />
	</div>
</div>
