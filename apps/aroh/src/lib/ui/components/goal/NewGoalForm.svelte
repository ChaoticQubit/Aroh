<script lang="ts">
	import { onMount } from 'svelte';
	import Plus from '@lucide/svelte/icons/plus';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import { Input } from '$lib/ui/primitives/input';
	import { Textarea } from '$lib/ui/primitives/textarea';
	import Field from '$lib/ui/components/Field.svelte';
	import type { CategorySlot } from '$lib/ui/contract';
	import { canCreate, noEndDate, pickCategory, problem, quickPicks, withTarget, type NewGoalDraft } from '$lib/app/newgoal';
	import AgentControls from './AgentControls.svelte';
	import Attachments, { type Attachment } from './Attachments.svelte';

	/** One category the picker offers. */
	export type CategoryChoice = { id: string; name: string; slot: CategorySlot };

	/**
	 * New Goal form (Q61): title (required), description, target date on the native date picker with
	 * quick picks from `today`, the category chip (guessed by the caller, `detected`), the picker with
	 * New category and a 6-hue swatch row, attachments when `attach` is true (Photo and File only with
	 * `upload`; `onfiles`, `onlink`, `onremove`, else links and removals edit `files` in place), the
	 * agent controls, and a pinned Create. `onaddcategory(name, slot)` returns an error to show or null; `oncreate` runs Create.
	 * `adding` and `newName` open the New category field, for the mockup.
	 */
	let {
		draft = $bindable(),
		categories,
		today,
		defaultLimit = 5,
		detected = false,
		attach = false,
		upload = false,
		files = $bindable([]),
		onfiles,
		onlink,
		onretry,
		onremove,
		creating = false,
		error = null,
		adding = false,
		newName = '',
		onaddcategory,
		oncreate
	}: {
		draft: NewGoalDraft;
		categories: CategoryChoice[];
		today: string;
		defaultLimit?: number;
		detected?: boolean;
		attach?: boolean;
		upload?: boolean;
		files?: Attachment[];
		onfiles?: (files: File[]) => void;
		onlink?: (url: string) => void;
		onretry?: (id: string) => void;
		onremove?: (id: string) => void;
		creating?: boolean;
		error?: string | null;
		adding?: boolean;
		newName?: string;
		onaddcategory?: (name: string, slot: CategorySlot) => Promise<string | null>;
		oncreate?: () => void;
	} = $props();

	const id = $props.id();
	const SLOTS: CategorySlot[] = [1, 2, 3, 4, 5, 6];
	const input = 'h-11 text-lg';
	const quick = $derived(quickPicks(today));
	const chosen = $derived(categories.find((c) => c.id === draft.categoryId) ?? null);
	const freeSlot = $derived(SLOTS.find((n) => !categories.some((c) => c.slot === n)) ?? null);
	let picking = $derived(adding);
	let naming = $derived(adding);
	let name = $derived(newName);
	let swatch = $state<CategorySlot | null>(null);
	let addError = $state<string | null>(null);
	let saving = $state(false);
	let field = $state<HTMLDivElement>();
	const hue = $derived(swatch ?? freeSlot);
	const why = $derived(problem(draft));
	let dated = $state(draft.target !== '');
	const picked = $derived(dated && !quick.some((q) => q.ymd === draft.target));

	onMount(() => {
		const main = field?.closest('main');
		if (adding && field && main) main.scrollTop = field.getBoundingClientRect().top - main.getBoundingClientRect().top - 120;
	});

	/** Picks category `c` and closes the picker. */
	function choose(c: CategoryChoice) {
		draft = pickCategory(draft, c.id);
		picking = false;
	}

	/** Adds the typed name as a category on the chosen or next free hue; a blank or taken name is refused. */
	async function addCategory() {
		const n = name.trim();
		if (!n || hue === null || saving) return;
		if (categories.some((c) => c.name.toLowerCase() === n.toLowerCase())) {
			addError = `You already have ${n}.`;
			return;
		}
		saving = true;
		addError = (await onaddcategory?.(n, hue)) ?? null;
		saving = false;
		if (addError) return;
		name = '';
		swatch = null;
		naming = false;
		picking = false;
	}

</script>

