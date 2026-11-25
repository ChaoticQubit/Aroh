<script lang="ts">
	import CircleCheck from '@lucide/svelte/icons/circle-check';
	import Circle from '@lucide/svelte/icons/circle';
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';
	import Mic from '@lucide/svelte/icons/mic';
	import Send from '@lucide/svelte/icons/send';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import { Switch } from '$lib/ui/primitives/switch';
	import Render from '../Render.svelte';
	import Persona from './Persona.svelte';
	import QuestionCard from './QuestionCard.svelte';
	import QuestionGroup from './QuestionGroup.svelte';
	import PlanCard, { type PlanContext } from '../plan/PlanCard.svelte';
	import CantFitCard from '../plan/CantFitCard.svelte';
	import type { ChatMessage, OnChange } from '$lib/ui/contract';
	import { Dictation, type Speech } from '$lib/app/speech.svelte';
	import DictationNotice from '../inputs/DictationNotice.svelte';
	import { scrollFor } from '$lib/app/questions';
	import { untrack } from 'svelte';
	import { deviceKeep, readDraft, writeDraft } from '$lib/app/chatkeep';
	import { grow } from '$lib/ui/grow';
	import { atEnd } from '$lib/ui/atend';
	import { followOf, splitTail } from '$lib/app/working';
	import ChatTail from './ChatTail.svelte';
	import LatestPill from './LatestPill.svelte';

	/**
	 * Owner and persona thread with inline cards, a question group, research progress and a plan
	 * awaiting approval, above a composer with text and, given `voice`, a mic that dictates into the draft.
	 * Emits `{ type: 'send', text }` (typed or a card's answer), the group's `answer`, `skip` and `submit`,
	 * `{ type: 'approve', fixed }` (indexes of plan items set to Fixed time), `{ type: 'change' }`, or a
	 * run's `stop` and `retry`. With `echo` false the caller adds sent messages itself. With
	 * `inline` the thread flows in the page, which is the only scroller, and the composer sticks
	 * to the bottom of the screen (the goal card); only a new question group scrolls into view.
	 * A plan with its items renders as the week strip (Q65) on `planContext`, in category `slot`; the
	 * limits question group renders as the can't-fit card (Q64). With `keep` the unsent draft is
	 * kept under that key until sent (B60); inline, a sent message scrolls into view (B62). A trailing
	 * `typing` or `failed` entry renders as the working bubble or the failed send with Retry, which
	 * emits `{ type: 'resend' }`; the chat follows it unless the owner scrolled up, then a pill jumps to it (Q87).
	 */
	let {
		messages,
		onchange,
		echo = true,
		voice,
		inline = false,
		planContext,
		slot = 1,
		keep
	}: { messages: ChatMessage[]; onchange?: OnChange; echo?: boolean; voice?: Speech; inline?: boolean; planContext?: PlanContext; slot?: number; keep?: string } = $props();

	const id = $props.id();
	let sent = $state<ChatMessage[]>([]);
	const store = deviceKeep();
	let draft = $state(untrack(() => (keep ? readDraft(store, keep) : '')));
	let follow = false;
	$effect(() => {
		if (keep) writeDraft(store, keep, draft);
	});
	let approved = $state(false);
	const dictation = $derived(voice ? new Dictation(voice, () => draft, (text) => (draft = text)) : null);
	let fixed = $state<number[]>([]);
	const split = $derived(splitTail(messages));
	const thread = $derived([...split.body, ...sent]);
	const tail = $derived(split.tail);
	let list = $state<HTMLOListElement>();
	let tailEl = $state<HTMLElement>();
	let end = $state(true);
	let inset = $state(0);
	const pill = $derived(followOf(tail, end, false) === 'pill');
	const heard = $derived(tail?.type === 'typing' ? 'Working' : tail?.type === 'failed' ? `Not sent. ${tail.text}` : '');

	/** The last thread entry rendered in the list, without the bubble and the end sentinel. */
	const lastEntry = () => (list ? ([...list.querySelectorAll(':scope > [data-entry]')].at(-1) as HTMLElement | undefined) : undefined);

	/** Scrolls the working bubble or failed send to the bottom of the view, above a sticky composer. */
	const jump = () => tailEl?.scrollIntoView({ block: 'end', behavior: 'smooth' });

	let seenGroup: string | null = null;

	$effect(() => {
		const next = scrollFor(thread, seenGroup);
		seenGroup = next.seen;
		const t = tail;
		void (t?.type === 'typing' ? t.step : t?.text);
		if (!thread.length || !list) return;
		const sentNow = follow;
		follow = false;
		const seen = untrack(() => end);
		if (next.to !== 'group' && followOf(t, seen, sentNow) === 'scroll') queueMicrotask(jump);
		else if (next.to === 'bottom' && inline && sentNow) lastEntry()?.scrollIntoView({ block: 'center', behavior: 'smooth' });
		else if (next.to === 'bottom' && !inline && (sentNow || seen)) list.scrollTo({ top: list.scrollHeight, behavior: 'smooth' });
		else if (next.to === 'group') lastEntry()?.scrollIntoView({ block: 'start', behavior: 'smooth' });
	});

	/** Sends a card's answer like typed text; an empty answer is ignored. */
	function answer(value: unknown) {
		const text = (Array.isArray(value) ? value.join(', ') : value === null || value === undefined ? '' : String(value)).trim();
		if (!text) return;
		if (echo) sent.push({ type: 'message', from: 'owner', name: 'You', text });
		onchange?.({ type: 'send', text });
	}

	/** Appends the draft as an owner message and reports it. */
	function send() {
		const text = draft.trim();
		if (!text) return;
		if (echo) sent.push({ type: 'message', from: 'owner', name: 'You', text });
		draft = '';
		follow = true;
		onchange?.({ type: 'send', text });
	}
