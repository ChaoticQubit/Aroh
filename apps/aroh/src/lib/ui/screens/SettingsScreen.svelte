<script lang="ts">
	import Plus from '@lucide/svelte/icons/plus';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import { Input } from '$lib/ui/primitives/input';
	import * as Select from '$lib/ui/primitives/select';
	import { Switch } from '$lib/ui/primitives/switch';
	import Field from '../components/Field.svelte';
	import StatusChip from '../components/StatusChip.svelte';
	import { providerCatalog, searchProviders } from '$lib/ui/mockdata/sixpack';

	/**
	 * Settings: model provider, model and key (the key goes to the server, never stored on the device),
	 * search providers, server URL, monthly budgets, and calendar and Apple Health access.
	 */
	const id = $props.id();
	let providerId = $state(providerCatalog[0].id);
	const provider = $derived(providerCatalog.find((p) => p.id === providerId) ?? providerCatalog[0]);
	let model = $derived(provider.models[0]);
	let key = $state('');
	let status = $state<string | null>(null);
	let calendars = $state(true);
	let health = $state(false);
	const input = 'h-11 text-lg';
</script>

<div class="flex w-full max-w-2xl flex-col gap-6 p-4 @3xl:p-6">
	<section class="flex flex-col gap-3" aria-labelledby="{id}-model">
		<h2 id="{id}-model" class="text-sm font-semibold text-fg">Model provider</h2>
		<Card.Root size="sm" class="gap-4 px-4">
			<Field label="Provider" labelId="{id}-provider">
				<Select.Root type="single" bind:value={providerId} onValueChange={() => (status = null)}>
					<Select.Trigger class="h-11! w-full text-base" aria-labelledby="{id}-provider">{provider.name}</Select.Trigger>
					<Select.Content>
						{#each providerCatalog as p (p.id)}<Select.Item value={p.id} label={p.name} class="min-h-11">{p.name}</Select.Item>{/each}
					</Select.Content>
				</Select.Root>
			</Field>
			<Field label="Model" labelId="{id}-modelname">
				<Select.Root type="single" bind:value={model}>
					<Select.Trigger class="h-11! w-full text-sm" aria-labelledby="{id}-modelname">{model}</Select.Trigger>
					<Select.Content>
						{#each provider.models as m (m)}<Select.Item value={m} label={m} class="min-h-11">{m}</Select.Item>{/each}
					</Select.Content>
				</Select.Root>
			</Field>
			<Field label="API key" id="{id}-key" detail={provider.keySet ? 'Key saved' : 'No key yet'}>
				<Input id="{id}-key" type="password" autocomplete="off" placeholder={provider.keySet ? 'Saved. Enter a new key to replace it' : 'Paste your key'} class={input} bind:value={key} aria-describedby="{id}-keyhelp" />
				<p id="{id}-keyhelp" class="text-xs text-muted">Saved on the server, never on this device.</p>
			</Field>
			<div class="flex flex-wrap items-center gap-3">
				<Button variant="outline" size="touch" class="@3xl:h-8" onclick={() => (status = provider.keySet || key ? 'Test sent to the server. The result appears here.' : 'Enter a key first.')}>Test connection</Button>
				{#if status}<p class="text-sm text-muted" role="status">{status}</p>{/if}
			</div>
		</Card.Root>
	</section>

	<section class="flex flex-col gap-3" aria-labelledby="{id}-search">
		<div class="flex items-center justify-between">
			<h2 id="{id}-search" class="text-sm font-semibold text-fg">Search providers</h2>
			<Button variant="outline" size="touch" class="@3xl:h-8"><Plus />Add</Button>
		</div>
		<Card.Root size="sm" class="gap-0 divide-y divide-border px-4 py-0">
			{#each searchProviders as s, i (s.name)}
				<div class="flex min-h-14 items-center gap-3 py-2">
					<span class="w-5 text-xs text-muted tabular-nums">{i + 1}</span>
					<div class="flex flex-1 flex-col">
						<span class="text-sm font-medium">{s.name}</span>
						<span class="text-xs text-muted">{s.detail}</span>
					</div>
					<StatusChip tone={s.keySet ? 'done' : 'warn'} label={s.keySet ? 'Ready' : 'Needs key'} />
				</div>
			{/each}
		</Card.Root>
	</section>

	<section class="flex flex-col gap-3" aria-labelledby="{id}-server">
		<h2 id="{id}-server" class="text-sm font-semibold text-fg">Server and budget</h2>
		<Card.Root size="sm" class="gap-4 px-4">
			<Field label="Server URL" id="{id}-url">
				<Input id="{id}-url" type="url" inputmode="url" value="https://aroh-engine.example.com" class="{input} text-sm" />
			</Field>
			<div class="grid gap-4 @md:grid-cols-2">
				<Field label="Monthly budget, all goals" id="{id}-budget">
					<Input id="{id}-budget" type="number" inputmode="decimal" value="20" class={input} />
				</Field>
				<Field label="Monthly budget per goal" id="{id}-goalbudget">
					<Input id="{id}-goalbudget" type="number" inputmode="decimal" value="8" class={input} />
				</Field>
			</div>
			<p class="text-xs text-muted">In US dollars. Agents pause and ask when a budget is reached.</p>
		</Card.Root>
	</section>

	<section class="flex flex-col gap-3" aria-labelledby="{id}-access">
		<h2 id="{id}-access" class="text-sm font-semibold text-fg">Access</h2>
		<Card.Root size="sm" class="gap-0 divide-y divide-border px-4 py-0">
			<label class="flex min-h-14 cursor-pointer items-center gap-3 py-2" for="{id}-cal">
				<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Calendars</span><span class="text-xs text-muted">Read device calendars as busy time. Nothing is written back.</span></span>
				<Switch id="{id}-cal" bind:checked={calendars} />
			</label>
			<label class="flex min-h-14 cursor-pointer items-center gap-3 py-2" for="{id}-health">
				<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Apple Health</span><span class="text-xs text-muted">Import weight, workouts and steps on iPhone.</span></span>
				<Switch id="{id}-health" bind:checked={health} />
			</label>
		</Card.Root>
	</section>
</div>