<form class="flex min-h-full flex-col" onsubmit={(e) => (e.preventDefault(), canCreate(draft) && !creating && oncreate?.())}>
	<div class="flex flex-1 flex-col gap-4 p-4">
		<Card.Root size="sm" class="gap-4 px-4">
			<Field label="Title" id="{id}-title" detail="Required">
				<Input id="{id}-title" class={input} placeholder="What do you want to reach?" bind:value={draft.title} aria-required="true" maxlength={200} />
			</Field>
			<Field label="Description" id="{id}-desc" detail="Optional">
				<Textarea id="{id}-desc" class="min-h-20" placeholder="Why it matters, what got in the way before" bind:value={draft.description} />
			</Field>
			<Field label="Target date" labelId="{id}-date">
				<div role="radiogroup" aria-labelledby="{id}-date" class="flex flex-wrap gap-1">
					<button type="button" role="radio" aria-checked={!dated} class="h-11 rounded-sm border px-3 text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {!dated ? 'border-fg bg-surface-2 font-medium' : 'border-border bg-bg'}" onclick={() => ((dated = false), (draft = noEndDate(draft)))}>No end date</button>
					{#each quick as q (q.ymd)}
						{@const on = dated && draft.target === q.ymd}
						<button type="button" role="radio" aria-checked={on} class="h-11 rounded-sm border px-3 text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {on ? 'border-fg bg-surface-2 font-medium' : 'border-border bg-bg'}" onclick={() => ((dated = true), (draft = withTarget(draft, q.ymd)))}>{q.label}</button>
					{/each}
					<button type="button" role="radio" aria-checked={picked} class="h-11 rounded-sm border px-3 text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {picked ? 'border-fg bg-surface-2 font-medium' : 'border-border bg-bg'}" onclick={() => ((dated = true), quick.some((q) => q.ymd === draft.target) && (draft = noEndDate(draft)))}>Pick a date</button>
				</div>
				{#if dated}
					<Input id="{id}-date-input" type="date" aria-label="Target date" class="{input} tabular-nums" min={today} bind:value={draft.target} />
				{/if}
			</Field>
			<Field label="Category" labelId="{id}-cat" detail={chosen ? (draft.categoryPicked ? 'Your pick' : detected ? 'Detected from the title' : '') : 'Detected when you type a title'}>
				{#if chosen && !picking}
					<div class="flex items-center gap-2">
						<span class="inline-flex h-11 items-center gap-2 rounded-sm border border-border bg-bg px-3 text-sm font-medium">
							<span class="size-2 rounded-full" style="background: var(--cat-{chosen.slot})" aria-hidden="true"></span>{chosen.name}
						</span>
						<Button variant="ghost" size="touch" onclick={() => (picking = true)}>Change</Button>
					</div>
				{:else}
					<div role="radiogroup" aria-labelledby="{id}-cat" class="flex flex-wrap gap-1">
						{#each categories as c (c.id)}
							<button type="button" role="radio" aria-checked={draft.categoryId === c.id} class="inline-flex h-11 items-center gap-2 rounded-sm border px-3 text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {draft.categoryId === c.id ? 'border-fg bg-surface-2 font-medium' : 'border-border bg-bg'}" onclick={() => choose(c)}>
								<span class="size-2 rounded-full" style="background: var(--cat-{c.slot})" aria-hidden="true"></span>{c.name}
							</button>
						{/each}
						{#if !naming}
							<button type="button" class="inline-flex h-11 items-center gap-1.5 rounded-sm border border-dashed border-border-strong bg-bg px-3 text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => (naming = true)}><Plus class="size-4" aria-hidden="true" />New category</button>
						{/if}
					</div>
					{#if naming}
						<div bind:this={field} class="flex flex-col gap-2 rounded-sm border border-border bg-bg p-3">
							<label for="{id}-newcat" class="text-sm font-medium">New category</label>
							<div class="flex items-center gap-2">
								<Input id="{id}-newcat" class="{input} flex-1" placeholder="For example Reading club" bind:value={name} onkeydown={(e) => e.key === 'Enter' && (e.preventDefault(), addCategory())} />
								<Button size="touch" disabled={!name.trim() || hue === null || saving} onclick={addCategory}>Add</Button>
							</div>
							<div role="radiogroup" aria-label="Color" class="flex gap-1">
								{#each SLOTS as s (s)}
									<button type="button" role="radio" aria-checked={hue === s} aria-label="Color {s}" class="flex size-11 items-center justify-center rounded-sm border outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {hue === s ? 'border-fg' : 'border-transparent'}" onclick={() => (swatch = s)}>
										<span class="size-5 rounded-full" style="background: var(--cat-{s})" aria-hidden="true"></span>
									</button>
								{/each}
							</div>
							<p class="text-xs text-muted">{freeSlot === null ? 'Every color is in use, so pick one. Colors may repeat.' : 'The next free color is picked for you.'} Goals in it use the general coach team until you set its coaches.</p>
							{#if addError}<p class="text-sm text-danger" role="alert">{addError}</p>{/if}
							<div class="flex"><Button variant="ghost" size="touch" onclick={() => ((naming = false), (name = ''), (addError = null))}>Cancel</Button></div>
						</div>
					{/if}
				{/if}
			</Field>
		</Card.Root>

		{#if attach}
			<Attachments
				{files}
				{upload}
				busy={creating}
				{onfiles}
				{onretry}
				onlink={onlink ?? ((url) => (files = [...files, { id: `l${files.length}`, kind: 'link', name: url, detail: 'Link' }]))}
				onremove={onremove ?? ((fid) => (files = files.filter((x) => x.id !== fid)))}
			/>
		{/if}

		<section class="flex flex-col gap-2" aria-labelledby="{id}-agents">
			<h2 id="{id}-agents" class="text-sm font-semibold">Agents</h2>
			<AgentControls bind:value={() => draft, (v) => (draft = { ...draft, ...v })} limit="goal" {defaultLimit} />
		</section>
	</div>

	<div data-hide-typing class="sticky bottom-0 flex flex-col gap-1.5 border-t border-border bg-bg p-4">
		{#if error}<p class="text-sm text-danger" role="alert">{error}</p>{/if}
		<Button type="submit" size="touch" class="w-full" disabled={!canCreate(draft) || creating}>{creating ? 'Creating' : 'Create goal'}</Button>
		<p class="text-center text-xs text-muted">{why ?? 'Agents research first, then ask only what they could not find.'}</p>
	</div>
</form>
