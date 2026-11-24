<script lang="ts">
	import PersonaChat from '../components/agent/PersonaChat.svelte';
	import type { ChatMessage, OnChange } from '$lib/ui/contract';
	import type { Speech } from '$lib/app/speech.svelte';

	/**
	 * Persona chat screen, for a new goal or one goal's chat. `onchange` gets the chat's send, approve and change events;
	 * `echo` false means `messages` already holds the owner's sent text; `voice` adds the mic.
	 */
	let { messages, onchange, echo = true, voice }: { messages: ChatMessage[]; onchange?: OnChange; echo?: boolean; voice?: Speech } = $props();
</script>

<div class="flex h-full w-full max-w-3xl flex-col @3xl:p-6">
	<div class="flex min-h-0 flex-1 flex-col">
		<PersonaChat {messages} {onchange} {echo} {voice} keep="goal:new" />
	</div>
</div>
