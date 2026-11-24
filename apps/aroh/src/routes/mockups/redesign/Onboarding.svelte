<script lang="ts">
	import Apple from '@lucide/svelte/icons/apple';
	import Bell from '@lucide/svelte/icons/bell';
	import CalendarDays from '@lucide/svelte/icons/calendar-days';
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import HeartPulse from '@lucide/svelte/icons/heart-pulse';
	import KeyRound from '@lucide/svelte/icons/key-round';
	import Mail from '@lucide/svelte/icons/mail';
	import MapPin from '@lucide/svelte/icons/map-pin';
	import Server from '@lucide/svelte/icons/server';
	import Target from '@lucide/svelte/icons/target';
	import { Button } from '$lib/ui/primitives/button';
	import { Input } from '$lib/ui/primitives/input';
	import Field from '$lib/ui/components/Field.svelte';

	/**
	 * First-run onboarding (Q72), one screen per `step`: welcome, sign in, own server, Claude key,
	 * permissions and first goal. Each screen keeps the pod 30 language: a step count, a 24 px
	 * title, a muted reason, cards with 1 px borders and one black primary button at the bottom.
	 */
	let { step }: { step: 'welcome' | 'signin' | 'server' | 'key' | 'permissions' | 'goal' } = $props();

	const ORDER = ['signin', 'key', 'permissions', 'goal'] as const;
	const at = $derived(ORDER.indexOf(step === 'server' ? 'signin' : (step as (typeof ORDER)[number])));
	const PERMS = [
		{ icon: CalendarDays, title: 'Calendar', why: 'Plans go around your meetings. Aroh only reads busy times and never writes to your calendars.', state: 'Allowed' },
		{ icon: Bell, title: 'Notifications', why: 'A reminder before each session and a note when a plan changes.', state: 'Allow' },
		{ icon: MapPin, title: 'Location', why: 'Nudges near places you name, like the gym. Visits stay on this phone.', state: 'Allow' },
		{ icon: HeartPulse, title: 'Apple Health', why: 'Imports weight, workouts and steps so you log less by hand.', state: 'Allow' }
	];
	const EXAMPLES = ['Read 12 books this year', 'Run a half marathon by March', 'Save 3,000 for a trip'];
	const pad = 'px-6 pt-[calc(env(safe-area-inset-top)+1.5rem)] pb-[calc(env(safe-area-inset-bottom)+1.5rem)]';
</script>

