<script lang="ts">
	import InboxScreen from '$lib/ui/screens/InboxScreen.svelte';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { inboxView } from '$lib/app/views';

	const s = $derived(data.snapshot);

	/** Applies or rejects a proposal, or confirms or dismisses a suggestion. */
	function ondecide(group: 'approval' | 'suggestion' | 'insight', id: string, decision: unknown) {
		const yes = decision === 'apply';
		if (group === 'approval') void data.run(() => client.decideProposal(id, yes));
		if (group === 'suggestion') void data.run(() => client.decidePreference(id, yes));
	}
</script>

{#if s}
	<InboxScreen {...inboxView(s)} {ondecide} />
{/if}
