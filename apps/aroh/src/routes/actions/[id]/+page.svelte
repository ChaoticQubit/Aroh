<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import Undo2 from '@lucide/svelte/icons/undo-2';
	import Clock from '@lucide/svelte/icons/clock';
	import Segmented from '$lib/ui/components/Segmented.svelte';
	import StatusChip from '$lib/ui/components/StatusChip.svelte';
	import CatchUpBanner from '$lib/ui/components/CatchUpBanner.svelte';
	import ActionSheet from '$lib/ui/screens/ActionSheet.svelte';
	import { catchUp } from '$lib/app/catchup.svelte';
	import { actionViews } from '$lib/app/views';
	import { logState } from '$lib/ui/status';
	import type { ActionView } from '$lib/ui/contract';
	import { Button } from '$lib/ui/primitives/button';
	import ActionCard from '$lib/ui/components/action/ActionCard.svelte';
	import ScopeSheet from '$lib/ui/components/action/ScopeSheet.svelte';
	import * as client from '$lib/app/client';
	import { data } from '$lib/app/data.svelte';
	import { getClock } from '$lib/ui/clock';
	import { cardMeta, fillDate, logOf, pageCard, scopeText } from '$lib/app/cardpage';
	import { cardProblem, voiceLog } from '$lib/ui/action/edit';
	import type { Card, CardLog } from '$lib/ui/action/card';
	import { Dictation, speech } from '$lib/app/speech.svelte';
	import { deviceKeep, readDraft, writeDraft } from '$lib/app/chatkeep';
	import { untrack } from 'svelte';
	import { formatDay, formatTime, formatYmd } from '$lib/ui/format';

	const SAVE_MS = 400;
	const clock = getClock();
	const id = $derived(page.params.id ?? '');
	let loaded = $state<client.CardPage | null>(null);
	let failed = $state<string | null>(null);
	let card = $state<Card | null>(null);
	let log = $state<CardLog>(logOf(null));
	let notes = $state('');
	let mode = $state<'plan' | 'log'>('plan');
	let scope = $state<'this' | 'future' | null>(null);
	let asking = $state(false);
	let undo = $state<client.CardUndo | null>(null);
	let hint = $state<string | null>(null);
	let editing = $state<{ block: number; row: number; set: number } | null>(null);
	let draft = $state('');
	let saveTimer: ReturnType<typeof setTimeout> | undefined;
	let notesTimer: ReturnType<typeof setTimeout> | undefined;
	const dictation = new Dictation(speech, () => draft, (text) => (draft = text));
	type LogPick = 'planned' | 'done' | 'skipped';
	const LOG: { value: LogPick; label: string }[] = [
		{ value: 'planned', label: 'Not done' },
		{ value: 'done', label: 'Done' },
		{ value: 'skipped', label: 'Skipped' }
	];
	let timing = $state(false);
	const keep = deviceKeep();
	$effect(() => {
		const key = `cardpage:${id}`;
		untrack(() => (draft = readDraft(keep, key)));
	});
	$effect(() => writeDraft(keep, `cardpage:${id}`, draft));

	/** Reads the card page of the action in the URL; a gone action shows why. */
	async function load(actionId: string) {
		try {
			const p = await client.cardPage(actionId);
			loaded = p;
			card = pageCard(p.action);
			log = logOf(p.log);
			notes = p.action.notes ?? '';
			mode = p.log && p.action.status === 'planned' ? 'log' : 'plan';
			failed = null;
		} catch (e) {
			failed = String(e);
		}
	}

	$effect(() => {
		scope = null;
		undo = null;
		void load(id);
	});

	const meta = $derived(loaded && data.snapshot ? cardMeta(data.snapshot, loaded.action, clock.zone) : null);
	const series = $derived(!!loaded?.action.series_id);

	/** Saves the card shortly after the last keystroke, once the scope is known. */
	function schedule() {
		clearTimeout(saveTimer);
		saveTimer = setTimeout(save, SAVE_MS);
	}

	async function save() {
		if (!card || !scope) return;
		hint = cardProblem(card);
		if (hint) return;
		try {
			const saved = await client.setCard(id, $state.snapshot(card), scope);
			undo ??= saved;
		} catch (e) {
			hint = String(e);
		}
	}

	/** An in-place edit: the first one asks this session or all future (series only), then saves as you type. */
	function edit(next: Card) {
		card = next;
		if (!scope && series) {
			asking = true;
			return;
		}
		scope ??= 'this';
		schedule();
	}

	function pick(choice: 'this' | 'future') {
		scope = choice;
		asking = false;
		schedule();
	}

	function cancelScope() {
		asking = false;
		if (loaded) card = pageCard(loaded.action);
	}

	async function revert() {
		if (!undo) return;
		clearTimeout(saveTimer);
		await client.restoreCards($state.snapshot(undo));
		undo = null;
		scope = null;
		await load(id);
	}

	function editNotes(text: string) {
		notes = text;
		clearTimeout(notesTimer);
		notesTimer = setTimeout(() => void client.setNotes(id, text).catch((e) => (hint = String(e))), SAVE_MS);
	}

	async function saveLog(next: CardLog) {
		log = next;
		try {
			await client.saveLog(id, $state.snapshot(log));
		} catch (e) {
			hint = String(e);
		}
	}

	async function finish() {
		await saveLog(log);
		if (loaded) await catchUp.setStatus(loaded.action, 'done');
		history.back();
	}

	/** Logs this session in any status (Q78), then reloads it; a late done may offer the catch-up. */
	async function setStatus(next: LogPick) {
		if (!loaded) return;
		await catchUp.setStatus(loaded.action, next);
		await load(id);
	}

	/** The action as the edit sheet draws it, from the snapshot when it holds the row. */
	const sheetAction = $derived.by((): ActionView | null => {
		if (!loaded) return null;
		const a = loaded.action;
		const view = data.snapshot ? actionViews({ ...data.snapshot, actions: [a] })[0] : undefined;
		return view ?? { id: a.id, title: a.title, start: a.start_at, end: a.end_at, status: a.status, locked: a.locked, category: 1, goalTitle: meta?.goal ?? '', series: a.series_id !== null };
	});
	const chip = $derived(loaded ? logState({ status: loaded.action.status, start: loaded.action.start_at, end: loaded.action.end_at }, clock.now, clock.zone) : null);

	/** Composer text: in a session a spoken or typed set ("last set 7 reps"); else a card chat request. */
	function send(text: string) {
		if (mode === 'log' && card) {
			const next = voiceLog(card, log, text);
			if (next) {
				draft = '';
				hint = null;
				void saveLog(next);
				return;
			}
			hint = "Say a set, such as 'last set 7 reps' or '8 reps at 60'.";
			return;
		}
		draft = '';
		void goto(`/actions/${id}/chat?text=${encodeURIComponent(text)}`);
	}

	async function mic() {
		const was = dictation.listening;
		await dictation.toggle();
		if (was && draft.trim()) send(draft.trim());
	}

	const when = $derived(loaded ? `${formatDay(loaded.action.start_at, clock.zone)}, ${formatTime(loaded.action.start_at, clock.zone)} to ${formatTime(loaded.action.end_at, clock.zone)}` : '');
