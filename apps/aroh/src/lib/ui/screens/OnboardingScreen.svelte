<script lang="ts" module>
	/**
	 * What onboarding asks its host to do. Each sign-in, key, consent and server call resolves to
	 * an owner-readable error, or null on success; `onemail` resolves to `{ checkEmail }` instead
	 * when a new account must confirm its address first; `onserver` with empty values returns to the
	 * hosted server. `onallow` resolves to true once the permission is granted; `ondone` closes
	 * onboarding with the typed first goal, if any.
	 */
	export type OnboardingActions = {
		onapple: () => Promise<string | null>;
		onemail: (email: string, password: string, create: boolean) => Promise<string | null | { checkEmail: string }>;
		onserver: (supabaseUrl: string, anonKey: string, serverUrl: string) => Promise<string | null>;
		onkey: (key: string) => Promise<string | null>;
		onconsent: () => Promise<string | null>;
		onallow: (id: Permission['id']) => Promise<boolean>;
		ondone: (goal: string | null) => void;
	};
</script>

<script lang="ts">
	import Apple from '@lucide/svelte/icons/apple';
	import Bell from '@lucide/svelte/icons/bell';
	import CalendarDays from '@lucide/svelte/icons/calendar-days';
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import HeartPulse from '@lucide/svelte/icons/heart-pulse';
	import Mail from '@lucide/svelte/icons/mail';
	import MapPin from '@lucide/svelte/icons/map-pin';
	import Server from '@lucide/svelte/icons/server';
	import { untrack } from 'svelte';
	import Logo from '../brand/Logo.svelte';
	import { Button } from '$lib/ui/primitives/button';
	import { Input } from '$lib/ui/primitives/input';
	import Field from '../components/Field.svelte';
	import { nextStep, PERMISSIONS, stepCount, type OnboardingStep, type Permission } from '$lib/app/setup';

	/**
	 * First-run onboarding (Q72, Q76): Welcome, Sign in (Apple, email, or the owner's own server),
	 * Claude key, AI consent, permissions and a first goal, one screen at a time with a step
	 * count and a back arrow. `start` is where it opens; `only` closes it after that one screen.
	 * `noServer` (a build without server defaults) asks for the owner's server before sign in.
	 */
	let { start, signedIn: signedInAtStart = false, apple = true, noServer = false, actions }: { start: { step: OnboardingStep; only: boolean }; signedIn?: boolean; apple?: boolean; noServer?: boolean; actions: OnboardingActions } = $props();

	const ICONS = { calendar: CalendarDays, notifications: Bell, location: MapPin, health: HeartPulse };
	const EXAMPLES = ['Read 12 books this year', 'Run a half marathon by March', 'Save 3,000 for a trip'];
	const pad = 'px-6 pt-[calc(env(safe-area-inset-top)+1.5rem)] pb-[calc(env(safe-area-inset-bottom)+1.5rem)]';
	const input = 'h-11 text-lg';

	let trail = $state<OnboardingStep[]>(untrack(() => [start.step]));
	let signedIn = $state(untrack(() => signedInAtStart));
	let busy = $state(false);
	let error = $state<string | null>(null);
	let email = $state('');
	let checkEmail = $state<string | null>(null);
	let password = $state('');
	let supabaseUrl = $state('');
	let anonKey = $state('');
	let serverUrl = $state('');
	let key = $state('');
	let goal = $state('');
	let allowed = $state<Partial<Record<Permission['id'], boolean>>>({});
	const step = $derived(trail[trail.length - 1]);
	const count = $derived(start.only ? null : stepCount(step));

	/** Opens `to` on top of the trail and clears the last error. */
	function open(to: OnboardingStep) {
		error = null;
		trail = [...trail, to];
	}

	/** Goes one screen back; never past the screen onboarding opened on. */
	function back() {
		error = null;
		if (trail.length > 1) trail = trail.slice(0, -1);
	}

	/** Moves past the current screen, or finishes onboarding after the last. */
	function next() {
		const to = nextStep(step, { signedIn, only: start.only, noServer });
		if (to) open(to);
		else actions.ondone(goal.trim() || null);
	}

	/** Runs one host call with the busy state; on success marks sign-in when asked, then moves on. */
	async function attempt(call: () => Promise<string | null>, signs = false) {
		if (busy) return;
		busy = true;
		error = null;
		try {
			error = await call();
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
		if (error) return;
		if (signs) signedIn = true;
		password = '';
		key = '';
		next();
	}

	/**
	 * Signs in, or with `create` signs up, by email. A sign-up that must confirm the address
	 * first shows the check-your-email state instead of moving on; anything else goes as `attempt`.
	 */
	async function emailAuth(create: boolean) {
		let sentTo: string | null = null;
		await attempt(async () => {
			const r = await actions.onemail(email.trim(), password, create);
			if (r === null || typeof r === 'string') return r;
			sentTo = r.checkEmail;
			return 'check email';
		}, true);
		if (sentTo === null) return;
		error = null;
		password = '';
		checkEmail = sentTo;
	}

	/** Asks the OS for one permission and marks it when granted. */
	async function allow(id: Permission['id']) {
		const granted = await actions.onallow(id).catch(() => false);
		allowed = { ...allowed, [id]: granted };
	}
</script>

{#snippet top()}
	<div class="flex h-11 items-center justify-between">
		{#if trail.length > 1}
			<button type="button" class="-ml-2 flex min-h-11 items-center gap-0.5 rounded-sm pr-2 text-base outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={back}><ChevronLeft class="size-5" aria-hidden="true" />Back</button>
		{:else}<span></span>{/if}
		{#if count}<p class="text-sm text-muted tabular-nums" aria-live="polite">Step {count.at} of {count.of}</p>{/if}
	</div>
{/snippet}

{#snippet heading(title: string, reason: string)}
	<div class="flex flex-col gap-3 pt-4">
		<h1 class="text-2xl leading-tight font-semibold">{title}</h1>
		<p class="text-lg text-muted">{reason}</p>
	</div>
{/snippet}

{#snippet failed()}
	{#if error}<p role="alert" class="text-sm text-status-missed">{error}</p>{/if}
{/snippet}

<div class="flex h-full flex-col overflow-y-auto bg-bg text-fg {pad}">
	{#if step === 'welcome'}
		<div class="flex flex-1 flex-col justify-center gap-5">
			<Logo size={56} />
			<div class="flex flex-col gap-3">
				<h1 class="text-2xl leading-tight font-semibold">Goals that plan themselves around your week</h1>
				<p class="text-lg text-muted">Tell Aroh what you want to reach. Agents research it, ask what they need and put sessions on your calendar. You approve every plan.</p>
			</div>
			<ul class="flex flex-col divide-y divide-border rounded-sm border border-border">
				{#each [['Plans around your calendar', 'Busy time and fixed events stay untouched.'], ['Replans when life happens', 'Missed a session? It moves, you get undo.'], ['Your data, your key', 'Your Claude key is stored encrypted on your account.']] as [t, d] (t)}
					<li class="flex flex-col px-4 py-3"><span class="text-sm font-medium">{t}</span><span class="text-sm text-muted">{d}</span></li>
				{/each}
			</ul>
		</div>
		<div class="flex flex-col gap-3 pt-6">
			<Button class="h-12 text-lg" onclick={next}>Get started</Button>
		</div>
	{:else if step === 'signin'}
		{@render top()}
		<Logo size={48} class="mt-4" />
		{@render heading('Sign in or create your account', 'Your goals sync across your devices and the agents work while the app is closed.')}
		<div class="mt-auto flex flex-col gap-3 pt-6">
			{@render failed()}
			{#if apple}<Button class="h-12 gap-2 text-lg" disabled={busy} onclick={() => attempt(actions.onapple, true)}><Apple class="size-5" aria-hidden="true" />Continue with Apple</Button>{/if}
			<Button variant="outline" class="h-12 gap-2 text-lg" onclick={() => open('email')}><Mail class="size-5" aria-hidden="true" />Continue with email</Button>
			<Button variant="ghost" class="h-12 text-lg" onclick={next}>Not now</Button>
			<p class="text-center text-sm text-muted">Without an account, goals and the calendar work on this phone; agents need an account.</p>
			<button type="button" class="mx-auto flex min-h-11 items-center gap-1.5 rounded-sm px-2 text-sm text-muted underline underline-offset-4 outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => open('server')}><Server class="size-4" aria-hidden="true" />Use my own server</button>
		</div>
	{:else if step === 'email'}
		{@render top()}
		{#if checkEmail}
		{@render heading('Check your email', `We sent a confirmation link to ${checkEmail}. Open it, then come back and sign in.`)}
		<div class="mt-6 flex flex-col gap-3">
			<Button class="h-12 gap-2 text-lg" onclick={() => (checkEmail = null)}><Mail class="size-5" aria-hidden="true" />I confirmed it, sign in</Button>
		</div>
		{:else}
		{@render heading('Continue with email', 'Sign in, or create an account with a new email.')}
		<form class="mt-6 flex flex-col gap-4" onsubmit={(e) => (e.preventDefault(), emailAuth(false))}>
			<Field label="Email" id="ob-email"><Input id="ob-email" type="email" inputmode="email" autocomplete="username" class={input} bind:value={email} /></Field>
			<Field label="Password" id="ob-password"><Input id="ob-password" type="password" autocomplete="current-password" class={input} bind:value={password} /></Field>
			{@render failed()}
			<Button type="submit" class="h-12 text-lg" disabled={busy || !email.trim() || !password}>Sign in</Button>
			<Button variant="outline" class="h-12 text-lg" disabled={busy || !email.trim() || !password} onclick={() => emailAuth(true)}>Create account</Button>
		</form>
		{/if}
	{:else if step === 'server'}
		{@render top()}
		{@render heading('Your own server', noServer ? 'This build has no server set. Enter your own to sign in and run agents.' : 'For self-hosting. Most people use the Aroh hosted server.')}
		<div class="mt-6 flex flex-col gap-4 rounded-sm border border-border bg-surface p-4">
			<Field label="Supabase project URL" id="ob-sb"><Input id="ob-sb" type="url" inputmode="url" autocomplete="off" placeholder="https://your-project.supabase.co" class={input} bind:value={supabaseUrl} /></Field>
			<Field label="Supabase anon key" id="ob-anon"><Input id="ob-anon" autocomplete="off" class={input} bind:value={anonKey} /></Field>
			<Field label="Aroh server URL" id="ob-srv"><Input id="ob-srv" type="url" inputmode="url" autocomplete="off" placeholder="https://aroh.example.com" class={input} bind:value={serverUrl} /></Field>
			<p class="text-xs text-muted">To switch later, sign out and choose Use my own server again.</p>
		</div>
		<div class="mt-auto flex flex-col gap-3 pt-6">
			{@render failed()}
			<Button class="h-12 text-lg" disabled={busy || !supabaseUrl.trim() || !anonKey.trim() || !serverUrl.trim()} onclick={() => attempt(() => actions.onserver(supabaseUrl.trim(), anonKey.trim(), serverUrl.trim()))}>Save and continue</Button>
			{#if noServer}
				<Button variant="ghost" class="h-12 text-lg" disabled={busy} onclick={() => open('permissions')}>Not now</Button>
				<p class="text-center text-sm text-muted">Without a server, goals and the calendar work on this phone; agents need one.</p>
			{:else}
				<Button variant="ghost" class="h-12 text-lg" disabled={busy} onclick={() => attempt(() => actions.onserver('', '', ''))}>Use the hosted server</Button>
			{/if}
		</div>
	{:else if step === 'key'}
		{@render top()}
		{@render heading('Add your Claude key', 'The agents run on your own Claude account, so you only pay Anthropic for what they use.')}
		<div class="mt-6 flex flex-col gap-3 rounded-sm border border-border bg-bg p-4">
			<Field label="Claude API key" id="ob-key"><Input id="ob-key" type="password" autocomplete="off" placeholder="sk-ant-..." class={input} bind:value={key} /></Field>
			<ul class="flex flex-col gap-1.5 text-sm text-muted">
				<li>Stored encrypted on the server, never on this phone.</li>
				<li>After saving, only the vendor and the last 4 characters show.</li>
				<li>Set a monthly budget later in Settings.</li>
			</ul>
		</div>
		<div class="mt-auto flex flex-col gap-3 pt-6">
			{@render failed()}
			<Button class="h-12 text-lg" disabled={busy || !key.trim()} onclick={() => attempt(() => actions.onkey(key.trim()))}>{busy ? 'Checking the key' : 'Save key'}</Button>
			<Button variant="ghost" class="h-12 text-lg" onclick={next}>Add later</Button>
			<p class="text-center text-xs text-muted">Without a key the agents wait. Goals and the calendar still work.</p>
		</div>
	{:else if step === 'consent'}
		{@render top()}
		{@render heading('Your goals and the AI', 'Agents send what they need to Anthropic, using your own Claude key, to research and plan your goals.')}
		<ul class="mt-6 flex flex-col divide-y divide-border rounded-sm border border-border">
			{#each [['What is sent', 'The goal, your answers and files, your logs, your planned sessions and busy times for the planning window.'], ['Who receives it', 'Anthropic, through your own API key and under its terms. Web searches go through Anthropic web tools.'], ['Where to read it again', 'Settings, Privacy and what Aroh knows.']] as [t, d] (t)}
				<li class="flex flex-col px-4 py-3"><span class="text-sm font-medium">{t}</span><span class="text-sm text-muted">{d}</span></li>
			{/each}
		</ul>
		<div class="mt-auto flex flex-col gap-3 pt-6">
			{@render failed()}
			<Button class="h-12 text-lg" disabled={busy} onclick={() => attempt(actions.onconsent)}>I agree</Button>
			<p class="text-center text-xs text-muted">Agents do not run without this.</p>
		</div>
	{:else if step === 'permissions'}
		{@render top()}
		{@render heading('What Aroh may use', 'Each one is optional. Change them any time in Settings.')}
		<ul class="mt-6 flex flex-col divide-y divide-border rounded-sm border border-border">
			{#each PERMISSIONS as p (p.id)}
				{@const Icon = ICONS[p.id]}
				<li class="flex items-start gap-3 px-4 py-3">
					<Icon class="mt-0.5 size-5 shrink-0" aria-hidden="true" />
					<span class="flex min-w-0 flex-1 flex-col gap-0.5"><span class="text-base font-medium">{p.title}</span><span class="text-sm text-muted">{p.why}</span></span>
					{#if allowed[p.id]}<span class="mt-0.5 shrink-0 text-sm font-medium text-status-done">Allowed</span>{:else}<Button variant="outline" size="sm" class="h-11 shrink-0" onclick={() => allow(p.id)}>Allow</Button>{/if}
				</li>
			{/each}
		</ul>
		<div class="mt-auto flex flex-col gap-3 pt-6">
			<Button class="h-12 text-lg" onclick={next}>Continue</Button>
		</div>
	{:else}
		{@render top()}
		{@render heading('Your first goal', 'One line is enough. Agents research it and ask what they cannot find.')}
		<div class="mt-6 flex flex-col gap-3">
			<Field label="What do you want to reach?" id="ob-goal"><Input id="ob-goal" class="h-12 text-lg" bind:value={goal} /></Field>
			<p class="text-xs font-medium text-muted">Or start from an example</p>
			<ul class="flex flex-wrap gap-2">
				{#each EXAMPLES as e (e)}<li><button type="button" class="min-h-11 rounded-sm border border-border px-3 text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => (goal = e)}>{e}</button></li>{/each}
			</ul>
			<p class="text-sm text-muted">Date, category, files and agent controls come on the next screen.</p>
		</div>
		<div class="mt-auto flex flex-col gap-3 pt-6">
			<Button class="h-12 text-lg" disabled={!goal.trim()} onclick={next}>Continue</Button>
			<Button variant="ghost" class="h-12 text-lg" onclick={() => ((goal = ''), next())}>Skip, show me the app</Button>
		</div>
	{/if}
</div>
