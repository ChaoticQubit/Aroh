import type { ChatMessage } from '$lib/ui/contract';
import type { LimitsCard } from '$lib/ui/plan/limits';
import * as client from './client';
import { ACTIVE, chatFromRun, engine, fixedActionIds, NEEDS_ENGINE, questionsOf, type PlanItem, type RunEvent, type RunView } from './engine';
import { fillNext, groupMessage, submissionOf, withAnswer, withSkip, type QuestionGroup } from './questions';
import { connectWakeups, wakeups } from './realtime';
import { blankDraft, startBody } from './newgoal';
import { deviceZone } from '$lib/ui/clock';
import { originRun, pickRun } from './goalcard';

const POLL_MAX = 400;
const DAY_MS = 86_400_000;
const SNAPSHOT_DAYS = 119;

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
 * said; the run's state renders after it from the events since the owner last spoke. Open
 * questions render last as one group with one submit; nothing is posted until it is submitted.
 */
export class GoalChat {
	log = $state<ChatMessage[]>([]);
	live = $state<ChatMessage[]>([]);
	group = $state<QuestionGroup | null>(null);
	view = $state<RunView | null>(null);
	origin = $state<RunView | null>(null);
	private originId: string | null = null;
	goalId: string | null;
	private runId: string | null = null;
	private change: string | null = null;
	private seq = -1;
	private shown = '';
	private plan: { items: PlanItem[]; timezone: string } | null = null;

	constructor(goalId: string | null, greeting: string) {
		this.goalId = goalId;
		this.log = [{ type: 'message', from: 'persona', name: 'Aroh', text: greeting }];
	}

	/** Every message of the thread, in order, with the open question group last. */
	get messages(): ChatMessage[] {
		return [...this.log, ...this.live, ...(this.group ? [groupMessage(this.group)] : [])];
	}

	/** The thread without run state lines (progress, working, stopped), which the goal card's status header shows instead. */
	get thread(): ChatMessage[] {
		return this.messages.filter((m) => m.type !== 'progress' && m.type !== 'working' && m.type !== 'stopped');
	}

	/**
	 * Picks up the goal card's run: the newest run of this goal that waits for the owner, else
	 * the newest run. A waiting or moving run is followed in the thread; a finished or stopped
	 * one only sets `view`, so the card's status reads it and Retry works. `origin` is the newest
	 * new-goal run, whose research and answers the card keeps showing after later replans.
	 */
	async open() {
		if (!this.goalId) return;
		const runs = await engine.runs({ goalId: this.goalId });
		const id = runs.ok ? pickRun(runs.value) : null;
		const picked = runs.ok ? runs.value.find((r) => r.run_id === id) : undefined;
		this.originId = runs.ok ? originRun(runs.value) : null;
		if (this.originId && this.originId !== id) {
			const first = await engine.run(this.originId);
			if (first.ok) this.origin = first.value;
		}
		if (!id || !picked) return;
		if (picked.state === 'waiting_human' || ACTIVE.includes(picked.state)) return this.watch(id);
		const r = await engine.run(id);
		if (!r.ok) return;
		this.runId = id;
		this.view = r.value;
		if (id === this.originId) this.origin = r.value;
		this.seq = Math.max(-1, ...r.value.events.map((e) => e.seq));
	}

	/** Picks up a run of this goal that already waits for the owner, if any, so its open questions show again. */
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

	/** Renders a run view when it moved, and opens its question group when it waits at questions. */
	private show(v: RunView) {
		this.view = v;
		if (v.run_id === this.originId) this.origin = v;
		const top = Math.max(this.seq, ...v.events.map((e) => e.seq));
		const key = `${v.status.status}:${v.waiting[0]?.node ?? ''}:${top}`;
		if (key === this.shown) return;
		this.shown = key;
		const plan = v.waiting.find((w) => w.node === 'approve')?.data.plan as { items: PlanItem[]; timezone: string } | undefined;
		if (plan) this.plan = plan;
		this.live = chatFromRun(v);
		const questions = v.status.status === 'waiting_human' ? questionsOf(v.waiting[0]) : [];
		if (!questions.length) this.group = null;
		else if (this.group?.runId !== v.run_id || this.group.node !== v.waiting[0].node) {
			const w = v.waiting[0];
			const card = w.node === 'limit_choice' ? (((w.data.limits as { card?: LimitsCard | null } | undefined)?.card ?? null) as LimitsCard | null) : undefined;
			this.group = { runId: v.run_id, node: w.node, prompt: w.prompt ?? 'Answer these so the plan fits you.', questions, values: {}, skipped: [], sending: false, error: null, card };
		}
	}

	/** Follows `runId` from the last seen event until it stops; hands a pending change text on. */
	private async watch(runId: string) {
		if (runId !== this.runId) {
			this.seq = -1;
			this.shown = '';
		}
		this.runId = runId;
		const last = await follow(runId, (v) => this.show(v), this.seq);
		if (!last) return this.say(NEEDS_ENGINE);
		this.seq = Math.max(this.seq, ...last.events.map((e) => e.seq));
		if (last.waiting[0]?.node === 'change' && this.change !== null) {
			const text = this.change;
			this.change = null;
			const r = await engine.answer(runId, 'change', text);
			if (!r.ok) return this.say(r.message);
			this.freeze();
			await this.watch(runId);
		}
	}

