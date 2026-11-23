<script lang="ts">
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import Plus from '@lucide/svelte/icons/plus';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import { Input } from '$lib/ui/primitives/input';
	import * as Select from '$lib/ui/primitives/select';
	import { Switch } from '$lib/ui/primitives/switch';
	import Field from '../components/Field.svelte';
	import StatusChip from '../components/StatusChip.svelte';
	import type { Snippet } from 'svelte';
	import type { SettingsView } from './types';

	/**
	 * Settings: account sign-in, model provider, model and key (the key goes to the server, never
	 * stored on the device), search providers, server URL, budgets, the run time limit, and calendar and Apple
	 * Health access. `ontest`, `onsignin` and `onhealth` resolve to a status line; the rest report edits.
	 * With `oncoaches`, a Coaches row opens the coach (persona) library; `onlocation` adds a Location row;
	 * `agents` renders the new-goal agent defaults.
	 */
	let {
		settings,
		onprovider,
		ontest,
		onsetting,
		onbudget,
		onsignin,
		onsignout,
		onhealth,
		oncoaches,
		runLimit = null,
		onrunlimit,
		onlocation,
		agents
	}: {
		settings: SettingsView;
		onprovider?: (providerId: string, model: string) => void;
		ontest?: (providerId: string, key: string) => Promise<string>;
		onsetting?: (key: 'server_url' | 'calendars' | 'health' | 'supabase_url' | 'supabase_anon_key', value: string) => void;
		onbudget?: (scope: 'all' | 'goal', usd: number | null) => void;
		onsignin?: (email: string, password: string, create: boolean) => Promise<string>;
		onsignout?: () => void;
		onhealth?: (on: boolean) => Promise<string>;
		oncoaches?: () => void;
		runLimit?: number | null;
		onrunlimit?: (minutes: number) => void;
		onlocation?: () => void;
		agents?: Snippet;
	} = $props();

	const id = $props.id();
	const providerCatalog = $derived(settings.providers);
	let providerId = $derived(settings.providerId);
	const provider = $derived(providerCatalog.find((p) => p.id === providerId) ?? providerCatalog[0]);
	let model = $derived(provider.id === settings.providerId && provider.models.includes(settings.model) ? settings.model : provider.models[0]);
	let key = $state('');
	let status = $state<string | null>(null);
	let calendars = $derived(settings.calendars);
	let health = $derived(settings.health);
	let email = $state('');
	let password = $state('');
	let accountStatus = $state<string | null>(null);
	let healthStatus = $state<string | null>(null);
	const input = 'h-11 text-lg';

	/** Connects or stops Apple Health through `onhealth` and shows its answer under the switch. */
	async function toggleHealth(on: boolean) {
		if (!onhealth) return onsetting?.('health', String(on));
		healthStatus = on ? 'Connecting to Apple Health.' : null;
		healthStatus = await onhealth(on);
	}

	/** Signs in or creates the account through `onsignin`; the password is cleared either way. */
	async function signIn(create: boolean) {
		if (!onsignin) return;
		accountStatus = create ? 'Creating the account.' : 'Signing in.';
		accountStatus = await onsignin(email.trim(), password, create);
		password = '';
	}

	/** Reports a picked provider with its first model. */
	function pickProvider(next: string) {
		status = null;
		const p = providerCatalog.find((c) => c.id === next);
		if (p) onprovider?.(p.id, p.models[0]);
	}

	/** Sends the typed key to the engine through `ontest`, or shows the offline status line. */
	async function test() {
		if (!ontest) {
			status = provider.keySet || key ? 'Test sent to the server. The result appears here.' : 'Enter a key first.';
			return;
		}
		status = await ontest(provider.id, key);
		key = '';
	}

	/** Saves a whole number of minutes from 1 to 120 through `onrunlimit`; other input is ignored. */
	function minutes(text: string) {
		const n = Math.round(Number(text));
		if (Number.isFinite(n) && n >= 1 && n <= 120) onrunlimit?.(n);
	}

	/** Parses a dollar input: empty is null, anything else a non-negative number or null. */
	function dollars(text: string): number | null {
		const n = Number(text);
		return text.trim() === '' || !Number.isFinite(n) || n < 0 ? null : n;
	}
</script>

