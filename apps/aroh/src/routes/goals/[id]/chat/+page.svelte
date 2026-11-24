<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';

	$effect(() => void goto(`/goals/${page.params.id}`, { replaceState: true }));
</script>
