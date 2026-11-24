<script lang="ts">
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import KeyRound from '@lucide/svelte/icons/key-round';
	import { Button } from '$lib/ui/primitives/button';
	import { Input } from '$lib/ui/primitives/input';
	import { Switch } from '$lib/ui/primitives/switch';
	import Field from '$lib/ui/components/Field.svelte';
	import { settingsSeed as s } from './data';

	/**
	 * Regrouped Settings (Q72): `page` 'root' is the grouped list of Account, Agents and coaches,
	 * Claude key and budget, Calendars and reminders, Location, Privacy and what Aroh knows, and
	 * About; 'key' is the Claude key and budget sub-page that the back bar returns from.
	 */
	let { page }: { page: 'root' | 'key' } = $props();

	type Row = { label: string; detail?: string; value?: string; go?: boolean; toggle?: boolean; danger?: boolean };
	const SECTIONS: { title: string; note?: string; rows: Row[] }[] = [
		{ title: 'Account', rows: [{ label: s.email, detail: `Signed in with ${s.signIn}` }, { label: 'Sign out' }, { label: 'Delete account', detail: 'Removes your goals, plans and key from the server.', danger: true }] },
		{
			title: 'Agents and coaches',
			rows: [
				{ label: 'Defaults for new goals', detail: `${s.autonomy} · reminders on · calendars on`, go: true },
				{ label: 'Run time limit', value: `${s.runLimit} min`, go: true },
				{ label: 'Coach library', value: `${s.coaches} coaches`, go: true }
			]
		},
		{
			title: 'Claude key and budget',
			rows: [
				{ label: 'Claude key', value: `${s.key.vendor} ····${s.key.last4}`, go: true },
				{ label: 'Monthly budget', detail: s.spent, value: `$${s.budgetAll} all · $${s.budgetGoal} per goal`, go: true }
			]
		},
		{
			title: 'Calendars and reminders',
			rows: [
				{ label: 'Use my calendars', detail: 'Busy time only. Nothing is written back.', toggle: true },
				{ label: 'Notifications', value: s.notifications, go: true },
				{ label: 'Apple Health', detail: 'Weight, workouts and steps.', toggle: true }
			]
		},
		{ title: 'Location', rows: [{ label: 'Location', value: s.location, go: true }] },
		{
			title: 'Privacy and what Aroh knows',
			rows: [
				{ label: 'What Aroh knows about you', value: `${s.facts} facts`, go: true },
				{ label: 'Location history', detail: 'Raw visits stay on this phone, 90 days.', go: true }
			]
		},
		{
			title: 'About',
			rows: [
				{ label: 'Server', detail: s.server },
				{ label: 'Version', value: s.version },
				{ label: 'Privacy policy', go: true },
				{ label: 'Open-source licences', go: true }
			]
		}
	];
	const ON: Record<string, boolean> = { 'Use my calendars': s.calendars, 'Apple Health': s.health };
	const row = 'flex min-h-13 w-full items-center gap-3 px-4 py-2 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset';
</script>

{#if page === 'root'}
	<div class="flex flex-col gap-6 p-4">
		{#each SECTIONS as sec (sec.title)}
			<section class="flex flex-col gap-2" aria-label={sec.title}>
				<h2 class="px-1 text-sm font-semibold">{sec.title}</h2>
				<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
					{#each sec.rows as r (r.label)}
						<li>
							{#if r.toggle}
								<label class="{row} cursor-pointer">
									<span class="flex min-w-0 flex-1 flex-col"><span class="text-md">{r.label}</span>{#if r.detail}<span class="text-sm text-muted">{r.detail}</span>{/if}</span>
									<Switch checked={ON[r.label]} />
								</label>
							{:else}
								<button type="button" class={row}>
									<span class="flex min-w-0 flex-1 flex-col"><span class="truncate text-md {r.danger ? 'text-danger' : ''}">{r.label}</span>{#if r.detail}<span class="text-sm text-muted">{r.detail}</span>{/if}</span>
									{#if r.value}<span class="max-w-[50%] shrink-0 truncate text-right text-sm text-muted tabular-nums">{r.value}</span>{/if}
									{#if r.go}<ChevronRight class="size-4 shrink-0 text-muted" aria-hidden="true" />{/if}
								</button>
							{/if}
						</li>
					{/each}
				</ul>
			</section>
		{/each}
	</div>
{:else}
	<div class="flex flex-col gap-6 p-4">
		<section class="flex flex-col gap-2" aria-label="Claude key">
			<h2 class="px-1 text-sm font-semibold">Claude key</h2>
			<div class="flex flex-col gap-3 rounded-sm border border-border bg-bg p-4">
				<div class="flex items-center gap-3">
					<span class="flex size-9 items-center justify-center rounded-sm bg-surface-2" aria-hidden="true"><KeyRound class="size-4" /></span>
					<span class="flex flex-1 flex-col"><span class="text-md font-medium">{s.key.vendor}</span><span class="text-sm text-muted tabular-nums">Key ending {s.key.last4} · works</span></span>
				</div>
				<p class="text-sm text-muted">Stored encrypted on the server. This phone never keeps it, and only the vendor and last 4 characters are shown.</p>
				<Field label="Replace key" id="st-key"><Input id="st-key" type="password" class="h-11 text-lg" placeholder="Paste a new key" /></Field>
				<div class="flex gap-2"><Button variant="outline" size="touch">Test connection</Button><Button variant="ghost" size="touch" class="text-danger">Remove key</Button></div>
			</div>
		</section>
		<section class="flex flex-col gap-2" aria-label="Model">
			<h2 class="px-1 text-sm font-semibold">Model</h2>
			<ul class="flex flex-col divide-y divide-border rounded-sm border border-border bg-bg">
				{#each ['claude-sonnet-5-5', 'claude-opus-5-5', 'claude-haiku-5-5'] as m (m)}
					<li class="flex min-h-13 items-center gap-3 px-4">
						<span class="flex size-4 shrink-0 items-center justify-center rounded-full border {m === s.key.model ? 'border-fg' : 'border-border-strong'}" aria-hidden="true">{#if m === s.key.model}<span class="size-2 rounded-full bg-fg"></span>{/if}</span>
						<span class="text-md">{m}</span>
					</li>
				{/each}
			</ul>
		</section>
		<section class="flex flex-col gap-2" aria-label="Monthly budget">
			<h2 class="px-1 text-sm font-semibold">Monthly budget</h2>
			<div class="flex flex-col gap-3 rounded-sm border border-border bg-bg p-4">
				<div class="flex flex-col gap-1.5">
					<p class="text-md font-semibold tabular-nums">{s.spent}</p>
					<div class="h-1.5 overflow-hidden rounded-full bg-surface-2"><div class="h-full w-[17%] rounded-full bg-fg"></div></div>
				</div>
				<div class="grid grid-cols-2 gap-3">
					<Field label="All goals, $" id="st-all"><Input id="st-all" inputmode="decimal" class="h-11 text-lg" value={s.budgetAll} /></Field>
					<Field label="Per goal, $" id="st-goal"><Input id="st-goal" inputmode="decimal" class="h-11 text-lg" value={s.budgetGoal} /></Field>
				</div>
				<p class="text-xs text-muted">Agents pause and ask when a budget is reached.</p>
			</div>
		</section>
	</div>
{/if}