</script>

<div class="flex flex-col {inline ? '' : 'h-full min-h-0'}">
	<ol bind:this={list} class="flex flex-col gap-3 {inline ? 'py-2' : 'min-h-0 flex-1 overflow-y-auto p-4'}" aria-label="Conversation">
		{#each thread as m, i (i)}
			<li data-entry class="flex flex-col {m.type === 'message' && m.from === 'owner' ? 'items-end' : 'items-stretch'}">
				{#if m.type === 'message'}
					{#if m.from === 'owner'}
						<p class="max-w-[85%] rounded-sm border border-border bg-surface-2 px-3 py-2 text-sm text-fg">{m.text}</p>
					{:else}
						<div class="flex max-w-[92%] flex-col gap-1">
							<Persona name={m.name} />
							<p class="text-sm text-fg">{m.text}</p>
						</div>
					{/if}
				{:else if m.type === 'card'}
					{#if m.spec.kind === 'question_card'}
						<QuestionCard spec={m.spec} onchange={(v) => answer(v)} />
					{:else}
						<Render spec={m.spec} onchange={(v) => answer(v)} />
					{/if}
				{:else if m.type === 'questions' && m.node === 'limit_choice'}
					<CantFitCard group={m} {onchange} />
				{:else if m.type === 'questions'}
					<QuestionGroup group={m} {onchange} />
				{:else if m.type === 'progress'}
					<div class="flex flex-col gap-1.5 border-l-2 border-border py-1 pl-3" role="status" aria-label="Research progress">
						{#each m.lines as line (line.text)}
							<p class="flex items-center gap-2 text-sm {line.done ? 'text-muted' : 'text-fg'}">
								{#if line.done}<CircleCheck class="size-4 text-status-done" />{:else if m.working}<LoaderCircle class="size-4 animate-spin text-status-planned" />{:else}<Circle class="size-4 text-status-skipped" />{/if}
								{line.text}
							</p>
						{/each}
					</div>
				{:else if m.type === 'working'}
					<div class="flex items-center gap-3 rounded-sm border border-border bg-surface px-3 py-2" role="status">
						<LoaderCircle class="size-4 shrink-0 animate-spin text-status-planned" />
						<p class="min-w-0 flex-1 text-sm text-fg">{m.step ?? 'Working'}<span class="block text-xs text-muted tabular-nums">{m.cost} spent so far</span></p>
						<Button size="touch" variant="outline" onclick={() => onchange?.({ type: 'stop' })}>Stop</Button>
					</div>
				{:else if m.type === 'stopped'}
					<div class="flex items-center gap-3 rounded-sm border border-border bg-surface px-3 py-2" role="status">
						<p class="min-w-0 flex-1 text-sm text-fg">{m.text}</p>
						{#if m.retry}<Button size="touch" variant="outline" onclick={() => onchange?.({ type: 'retry' })}>Retry</Button>{/if}
					</div>
				{:else if m.type === 'plan' && m.plan}
					<PlanCard
						title={m.title}
						plan={m.plan}
						context={planContext}
						{slot}
						approved={m.approved ?? approved}
						onapprove={(f) => {
							approved = true;
							onchange?.({ type: 'approve', fixed: f });
						}}
						onchange={() => onchange?.({ type: 'change' })}
					/>
				{:else if m.type === 'plan'}
					<Card.Root size="sm" class="gap-3 px-4">
						<Persona name="Orchestrator" note="Plan ready" />
						<p class="text-sm font-semibold">{m.title}</p>
						<ul class="flex list-disc flex-col gap-1 pl-5 text-sm text-fg">
							{#each m.items as item, i (i)}
								<li>
									<span class="flex items-center justify-between gap-3">
										<span>{item}</span>
										{#if m.fixable?.[i] && !(m.approved ?? approved)}
											<label class="flex shrink-0 items-center gap-2 text-xs text-muted">
												Fixed time
												<Switch checked={fixed.includes(i)} aria-label="Fixed time for {item}" onCheckedChange={(v) => (fixed = v ? [...fixed, i] : fixed.filter((x) => x !== i))} />
											</label>
										{/if}
									</span>
								</li>
							{/each}
						</ul>
						{#if m.approved ?? approved}
							<p class="text-sm text-muted" role="status">Approved. The plan is on your calendar.</p>
						{:else}
							<div class="flex gap-2">
								<Button
									size="touch"
									class="@3xl:h-8"
									onclick={() => {
										approved = true;
										onchange?.({ type: 'approve', fixed });
									}}>Approve plan</Button
								>
								<Button size="touch" variant="outline" class="@3xl:h-8" onclick={() => onchange?.({ type: 'change' })}>Change</Button>
							</div>
						{/if}
					</Card.Root>
				{/if}
			</li>
		{/each}
		{#if tail}
			<li bind:this={tailEl} class="flex flex-col" style="scroll-margin-bottom: {inline ? inset + 12 : 0}px">
				<ChatTail {tail} onretry={() => onchange?.({ type: 'resend' })} />
			</li>
		{/if}
		<li aria-hidden="true" class="-mt-3 h-px shrink-0" use:atEnd={{ own: !inline, inset: inline ? inset : 0, onchange: (v) => (end = v) }}></li>
	</ol>
	<p class="sr-only" aria-live="polite">{heard}</p>
	<div data-composer class={inline ? 'sticky bottom-0 z-10 -mx-4 @3xl:-mx-6' : 'relative'} bind:clientHeight={inset}>
		{#if pill}<LatestPill onjump={jump} />{/if}
		{#if dictation}<DictationNotice {dictation} />{/if}
		<form
			class="flex items-end gap-1 border-t border-border bg-bg p-2"
			onsubmit={(e) => {
				e.preventDefault();
				send();
			}}
		>
			<label for={id} class="sr-only">Message</label>
			<textarea
				{id}
				rows="1"
				placeholder="Message your personas"
				use:grow={draft}
				class="min-h-11 min-w-0 flex-1 resize-none rounded-sm border border-input bg-transparent px-3 py-2 text-lg outline-none placeholder:truncate placeholder:text-muted focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
				bind:value={draft}
				onkeydown={(e) => {
					if (e.key === 'Enter' && !e.shiftKey) {
						e.preventDefault();
						send();
					}
				}}
			></textarea>
			{#if dictation}
				<Button variant={dictation.listening ? 'default' : 'ghost'} size="icon-touch" aria-label={dictation.listening ? 'Stop speaking' : 'Speak a message'} aria-pressed={dictation.listening} type="button" onclick={() => void dictation.toggle()}><Mic /></Button>
			{/if}
			<Button size="icon-touch" aria-label="Send" type="submit" disabled={!draft.trim()}><Send /></Button>
		</form>
	</div>
</div>