	/**
	 * Sends owner text: starts the goal, fills the next open question of the group, answers the
	 * waiting node or starts a replan.
	 */
	async send(text: string) {
		if (this.group) return this.fill(text);
		this.freeze();
		this.log.push({ type: 'message', from: 'owner', name: 'You', text });
		if (!this.goalId) {
			const started = await engine.startGoal(startBody({ ...blankDraft(), title: text.trim().slice(0, 200), description: text }, deviceZone()));
			if (!started.ok) return this.say(started.message);
			this.goalId = started.value.goal_id;
			return this.watch(started.value.run_id);
		}
		const sent = await engine.sendChat(this.goalId, text);
		if (!sent.ok) return this.say(sent.message);
		this.change = sent.value.change ?? null;
		if (sent.value.run_id) await this.watch(sent.value.run_id);
		if (sent.value.questions) this.fill(text);
	}

	/** Puts typed text into the first open question of the group, or says every question has an answer. */
	private fill(text: string) {
		if (!this.group) return;
		const next = fillNext(this.group, text);
		if (next) this.group = next;
		else this.say('Every question has an answer or a skip. Tap Send answers.');
	}

	/** Posts every answer and skip of the group in one request; on a refusal keeps the answers and says why. */
	async submit() {
		const g = this.group;
		if (!g || g.sending) return;
		const answer = submissionOf(g);
		if (!answer) {
			this.group = { ...g, error: 'Answer or skip every question first.' };
			return;
		}
		this.group = { ...g, sending: true, error: null };
		const r = await engine.answer(g.runId, g.node, answer);
		if (!r.ok) {
			this.group = { ...g, sending: false, error: `Your answers were not sent: ${r.message}` };
			return;
		}
		this.freeze();
		this.log.push(groupMessage({ ...g, sending: false, error: null }, true));
		this.group = null;
		await this.watch(g.runId);
	}

	/** Stops the followed run now; the chat shows it stopped once the engine has saved that. */
	async stop() {
		if (!this.runId) return;
		const r = await engine.cancel(this.runId);
		if (!r.ok) return this.say(`The run was not stopped: ${r.message}`);
		await this.watch(this.runId);
	}

	/** Continues the stopped run where it stopped, with a fresh time limit, and follows it. */
	async retry() {
		if (!this.runId) return;
		const r = await engine.retry(this.runId);
		if (!r.ok) return this.say(`The run did not restart: ${r.message}`);
		await this.watch(this.runId);
	}

	/** Hands a PersonaChat event (send, answer, skip, submit, approve with its fixed items, change, stop or retry) to the chat. */
	handle(event: unknown) {
		if ((event as { type?: string }).type === 'stop') void this.stop();
		if ((event as { type?: string }).type === 'retry') void this.retry();
		const e = event as { type: string; text?: string; id?: string; value?: unknown; skipped?: boolean; fixed?: number[] };
		if (e.type === 'send' && e.text?.trim()) void this.send(e.text.trim());
		if (e.type === 'answer' && this.group && e.id) this.group = withAnswer(this.group, e.id, e.value);
		if (e.type === 'skip' && this.group && e.id) this.group = withSkip(this.group, e.id, e.skipped ?? true);
		if (e.type === 'submit') void this.submit();
		if (e.type === 'approve') void this.decide(true, e.fixed ?? []);
		if (e.type === 'change') void this.decide(false);
	}

	/**
	 * Approves the plan or proposal the run waits on, or with `yes` false asks what to change.
	 * Once an approved plan is written and synced, the actions of its `fixed` item indexes are locked.
	 */
	async decide(yes: boolean, fixed: number[] = []) {
		if (!this.runId) return;
		const plan = this.plan;
		const r = await engine.answer(this.runId, 'approve', yes);
		if (!r.ok) return this.say(r.message);
		this.freeze();
		if (!yes) this.say('What should change? Type it below.');
		await this.watch(this.runId);
		if (yes && plan && fixed.length && this.goalId) await this.lockFixed(this.goalId, plan, fixed);
	}

	/** Syncs the written plan, then sets Fixed time on the actions the plan items at `fixed` became. */
	private async lockFixed(goalId: string, plan: { items: PlanItem[]; timezone: string }, fixed: number[]) {
		try {
			await client.sync();
			const now = Date.now();
			const s = await client.snapshot(new Date(now - DAY_MS).toISOString(), new Date(now + SNAPSHOT_DAYS * DAY_MS).toISOString());
			for (const id of fixedActionIds(s.actions, goalId, plan.items, fixed, plan.timezone)) await client.setLocked(id, true);
		} catch (e) {
			this.say(`Fixed time was not set: ${String(e)}`);
		}
	}
}
