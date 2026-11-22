<script lang="ts">
	import CircleCheck from '@lucide/svelte/icons/circle-check';
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';
	import Mic from '@lucide/svelte/icons/mic';
	import Paperclip from '@lucide/svelte/icons/paperclip';
	import Send from '@lucide/svelte/icons/send';
	import { Button } from '$lib/ui/primitives/button';
	import * as Card from '$lib/ui/primitives/card';
	import Render from '../Render.svelte';
	import Persona from './Persona.svelte';
	import QuestionCard from './QuestionCard.svelte';
	import type { ChatMessage, OnChange } from '$lib/ui/contract';

	/**
	 * Owner and persona thread with inline cards, research progress and a plan awaiting approval,
	 * above a composer with text, attach and mic. Emits `{ type: 'send', text }` (typed or a card's
	 * answer), `{ type: 'approve' }` or `{ type: 'change' }`. With `echo` false the caller adds
	 * sent messages to `messages` itself, so its replies stay in order after them.
	 */
	let { messages, onchange, echo = true }: { messages: ChatMessage[]; onchange?: OnChange; echo?: boolean } = $props();

	const id = $props.id();
	let sent = $state<ChatMessage[]>([]);
	let draft = $state('');
	let approved = $state(false);
	const thread = $derived([...messages, ...sent]);
	let list = $state<HTMLOListElement>();

	$effect(() => {
		if (thread.length && list) list.scrollTo({ top: list.scrollHeight, behavior: 'smooth' });
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

<div class="flex h-full min-h-0 flex-col">
	<ol bind:this={list} class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4" aria-label="Conversation">
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
						<QuestionCard spec={m.spec} answered={m.answered} onchange={(v) => answer(v)} />
					{:else}
						<Render spec={m.spec} onchange={(v) => answer(v)} />
					{/if}
				{:else if m.type === 'progress'}
					<div class="flex flex-col gap-1.5 border-l-2 border-border py-1 pl-3" role="status" aria-label="Research progress">
						{#each m.lines as line (line.text)}
							<p class="flex items-center gap-2 text-sm {line.done ? 'text-muted' : 'text-fg'}">
								{#if line.done}<CircleCheck class="size-4 text-status-done" />{:else}<LoaderCircle class="size-4 animate-spin text-status-planned" />{/if}
								{line.text}
							</p>
						{/each}
					</div>
				{:else if m.type === 'plan'}
					<Card.Root size="sm" class="gap-3 px-4">
						<Persona name="Orchestrator" note="Plan ready" />
						<p class="text-sm font-semibold">{m.title}</p>
						<ul class="flex list-disc flex-col gap-1 pl-5 text-sm text-fg">
							{#each m.items as item (item)}<li>{item}</li>{/each}
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
										onchange?.({ type: 'approve' });
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
		class="flex items-end gap-1 border-t border-border bg-bg p-2"
		onsubmit={(e) => {
			e.preventDefault();
			send();
		}}
	>
		<Button variant="ghost" size="icon-touch" aria-label="Attach a file" type="button"><Paperclip /></Button>
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
		<Button variant="ghost" size="icon-touch" aria-label="Speak a message" type="button"><Mic /></Button>
		<Button size="icon-touch" aria-label="Send" type="submit" disabled={!draft.trim()}><Send /></Button>
	</form>
</div>