</script>

{#if failed}
	<p class="p-4 text-md text-muted" role="alert">This session is gone. {failed}</p>
{:else if loaded && card && meta}
	<ActionCard
		{meta}
		{card}
		{log}
		{mode}
		{editing}
		{hint}
		notes={mode === 'plan' ? notes : notes || null}
		fillOn={card.depth === 'outline' && !card.blocks.length ? fillDate(loaded.action, clock.zone, formatYmd(clock.now, clock.zone)) : null}
		mic={true}
		bind:draft
		listening={dictation.listening}
		oncard={card.blocks.length ? edit : undefined}
		onnotes={editNotes}
		onlog={saveLog}
		onedit={(at) => (editing = at)}
		onstart={() => (mode = 'log')}
		onfinish={finish}
		onchat={send}
		onmic={mic}
	>
		{#snippet status()}
			<section class="flex flex-col gap-2" aria-label="Session status">
				<div class="flex flex-wrap items-center gap-2">
					{#if chip && loaded?.action.status !== 'done'}<StatusChip tone={chip.tone} label={chip.label} />{/if}
					<Segmented
						label="Log this session"
						options={LOG}
						bind:value={() => (loaded?.action.status === 'missed' ? ('' as LogPick) : (loaded?.action.status as LogPick)), (v: LogPick) => void setStatus(v)}
					/>
					<Button variant="outline" size="touch" onclick={() => (timing = true)}><Clock />Change time</Button>
				</div>
				<CatchUpBanner offer={catchUp.offer} note={catchUp.note} onremove={() => catchUp.remove()} onkeep={() => catchUp.keep()} />
			</section>
		{/snippet}
	</ActionCard>
	{#if undo && scope}
		<div data-hide-typing class="fixed inset-x-0 bottom-[calc(4.5rem+env(safe-area-inset-bottom))] z-30 mx-auto flex w-[calc(100%-2rem)] max-w-md items-center justify-between gap-2 rounded-md border border-border bg-surface px-3 py-1 shadow-sm" role="status">
			<p class="text-sm">Saved for {scopeText(scope).toLowerCase()}.</p>
			<Button variant="ghost" size="touch" onclick={revert}><Undo2 />Undo</Button>
		</div>
	{/if}
	{#if timing && sheetAction}
		<ActionSheet
			action={sheetAction}
			onstatus={(_, next) => void setStatus(next)}
			onsave={(e) => {
				timing = false;
				void data.run(() => client.editAction(e.id, e.start, e.end)).then(() => load(id));
			}}
			onlock={(actionId, locked) => void data.run(() => client.setLocked(actionId, locked))}
			ondelete={(actionId, scope) => {
				timing = false;
				void data.run(() => client.deleteAction(actionId, scope)).then(() => history.back());
			}}
			onclose={() => (timing = false)}
		/>
	{/if}
	{#if asking}
		<div class="fixed inset-0 z-40">
			<ScopeSheet request="Your edit to {loaded.action.title}" session="{when}. The next sessions stay as planned." future="This one and every later {loaded.action.title} that has no log yet." onpick={pick} oncancel={cancelScope} />
		</div>
	{/if}
{:else}
	<p class="p-4 text-md text-muted">Loading the session.</p>
{/if}
