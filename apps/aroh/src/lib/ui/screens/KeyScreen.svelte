<script lang="ts">
	import { Button } from '$lib/ui/primitives/button';
	import { Input } from '$lib/ui/primitives/input';
	import Field from '../components/Field.svelte';
	import type { SettingsView } from './types';

	/**
	 * Settings, Claude key and budget (Q72, Q76): the key of the chosen provider (Claude by
	 * default) with a save-and-test, the model list, the other providers behind "More providers",
	 * and the monthly budgets. `ontest` resolves to a status line; the rest report edits.
	 */
	let {
		settings,
		onprovider,
		ontest,
		onbudget
	}: {
		settings: SettingsView;
		onprovider?: (providerId: string, model: string) => void;
		ontest?: (providerId: string, key: string) => Promise<string>;
		onbudget?: (scope: 'all' | 'goal', usd: number | null) => void;
	} = $props();

	const id = $props.id();
	const provider = $derived(settings.providers.find((p) => p.id === settings.providerId) ?? settings.providers[0]);
	const claude = $derived(provider.id === 'anthropic');
	let more = $state(false);
	let key = $state('');
	let status = $state<string | null>(null);
	let testing = $state(false);
	const input = 'h-11 text-lg';
	const radio = 'flex min-h-13 w-full items-center gap-3 px-4 py-2 text-left outline-none hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset';

	/** Sends the typed key to the engine through `ontest` and shows the answer. */
	async function test() {
		if (!ontest || testing) return;
		testing = true;
		status = 'Checking the key.';
		status = await ontest(provider.id, key.trim());
		key = '';
		testing = false;
	}

	/** Switches provider with its first model and clears the status line. */
	function pick(next: string) {
		const p = settings.providers.find((c) => c.id === next);
		if (!p || p.id === provider.id) return;
		status = null;
		onprovider?.(p.id, p.models[0]);
	}

	/** Parses a dollar input: empty is null, anything else a non-negative number or null. */
	function dollars(text: string): number | null {
		const n = Number(text);
		return text.trim() === '' || !Number.isFinite(n) || n < 0 ? null : n;
	}
</script>

{#snippet dot(on: boolean)}
	<span class="flex size-5 shrink-0 items-center justify-center rounded-full border {on ? 'border-fg' : 'border-border-strong'}" aria-hidden="true">{#if on}<span class="size-2.5 rounded-full bg-fg"></span>{/if}</span>
{/snippet}

<div class="flex w-full max-w-2xl flex-col gap-6 p-4 @3xl:p-6">
	<section class="flex flex-col gap-2" aria-labelledby="{id}-key">
		<h2 id="{id}-key" class="px-1 text-sm font-semibold">{claude ? 'Claude key' : `${provider.name} key`}</h2>
		<div class="flex flex-col gap-3 rounded-sm border border-border bg-bg p-4">
			<Field label="API key" id="{id}-input" detail={provider.last4 ? `Saved, ends ${provider.last4}` : provider.keySet ? 'Saved' : 'No key yet'}>
				<Input id="{id}-input" type="password" autocomplete="off" placeholder={provider.keySet ? 'Enter a new key to replace it' : claude ? 'sk-ant-...' : 'Paste your key'} class={input} bind:value={key} />
			</Field>
			<p class="text-xs text-muted">Stored encrypted on the server, never on this phone.</p>
			<Button variant="outline" class="h-11 self-start" disabled={testing || (!key.trim() && !provider.keySet)} onclick={test}>{key.trim() ? 'Save and test' : 'Test connection'}</Button>
			{#if status}<p class="text-sm text-muted" role="status">{status}</p>{/if}
		</div>
	</section>

	<section class="flex flex-col gap-2" aria-labelledby="{id}-model">
		<h2 id="{id}-model" class="px-1 text-sm font-semibold">Model</h2>
		<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg" role="radiogroup" aria-labelledby="{id}-model">
			{#each provider.models as m (m)}
				<li><button type="button" role="radio" aria-checked={m === settings.model} class={radio} onclick={() => onprovider?.(provider.id, m)}>{@render dot(m === settings.model)}<span class="min-w-0 flex-1 text-base break-words">{m}</span></button></li>
			{/each}
		</ul>
	</section>

	<section class="flex flex-col gap-2" aria-labelledby="{id}-providers">
		{#if more || !claude}
			<h2 id="{id}-providers" class="px-1 text-sm font-semibold">Provider</h2>
			<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg" role="radiogroup" aria-labelledby="{id}-providers">
				{#each settings.providers as p (p.id)}
					<li>
						<button type="button" role="radio" aria-checked={p.id === provider.id} class={radio} onclick={() => pick(p.id)}>
							{@render dot(p.id === provider.id)}
							<span class="min-w-0 flex-1 text-base">{p.id === 'anthropic' ? 'Claude (Anthropic)' : p.name}</span>
							{#if p.keySet}<span class="text-sm text-muted">Key saved</span>{/if}
						</button>
					</li>
				{/each}
			</ul>
		{:else}
			<Button id="{id}-providers" variant="ghost" class="h-11 self-start" onclick={() => (more = true)}>More providers</Button>
		{/if}
	</section>

	<section class="flex flex-col gap-2" aria-labelledby="{id}-budget">
		<h2 id="{id}-budget" class="px-1 text-sm font-semibold">Monthly budget</h2>
		<div class="flex flex-col gap-4 rounded-sm border border-border bg-bg p-4">
			<Field label="All goals, US dollars" id="{id}-all">
				<Input id="{id}-all" type="number" inputmode="decimal" value={settings.budgetAll ?? ''} onchange={(e) => onbudget?.('all', dollars(e.currentTarget.value))} class={input} />
			</Field>
			<Field label="Per goal, US dollars" id="{id}-goal">
				<Input id="{id}-goal" type="number" inputmode="decimal" value={settings.budgetGoal ?? ''} onchange={(e) => onbudget?.('goal', dollars(e.currentTarget.value))} class={input} />
			</Field>
			<p class="text-xs text-muted">Agents pause and ask when a budget is reached.</p>
		</div>
	</section>
</div>
