<script lang="ts">
	import CircleCheck from '@lucide/svelte/icons/circle-check';
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
	import type { ChatMessage, OnChange } from '$lib/ui/contract';
	import { Dictation, type Speech } from '$lib/app/speech.svelte';
	import { scrollFor } from '$lib/app/questions';

	/**
	 * Owner and persona thread with inline cards, a question group, research progress and a plan
	 * awaiting approval, above a composer with text and, given `voice`, a mic that dictates into the draft.
	 * Emits `{ type: 'send', text }` (typed or a card's answer), the group's `answer`, `skip` and `submit`,
	 * `{ type: 'approve', fixed }` (indexes of plan items set to Fixed time), `{ type: 'change' }`, or a
	 * run's `stop` and `retry`. With `echo` false the caller adds sent messages itself. With
	 * `inline` the thread flows in the page, which is the only scroller, and the composer sticks
	 * to the bottom of the screen (the goal card); only a new question group scrolls into view.
	 */
	let {
		messages,
		onchange,
		echo = true,
		voice,
		inline = false
	}: { messages: ChatMessage[]; onchange?: OnChange; echo?: boolean; voice?: Speech; inline?: boolean } = $props();

	const id = $props.id();
	let sent = $state<ChatMessage[]>([]);
	let draft = $state('');
	let approved = $state(false);
	const dictation = $derived(voice ? new Dictation(voice, (text) => (draft = text)) : null);
	let fixed = $state<number[]>([]);
	const thread = $derived([...messages, ...sent]);
	let list = $state<HTMLOListElement>();

	let seenGroup: string | null = null;

	$effect(() => {
		const next = scrollFor(thread, seenGroup);
		seenGroup = next.seen;
		if (!thread.length || !list) return;
		if (next.to === 'bottom' && !inline) list.scrollTo({ top: list.scrollHeight, behavior: 'smooth' });
		else if (next.to === 'group') (list.lastElementChild as HTMLElement | null)?.scrollIntoView({ block: 'start', behavior: 'smooth' });
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
		onchange?.({ type: 'send', text });
	}
</script>

<div class="flex flex-col {inline ? '' : 'h-full min-h-0'}">
	<ol bind:this={list} class="flex flex-col gap-3 {inline ? 'py-2' : 'min-h-0 flex-1 overflow-y-auto p-4'}" aria-label="Conversation">
		{#each thread as m, i (i)}
			<li class="flex flex-col {m.type === 'message' && m.from === 'owner' ? 'items-end' : 'items-stretch'}">
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
				{:else if m.type === 'questions'}
					<QuestionGroup group={m} {onchange} />
				{:else if m.type === 'progress'}
					<div class="flex flex-col gap-1.5 border-l-2 border-border py-1 pl-3" role="status" aria-label="Research progress">
						{#each m.lines as line (line.text)}
							<p class="flex items-center gap-2 text-sm {line.done ? 'text-muted' : 'text-fg'}">
								{#if line.done}<CircleCheck class="size-4 text-status-done" />{:else}<LoaderCircle class="size-4 animate-spin text-status-planned" />{/if}
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
				{:else if m.type === 'plan'}
					<Card.Root size="sm" class="gap-3 px-4">
						<Persona name="Orchestrator" note="Plan ready" />
						<p class="text-sm font-semibold">{m.title}</p>
						<ul class="flex list-disc flex-col gap-1 pl-5 text-sm text-fg">
							{#each m.items as item, i (i)}
								<li>
									<span class="flex items-center justify-between gap-3">
										<span>{item}</span>
										{#if m.fixable?.[i] && !approved}
											<label class="flex shrink-0 items-center gap-2 text-xs text-muted">
												Fixed time
												<Switch checked={fixed.includes(i)} aria-label="Fixed time for {item}" onCheckedChange={(v) => (fixed = v ? [...fixed, i] : fixed.filter((x) => x !== i))} />
											</label>
										{/if}
									</span>
								</li>
							{/each}
						</ul>
						{#if approved}
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
	</ol>
	<form
		class="flex items-end gap-1 border-t border-border bg-bg p-2 {inline ? 'sticky bottom-0 z-10 -mx-4 @3xl:-mx-6' : ''}"
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
			class="max-h-32 min-h-11 min-w-0 flex-1 resize-none rounded-sm border border-input bg-transparent px-3 py-2 text-lg outline-none placeholder:truncate placeholder:text-muted focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
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
	{#if dictation?.error}<p role="alert" class="border-t border-border bg-bg px-3 py-2 text-sm text-status-missed">{dictation.error}</p>{/if}
</div>
