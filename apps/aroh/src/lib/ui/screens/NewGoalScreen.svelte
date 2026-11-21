<script lang="ts">
	import PersonaChat from '../components/agent/PersonaChat.svelte';
	import { sixpackChat } from '$lib/ui/mockdata/sixpack';
</script>

<div class="flex h-full w-full max-w-3xl flex-col @3xl:p-6">
	<div class="flex min-h-0 flex-1 flex-col">
		<PersonaChat messages={sixpackChat} />
	</div>
</div>
