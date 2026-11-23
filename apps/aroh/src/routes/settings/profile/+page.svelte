<script lang="ts">
	import ProfileScreen from '$lib/ui/screens/ProfileScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { profileView } from '$lib/app/views';

	const s = $derived(data.snapshot);
</script>

{#if s}
	<ProfileScreen
		profile={profileView(s)}
		onedit={(id, text) => data.run(() => client.editFact(id, text))}
		ondelete={(id) => data.run(() => client.decidePreference(id, false))}
		onblock={(id) => data.run(() => client.blockTopic(id))}
		onallow={(id) => data.run(() => client.allowTopic(id))}
	/>
{/if}