{#snippet top(back: boolean)}
	<div class="flex h-11 items-center justify-between">
		{#if back}<button type="button" class="-ml-2 flex min-h-11 items-center gap-0.5 rounded-sm pr-2 text-md outline-none focus-visible:ring-3 focus-visible:ring-ring/50"><ChevronLeft class="size-5" aria-hidden="true" />Back</button>{:else}<span></span>{/if}
		{#if at >= 0}<p class="text-sm text-muted tabular-nums" aria-live="polite">Step {at + 1} of {ORDER.length}</p>{/if}
	</div>
{/snippet}

<div class="flex h-full flex-col overflow-y-auto bg-bg text-fg {pad}">
	{#if step === 'welcome'}
		<div class="flex flex-1 flex-col justify-center gap-5">
			<span class="flex size-14 items-center justify-center rounded-sm bg-fg text-bg" aria-hidden="true"><Target class="size-7" /></span>
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
			<Button class="h-12 text-lg">Get started</Button>
			<Button variant="ghost" class="h-12 text-lg">I already have an account</Button>
		</div>
	{:else if step === 'signin' || step === 'server'}
		{@render top(true)}
		<div class="flex flex-col gap-3 pt-4">
			<h1 class="text-2xl leading-tight font-semibold">Create your account</h1>
			<p class="text-lg text-muted">Your goals sync across your devices and the agents work while the app is closed.</p>
		</div>
		{#if step === 'signin'}
			<div class="mt-auto flex flex-col gap-3 pt-6">
				<Button class="h-12 gap-2 text-lg"><Apple class="size-5" aria-hidden="true" />Continue with Apple</Button>
				<Button variant="outline" class="h-12 gap-2 text-lg"><Mail class="size-5" aria-hidden="true" />Continue with email</Button>
				<p class="pt-2 text-center text-sm text-muted">Signing in creates an account if you are new.</p>
				<button type="button" class="mx-auto flex min-h-11 items-center gap-1.5 rounded-sm px-2 text-sm text-muted underline underline-offset-4 outline-none focus-visible:ring-3 focus-visible:ring-ring/50"><Server class="size-4" aria-hidden="true" />Use my own server</button>
			</div>
		{:else}
			<div class="mt-6 flex flex-col gap-4 rounded-sm border border-border bg-surface p-4">
				<div class="flex flex-col gap-1"><p class="text-md font-semibold">Your own server</p><p class="text-sm text-muted">For self-hosting. Leave this off to use the Aroh hosted server.</p></div>
				<Field label="Supabase project URL" id="ob-sb"><Input id="ob-sb" type="url" class="h-11 text-lg" value="https://xyzcompany.supabase.co" /></Field>
				<Field label="Supabase anon key" id="ob-anon"><Input id="ob-anon" class="h-11 text-lg" value="eyJhbGciOiJIUzI1NiIs..." /></Field>
				<Field label="Aroh server URL" id="ob-srv"><Input id="ob-srv" type="url" class="h-11 text-lg" value="https://aroh.my-domain.dev" /></Field>
				<p class="text-xs text-muted">To switch back later, sign out and choose the hosted server.</p>
			</div>
			<div class="mt-auto flex flex-col gap-3 pt-6">
				<Button class="h-12 text-lg">Check and continue</Button>
				<Button variant="ghost" class="h-12 text-lg">Use the hosted server</Button>
			</div>
		{/if}
	{:else if step === 'key'}
		{@render top(true)}
		<div class="flex flex-col gap-3 pt-4">
			<h1 class="text-2xl leading-tight font-semibold">Add your Claude key</h1>
			<p class="text-lg text-muted">The agents run on your own Claude account, so you only pay Anthropic for what they use.</p>
		</div>
		<div class="mt-6 flex flex-col gap-3 rounded-sm border border-border bg-bg p-4">
			<Field label="Claude API key" id="ob-key"><Input id="ob-key" type="password" class="h-11 text-lg" value="sk-ant-api03-xxxxxxxxxxxx4f2a" /></Field>
			<div class="flex items-center gap-3 rounded-sm bg-surface-2 px-3 py-2.5">
				<KeyRound class="size-4 shrink-0 text-muted" aria-hidden="true" />
				<p class="text-sm">Anthropic key ending <span class="font-medium tabular-nums">4f2a</span> works.</p>
			</div>
			<ul class="flex flex-col gap-1.5 text-sm text-muted">
				<li>Stored encrypted on the server, never on this phone.</li>
				<li>After saving, only the vendor and the last 4 characters show.</li>
				<li>Set a monthly budget later in Settings.</li>
			</ul>
		</div>
		<div class="mt-auto flex flex-col gap-3 pt-6">
			<Button class="h-12 text-lg">Save key</Button>
			<Button variant="ghost" class="h-12 text-lg">Add later</Button>
			<p class="text-center text-xs text-muted">Without a key the agents wait. Goals and the calendar still work.</p>
		</div>
	{:else if step === 'permissions'}
		{@render top(true)}
		<div class="flex flex-col gap-3 pt-4">
			<h1 class="text-2xl leading-tight font-semibold">What Aroh may use</h1>
			<p class="text-lg text-muted">Each one is optional. Change them any time in Settings.</p>
		</div>
		<ul class="mt-6 flex flex-col divide-y divide-border rounded-sm border border-border">
			{#each PERMS as p (p.title)}
				<li class="flex items-start gap-3 px-4 py-3">
					<p.icon class="mt-0.5 size-5 shrink-0" aria-hidden="true" />
					<span class="flex min-w-0 flex-1 flex-col gap-0.5"><span class="text-md font-medium">{p.title}</span><span class="text-sm text-muted">{p.why}</span></span>
					{#if p.state === 'Allowed'}<span class="mt-0.5 shrink-0 text-sm font-medium text-status-done">Allowed</span>{:else}<Button variant="outline" size="sm" class="shrink-0">Allow</Button>{/if}
				</li>
			{/each}
		</ul>
		<div class="mt-auto flex flex-col gap-3 pt-6">
			<Button class="h-12 text-lg">Continue</Button>
		</div>
	{:else}
		{@render top(true)}
		<div class="flex flex-col gap-3 pt-4">
			<h1 class="text-2xl leading-tight font-semibold">Your first goal</h1>
			<p class="text-lg text-muted">One line is enough. Agents research it and ask what they cannot find.</p>
		</div>
		<div class="mt-6 flex flex-col gap-3">
			<Field label="What do you want to reach?" id="ob-goal"><Input id="ob-goal" class="h-12 text-lg" value="Finish reading Harbor Lights" /></Field>
			<p class="text-xs font-medium text-muted">Or start from an example</p>
			<ul class="flex flex-wrap gap-2">
				{#each EXAMPLES as e (e)}<li><button type="button" class="min-h-11 rounded-sm border border-border px-3 text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50">{e}</button></li>{/each}
			</ul>
			<p class="text-sm text-muted">Date, category, files and agent controls come on the next screen.</p>
		</div>
		<div class="mt-auto flex flex-col gap-3 pt-6">
			<Button class="h-12 text-lg">Continue</Button>
			<Button variant="ghost" class="h-12 text-lg">Skip, show me the app</Button>
		</div>
	{/if}
</div>
