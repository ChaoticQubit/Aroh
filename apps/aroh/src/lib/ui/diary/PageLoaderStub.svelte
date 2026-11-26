<script lang="ts">
	/** Test stand-in for the notebook PageLoader: prints the page it was asked to open. */
	let { kind, target }: { kind: string; target: string; embedded?: boolean } = $props();
</script>

<p data-testid="page-loader">page {kind} {target}</p>
