<script lang="ts">
	import { page } from '$app/state';
	import PluginSlot from '../../../lib/plugins/PluginSlot.svelte';
</script>

<h1>Goal</h1>
<PluginSlot name="goal.detail" goalId={page.params.id} />
