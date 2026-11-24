<script lang="ts">
	import { onMount } from 'svelte';
	import AgentControls from '$lib/ui/components/goal/AgentControls.svelte';
	import Field from '$lib/ui/components/Field.svelte';
	import { Input } from '$lib/ui/primitives/input';
	import { data } from '$lib/app/data.svelte';
	import * as client from '$lib/app/client';
	import { engine } from '$lib/app/engine';
	import { agentDefaults, defaultsValue } from '$lib/app/newgoal';

	const s = $derived(data.snapshot);
	let runLimit = $state<number | null>(null);
	let connected = $state(false);

	onMount(async () => {
		const limit = await engine.runLimit();
		connected = limit.ok;
		runLimit = limit.ok ? limit.value.minutes : null;
	});

	/** Saves a whole number of minutes from 1 to 120; other input is ignored. */
	function minutes(text: string) {
		const n = Math.round(Number(text));
		if (!Number.isFinite(n) || n < 1 || n > 120) return;
		void engine.setRunLimit(n).then((r) => (r.ok ? (runLimit = r.value.minutes) : (data.error = r.message)));
	}
</script>

{#if s}
	<div class="flex w-full max-w-2xl flex-col gap-6 p-4 @3xl:p-6">
		<section class="flex flex-col gap-2" aria-labelledby="agents-defaults">
			<h2 id="agents-defaults" class="px-1 text-sm font-semibold">New goals start with</h2>
			<p class="px-1 text-xs text-muted">Each goal can change these in its Coaches and plugins card.</p>
			<AgentControls value={agentDefaults(s.settings.agent_defaults)} onchange={(v) => data.run(() => client.setSetting('agent_defaults', defaultsValue(v)))} />
		</section>
		{#if connected}
			<section class="flex flex-col gap-2" aria-labelledby="agents-limit">
				<h2 id="agents-limit" class="px-1 text-sm font-semibold">Time limit per run</h2>
				<div class="flex flex-col gap-3 rounded-sm border border-border bg-bg p-4">
					<Field label="Minutes, 1 to 120" id="agents-minutes">
						<Input id="agents-minutes" type="number" inputmode="numeric" min="1" max="120" value={runLimit ?? 5} onchange={(e) => minutes(e.currentTarget.value)} class="h-11 text-lg" />
					</Field>
					<p class="text-xs text-muted">Counts only time agents work, never time waiting for you. A run stops at the limit; retry it from its goal.</p>
				</div>
			</section>
		{/if}
	</div>
{/if}
