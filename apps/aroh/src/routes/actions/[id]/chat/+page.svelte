<script lang="ts">
	import { page } from '$app/state';
	import CardChat, { type CardChatEntry } from '$lib/ui/components/action/CardChat.svelte';
	import ScopeSheet from '$lib/ui/components/action/ScopeSheet.svelte';
	import * as client from '$lib/app/client';
	import { engine } from '$lib/app/engine';
	import { follow } from '$lib/app/chat.svelte';
	import { pageCard, scopeText } from '$lib/app/cardpage';
	import { cardDiff, type Card } from '$lib/ui/action/card';
	import { getClock } from '$lib/ui/clock';
	import { formatDay, formatTime } from '$lib/ui/format';

	type Edit = { card: Card | null; minutes: number };
	const COACH = 'Your coach';
	const clock = getClock();
	const id = $derived(page.params.id ?? '');
	let action = $state<client.Action | null>(null);
	let entries = $state<CardChatEntry[]>([]);
	let proposals = $state<Record<number, string>>({});
	let pending = $state<string | null>(null);
	let busy = $state(false);

	/** Loads the action and sends the request the card page handed over in `?text=`. */
	$effect(() => {
		const text = page.url.searchParams.get('text');
		void client.cardPage(id).then((p) => {
			action = p.action;
			if (text) ask(text);
		});
	});

	/** The scope question comes first for a series session; a one-off session changes alone. */
	function ask(text: string) {
		if (busy) return;
		if (action?.series_id) pending = text;
		else void send('this', text);
	}

	/** Sends `text` to the card coach for `scope`, follows the run and shows the applied diff with Undo. */
	async function send(scope: 'this' | 'future', text: string) {
		pending = null;
		busy = true;
		entries = [...entries, { kind: 'owner', text }];
		const started = await engine.cardChat(id, scope, text);
		if (!started.ok) {
			entries = [...entries, { kind: 'coach', text: started.message }];
			busy = false;
			return;
		}
		const view = await follow(started.value.run_id);
		const p = await client.cardPage(id);
		action = p.action;
		const edit = p.edit;
		const diff = edit?.diff as { before?: Edit; after?: Edit; scope?: 'this' | 'future' } | undefined;
		if (view?.status.status !== 'done' || !edit || !diff?.after?.card) {
			entries = [...entries, { kind: 'coach', text: view?.status.error ?? 'The coach could not change the card. Try again.' }];
			busy = false;
			return;
		}
		const before = { card: diff.before?.card ?? pageCard({ ...p.action, card: null }), minutes: diff.before?.minutes ?? diff.after.minutes };
		entries = [
			...entries,
			{ kind: 'coach', text: edit.summary },
			{ kind: 'applied', scope: scopeText(diff.scope ?? scope), lines: cardDiff(before, { card: diff.after.card, minutes: diff.after.minutes }), undone: edit.status === 'undone' }
		];
		proposals = { ...proposals, [entries.length - 1]: edit.id };
		busy = false;
	}

	async function undo(i: number) {
		const proposal = proposals[i];
		const e = entries[i];
		if (!proposal || e.kind !== 'applied') return;
		await client.undoCardEdit(proposal);
		entries = entries.map((x, j) => (j === i && x.kind === 'applied' ? { ...x, undone: true } : x));
	}

	const when = $derived(action ? `${formatDay(action.start_at, clock.zone)}, ${formatTime(action.start_at, clock.zone)} to ${formatTime(action.end_at, clock.zone)}` : '');
</script>

<div class="flex h-full min-h-[calc(100dvh-8rem)] flex-col">
	<CardChat coach={COACH} {entries} onsend={ask} onundo={undo} />
	{#if busy}<p class="px-4 pb-2 text-sm text-muted" role="status">The coach is changing the card.</p>{/if}
</div>
{#if pending && action}
	<div class="fixed inset-0 z-40">
		<ScopeSheet
			request={pending}
			session="{when}. The next {action.title} sessions stay as planned."
			future="This one and every later {action.title} that has no log yet."
			onpick={(scope) => pending && send(scope, pending)}
			oncancel={() => (pending = null)}
		/>
	</div>
{/if}