<div class="flex w-full max-w-2xl flex-col gap-6 p-4 @3xl:p-6">
	<section class="flex flex-col gap-3" aria-labelledby="{id}-account">
		<h2 id="{id}-account" class="text-sm font-semibold text-fg">Account</h2>
		<Card.Root size="sm" class="gap-4 px-4">
			<Field label="Project URL" id="{id}-sburl">
				<Input id="{id}-sburl" type="url" inputmode="url" placeholder="https://your-project.supabase.co" value={settings.account.supabaseUrl} onchange={(e) => onsetting?.('supabase_url', e.currentTarget.value.trim())} class="{input} text-lg" />
			</Field>
			<Field label="Anon key" id="{id}-anon">
				<Input id="{id}-anon" autocomplete="off" placeholder="Public anon key" value={settings.account.anonKey} onchange={(e) => onsetting?.('supabase_anon_key', e.currentTarget.value.trim())} class="{input} text-lg" />
			</Field>
			{#if settings.account.email}
				<div class="flex flex-wrap items-center gap-3">
					<StatusChip tone="done" label="Signed in" />
					<p class="flex-1 text-sm text-fg">{settings.account.email}</p>
					<Button variant="outline" size="touch" class="@3xl:h-8" onclick={() => onsignout?.()}>Sign out</Button>
				</div>
			{:else}
				<div class="grid gap-4 @md:grid-cols-2">
					<Field label="Email" id="{id}-email">
						<Input id="{id}-email" type="email" inputmode="email" autocomplete="username" class="{input} text-lg" bind:value={email} />
					</Field>
					<Field label="Password" id="{id}-password">
						<Input id="{id}-password" type="password" autocomplete="current-password" class="{input} text-lg" bind:value={password} />
					</Field>
				</div>
				<div class="flex flex-wrap items-center gap-3">
					<Button size="touch" class="@3xl:h-8" onclick={() => signIn(false)}>Sign in</Button>
					<Button variant="outline" size="touch" class="@3xl:h-8" onclick={() => signIn(true)}>Create account</Button>
				</div>
			{/if}
			{#if accountStatus}<p class="text-sm text-muted" role="status">{accountStatus}</p>{/if}
		</Card.Root>
	</section>

	{#if onlocation}
		<section class="flex flex-col gap-3" aria-labelledby="{id}-location">
			<h2 id="{id}-location" class="text-sm font-semibold text-fg">Location</h2>
			<Card.Root size="sm" class="gap-0 px-0 py-0">
				<button type="button" class="flex min-h-14 w-full items-center gap-3 rounded-md px-4 py-2 text-left outline-none hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset" onclick={() => onlocation?.()}>
					<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Location</span><span class="text-xs text-muted">Places, nudges, pause tracking and visit history.</span></span>
					<ChevronRight class="size-4 shrink-0 text-muted" aria-hidden="true" />
				</button>
			</Card.Root>
		</section>
	{/if}

	<section class="flex flex-col gap-3" aria-labelledby="{id}-model">
		<h2 id="{id}-model" class="text-sm font-semibold text-fg">Model provider</h2>
		<Card.Root size="sm" class="gap-4 px-4">
			<Field label="Provider" labelId="{id}-provider">
				<Select.Root type="single" bind:value={providerId} onValueChange={pickProvider}>
					<Select.Trigger class="h-11! w-full text-base" aria-labelledby="{id}-provider">{provider.name}</Select.Trigger>
					<Select.Content>
						{#each providerCatalog as p (p.id)}<Select.Item value={p.id} label={p.name} class="min-h-11">{p.name}</Select.Item>{/each}
					</Select.Content>
				</Select.Root>
			</Field>
			<Field label="Model" labelId="{id}-modelname">
				<Select.Root type="single" bind:value={model} onValueChange={(m) => onprovider?.(provider.id, m)}>
					<Select.Trigger class="h-11! w-full text-base" aria-labelledby="{id}-modelname">{model}</Select.Trigger>
					<Select.Content>
						{#each provider.models as m (m)}<Select.Item value={m} label={m} class="min-h-11">{m}</Select.Item>{/each}
					</Select.Content>
				</Select.Root>
			</Field>
			<Field label="API key" id="{id}-key" detail={provider.last4 ? `Key saved, ends ${provider.last4}` : provider.keySet ? 'Key saved' : 'No key yet'}>
				<Input id="{id}-key" type="password" autocomplete="off" placeholder={provider.keySet ? 'Saved. Enter a new key to replace it' : 'Paste your key'} class={input} bind:value={key} aria-describedby="{id}-keyhelp" />
				<p id="{id}-keyhelp" class="text-xs text-muted">Saved on the server, never on this device.</p>
			</Field>
			<div class="flex flex-wrap items-center gap-3">
				<Button variant="outline" size="touch" class="@3xl:h-8" onclick={test}>Test connection</Button>
				{#if status}<p class="text-sm text-muted" role="status">{status}</p>{/if}
			</div>
		</Card.Root>
	</section>

	{#if oncoaches}
		<section class="flex flex-col gap-3" aria-labelledby="{id}-coaches">
			<h2 id="{id}-coaches" class="text-sm font-semibold text-fg">Coaches</h2>
			<Card.Root size="sm" class="gap-0 px-0 py-0">
				<button type="button" class="flex min-h-14 w-full items-center gap-3 rounded-md px-4 py-2 text-left outline-none hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset" onclick={() => oncoaches?.()}>
					<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Coach library</span><span class="text-xs text-muted">See every coach by team. Copy one to change its name, style and rules.</span></span>
					<ChevronRight class="size-4 shrink-0 text-muted" aria-hidden="true" />
				</button>
			</Card.Root>
		</section>
	{/if}

	<section class="flex flex-col gap-3" aria-labelledby="{id}-search">
		<div class="flex items-center justify-between">
			<h2 id="{id}-search" class="text-sm font-semibold text-fg">Search providers</h2>
			<Button variant="outline" size="touch" class="@3xl:h-8"><Plus />Add</Button>
		</div>
		{#if settings.search.length}
			<Card.Root size="sm" class="gap-0 divide-y divide-border px-4 py-0">
				{#each settings.search as s, i (s.name)}
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
		{:else}
			<p class="text-sm text-muted">No search providers yet.</p>
		{/if}
	</section>

	<section class="flex flex-col gap-3" aria-labelledby="{id}-server">
		<h2 id="{id}-server" class="text-sm font-semibold text-fg">Server and budget</h2>
		<Card.Root size="sm" class="gap-4 px-4">
			<Field label="Server URL" id="{id}-url">
				<Input id="{id}-url" type="url" inputmode="url" placeholder="https://aroh-engine.example.com" value={settings.serverUrl} onchange={(e) => onsetting?.('server_url', e.currentTarget.value.trim())} class="{input} text-lg" />
			</Field>
			<div class="grid gap-4 @md:grid-cols-2">
				<Field label="Monthly budget, all goals" id="{id}-budget">
					<Input id="{id}-budget" type="number" inputmode="decimal" value={settings.budgetAll ?? ''} onchange={(e) => onbudget?.('all', dollars(e.currentTarget.value))} class={input} />
				</Field>
				<Field label="Monthly budget per goal" id="{id}-goalbudget">
					<Input id="{id}-goalbudget" type="number" inputmode="decimal" value={settings.budgetGoal ?? ''} onchange={(e) => onbudget?.('goal', dollars(e.currentTarget.value))} class={input} />
				</Field>
			</div>
			<p class="text-xs text-muted">In US dollars. Agents pause and ask when a budget is reached.</p>
			{#if onrunlimit}
				<Field label="Time limit per run, minutes" id="{id}-runlimit">
					<Input id="{id}-runlimit" type="number" inputmode="numeric" min="1" max="120" value={runLimit ?? 5} onchange={(e) => minutes(e.currentTarget.value)} class={input} />
				</Field>
				<p class="text-xs text-muted">Counts only time agents work, never time waiting for you. A run stops at the limit; retry it from its chat.</p>
			{/if}
		</Card.Root>
	</section>

	{#if agents}
		<section class="flex flex-col gap-3" aria-labelledby="{id}-agents">
			<h2 id="{id}-agents" class="text-sm font-semibold text-fg">Agents</h2>
			<p class="text-xs text-muted">What new goals start with. Each goal can change them in its agents sheet.</p>
			{@render agents()}
		</section>
	{/if}

	<section class="flex flex-col gap-3" aria-labelledby="{id}-access">
		<h2 id="{id}-access" class="text-sm font-semibold text-fg">Access</h2>
		<Card.Root size="sm" class="gap-0 divide-y divide-border px-4 py-0">
			<label class="flex min-h-14 cursor-pointer items-center gap-3 py-2" for="{id}-cal">
				<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Calendars</span><span class="text-xs text-muted">Read device calendars as busy time. Nothing is written back.</span></span>
				<Switch id="{id}-cal" bind:checked={calendars} onCheckedChange={(v) => onsetting?.('calendars', String(v))} />
			</label>
			<label class="flex min-h-14 cursor-pointer items-center gap-3 py-2" for="{id}-health">
				<span class="flex flex-1 flex-col"><span class="text-sm font-medium">Apple Health</span><span class="text-xs text-muted">{healthStatus ?? 'Import weight, workouts and steps on iPhone.'}</span></span>
				<Switch id="{id}-health" bind:checked={health} onCheckedChange={(v) => void toggleHealth(v)} />
			</label>
		</Card.Root>
	</section>
</div>
