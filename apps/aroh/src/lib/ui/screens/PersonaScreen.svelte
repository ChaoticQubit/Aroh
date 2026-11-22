<script lang="ts">
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import Lock from '@lucide/svelte/icons/lock';
	import Plus from '@lucide/svelte/icons/plus';
	import X from '@lucide/svelte/icons/x';
	import { Badge } from '$lib/ui/primitives/badge';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import { Input } from '$lib/ui/primitives/input';
	import { Textarea } from '$lib/ui/primitives/textarea';
	import Field from '../components/Field.svelte';
	import type { CoachFields, CoachView } from './types';

	/**
	 * One coach. A built-in shows its name, style, instructions and rules read-only with Copy to edit;
	 * the owner's copy edits them and saves through `onsave`. Locked safety rules show with a lock and
	 * no remove control and are always sent unchanged. `status` is the line under the actions.
	 */
	let {
		persona,
		status = null,
		busy = false,
		oncopy,
		onsave,
		onback
	}: {
		persona: CoachView;
		status?: string | null;
		busy?: boolean;
		oncopy?: () => void;
		onsave?: (fields: CoachFields) => void;
		onback?: () => void;
	} = $props();

	const id = $props.id();
	const editable = $derived(persona.source === 'library');
	let displayName = $derived(persona.display_name);
	let role = $derived(persona.role);
	let instructions = $derived(persona.instructions);
	let rules = $derived([...persona.guardrails]);
	let draft = $state('');

	/** Appends the typed rule when it is not blank and not already listed. */
	function addRule() {
		const rule = draft.trim();
		if (rule && !rules.includes(rule)) rules = [...rules, rule];
		draft = '';
	}

	/** Removes the rule at `index`; locked safety rules have no remove control. */
	function removeRule(index: number) {
		rules = rules.filter((_, i) => i !== index);
	}

	/** Reports the edited fields; locked rules missing from the list are put back first. */
	function save() {
		const kept = [...rules, ...persona.locked.filter((r) => !rules.includes(r))];
		onsave?.({ display_name: displayName.trim(), role: role.trim(), instructions, guardrails: kept });
	}
</script>

<div class="flex w-full max-w-2xl flex-col gap-5 p-4 @3xl:p-6">
	{#if onback}
		<button type="button" class="-ml-2 flex h-11 w-fit items-center gap-1 rounded-sm px-2 text-sm text-muted outline-none hover:text-fg focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => onback?.()}>
			<ChevronLeft class="size-4" aria-hidden="true" />Coaches
		</button>
	{/if}
	<div class="flex flex-col gap-1">
		<div class="flex items-center gap-2">
			<h2 class="text-xl font-semibold">{persona.display_name}</h2>
			<Badge variant="outline">{editable ? 'Your copy' : 'Built-in'}</Badge>
		</div>
		<p class="text-xs text-muted">{persona.name}</p>
	</div>

	{#if editable}
		<Card.Root size="sm" class="gap-4 px-4">
			<Field label="Name" id="{id}-name">
				<Input id="{id}-name" class="h-11 text-base" bind:value={displayName} />
			</Field>
			<Field label="Style" id="{id}-role">
				<Textarea id="{id}-role" class="text-sm" bind:value={role} />
			</Field>
			<Field label="Instructions" id="{id}-instructions">
				<Textarea id="{id}-instructions" class="min-h-40 text-sm" bind:value={instructions} />
			</Field>
		</Card.Root>
	{:else}
		<Card.Root size="sm" class="gap-0 divide-y divide-border px-4 py-0">
			<div class="flex flex-col gap-1 py-3"><p class="text-xs font-medium text-muted">Style</p><p class="text-sm">{persona.role}</p></div>
			<div class="flex flex-col gap-1 py-3"><p class="text-xs font-medium text-muted">Instructions</p><p class="text-sm whitespace-pre-line">{persona.instructions}</p></div>
		</Card.Root>
	{/if}

	<section class="flex flex-col gap-2" aria-labelledby="{id}-rules">
		<h3 id="{id}-rules" class="text-sm font-semibold text-fg">Rules</h3>
		<ul class="divide-y divide-border overflow-hidden rounded-md border border-border bg-bg">
			{#each rules as rule, i (i)}
				{@const locked = persona.locked.includes(rule)}
				<li class="flex min-h-12 items-center gap-3 py-1 pr-1 pl-4">
					<span class="flex-1 py-2 text-sm">{rule}</span>
					{#if locked}
						<span class="flex size-11 shrink-0 items-center justify-center text-muted"><Lock class="size-4" aria-label="Locked safety rule" /></span>
					{:else if editable}
						<Button variant="ghost" size="icon-touch" aria-label="Remove rule: {rule}" onclick={() => removeRule(i)}><X /></Button>
					{/if}
				</li>
			{/each}
		</ul>
		{#if persona.locked.length}<p class="text-xs text-muted">Safety rules stay on every coach and cannot be removed.</p>{/if}
		{#if editable}
			<div class="flex items-center gap-2">
				<Input aria-label="New rule" placeholder="Add a rule" class="h-11 flex-1 text-sm" bind:value={draft} onkeydown={(e) => e.key === 'Enter' && addRule()} />
				<Button variant="outline" size="touch" class="@3xl:h-8" onclick={addRule}><Plus />Add</Button>
			</div>
		{/if}
	</section>

	<div class="flex flex-wrap items-center gap-3">
		{#if editable}
			<Button size="touch" class="@3xl:h-8" disabled={busy} onclick={save}>Save</Button>
		{:else}
			<Button size="touch" class="@3xl:h-8" disabled={busy} onclick={() => oncopy?.()}>Copy to edit</Button>
		{/if}
		{#if status}<p class="text-sm text-muted" role="status">{status}</p>{/if}
	</div>
</div>
