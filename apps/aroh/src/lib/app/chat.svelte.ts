import type { ChatMessage } from '$lib/ui/contract';
import * as client from './client';
import { ACTIVE, chatFromRun, closeOldestQuestion, engine, NEEDS_ENGINE, type RunEvent, type RunView } from './engine';
import { connectWakeups, wakeups } from './realtime';

const POLL_MAX = 400;

/**
 * Reads a run with `after_seq` while it is queued, running or checkpointed, waking on its
 * Realtime events (1.5 s polling when the socket is down), then syncs so rows the engine wrote
 * reach the device. `onview` gets each view with every event after `afterSeq` read so far.
 * Resolves to the last view, or null when the engine was not read.
 */
export async function follow(runId: string, onview: (v: RunView) => void = () => {}, afterSeq = -1): Promise<RunView | null> {
	let last: RunView | null = null;
	const events: RunEvent[] = [];
	let seq = afterSeq;
	await connectWakeups();
	for (let i = 0; i < POLL_MAX; i++) {
		const r = await engine.run(runId, seq < 0 ? undefined : seq);
		if (!r.ok) break;
		events.push(...r.value.events.filter((e) => e.seq > seq));
		seq = Math.max(seq, ...events.map((e) => e.seq));
		last = { ...r.value, events: [...events] };
		onview(last);
		if (!ACTIVE.includes(last.status.status)) break;
		await wakeups.wait(runId);
	}
	await client.sync().catch(() => null);
	return last;
}

/**
 * One persona chat thread. Without a goal the first message starts the new-goal run; with one,
 * text answers the goal's waiting run or starts a replan (see `route`). The thread keeps what was
 * said; the run's state renders after it from the events since the owner last spoke.
 */
export class GoalChat {
	log = $state<ChatMessage[]>([]);
	live = $state<ChatMessage[]>([]);
	goalId: string | null;
	private runId: string | null = null;
	private held: string[] = [];
	private seq = -1;
	private shown = '';

	constructor(goalId: string | null, greeting: string) {
		this.goalId = goalId;
		this.log = [{ type: 'message', from: 'persona', name: 'Aroh', text: greeting }];
	}

	/** Every message of the thread, in order. */
	get messages(): ChatMessage[] {
		return [...this.log, ...this.live];
	}

	/** Picks up a run of this goal that already waits for the owner, if any. */
	async resume() {
		if (!this.goalId) return;
		const waiting = await engine.runs({ goalId: this.goalId, state: 'waiting_human' });
		if (waiting.ok && waiting.value[0]) await this.watch(waiting.value[0].run_id);
	}

	/** Keeps the rendered run state in the thread, so later events render after it. */
	private freeze() {
		this.log.push(...this.live);
		this.live = [];
	}

	/** Shows a line from Aroh after the thread. */
	private say(text: string) {
		this.freeze();
		this.log.push({ type: 'message', from: 'persona', name: 'Aroh', text });
	}

	/** Renders a run view when it moved: new events, or a new status or waiting node. */
	private show(v: RunView) {
		const top = Math.max(this.seq, ...v.events.map((e) => e.seq));
		const key = `${v.status.status}:${v.waiting[0]?.node ?? ''}:${top}`;
		if (key === this.shown) return;
		this.shown = key;
		this.live = chatFromRun(v);
	}

	/** Follows `runId` from the last seen event until it stops; hands a held change text on. */
	private async watch(runId: string) {
		if (runId !== this.runId) {
			this.seq = -1;
			this.shown = '';
		}
		this.runId = runId;
		const last = await follow(runId, (v) => this.show(v), this.seq);
		if (!last) return this.say(NEEDS_ENGINE);
		this.seq = Math.max(this.seq, ...last.events.map((e) => e.seq));
		if (last.waiting[0]?.node === 'change' && this.held.length) {
			const r = await engine.answer(runId, 'change', this.held.shift()!);
			if (!r.ok) return this.say(r.message);
			this.freeze();
			await this.watch(runId);
		}
	}

	/** Sends owner text: starts the goal, answers the waiting node or starts a replan. The text closes the oldest open question card. */
	async send(text: string) {
		this.freeze();
		this.log = closeOldestQuestion(this.log);
		this.log.push({ type: 'message', from: 'owner', name: 'You', text });
		if (!this.goalId) {
			const started = await engine.startGoal(text);
			if (!started.ok) return this.say(started.message);
			this.goalId = started.value.goal_id;
			return this.watch(started.value.run_id);
		}
		const sent = await engine.sendChat(this.goalId, text, this.held);
		if (!sent.ok) return this.say(sent.message);
		this.held = sent.value.held;
		if (sent.value.run_id) await this.watch(sent.value.run_id);
	}

	/** Hands a PersonaChat event (send, approve or change) to `send` or `decide`. */
	handle(event: unknown) {
		const e = event as { type: string; text?: string };
		if (e.type === 'send' && e.text) void this.send(e.text);
		if (e.type === 'approve') void this.decide(true);
		if (e.type === 'change') void this.decide(false);
	}

	/** Approves the plan or proposal the run waits on, or with `yes` false asks what to change. */
	async decide(yes: boolean) {
		if (!this.runId) return;
		const r = await engine.answer(this.runId, 'approve', yes);
		if (!r.ok) return this.say(r.message);
		this.freeze();
		if (!yes) this.say('What should change? Type it below.');
		await this.watch(this.runId);
	}
}
