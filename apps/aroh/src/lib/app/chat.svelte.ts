import type { ChatMessage } from '$lib/ui/contract';
import type { LimitsCard } from '$lib/ui/plan/limits';
import * as client from './client';
import { ACTIVE, chatFromRun, engine, fixedActionIds, NEEDS_ENGINE, questionsOf, type EngineResult, type PlanItem, type RunAnswer, type RunEvent, type RunSummary, type RunView } from './engine';
import { fillNext, groupMessage, knownValues, submissionOf, withAnswer, withSkip, type QuestionGroup } from './questions';
import { connectWakeups, wakeups } from './realtime';
import { blankDraft, startBody } from './newgoal';
import { deviceZone } from '$lib/ui/clock';
import { originRun, pickRun } from './goalcard';
import { deviceKeep, readSent, rememberSent, type Keep } from './chatkeep';
import { bubbleOf, runWorking, type FailedSend } from './working';
import { decideProposal } from './proposals';

const POLL_MAX = 400;

/** The run and node a waiting card belongs to, or null for a message that is not one. */
const keyOf = (m: ChatMessage): string | null => (m.type === 'plan' ? (m.key ?? null) : m.type === 'proposal' ? m.key : null);
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
	const ticket = engine.ticket();
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
	if (last && afterSeq < 0) engine.keepRun(ticket, last);
	await client.sync().catch(() => null);
	return last;
}

/**
 * One persona chat thread. Without a goal the first message starts the new-goal run; with one,
 * text answers the goal's waiting run or starts a replan (see `route`). The thread keeps what was
 * said; the run's state renders after it from the events since the owner last spoke. Open
 * questions render last as one group with one submit; nothing is posted until it is submitted.
 * Owner messages sent on a goal are kept in `store` (B62) and open again after the greeting.
 */
export class GoalChat {
	log = $state<ChatMessage[]>([]);
	live = $state<ChatMessage[]>([]);
	group = $state<QuestionGroup | null>(null);
	view = $state<RunView | null>(null);
	origin = $state<RunView | null>(null);
	unreachable = $state(false);
	known = $state(false);
	busy = $state(0);
	following = $state(0);
	private read = $state(false);
	failed = $state<FailedSend | null>(null);
	private approvedKeys = $state<string[]>([]);
	private decidedKeys = $state<Record<string, 'apply' | 'reject'>>({});
	private originId: string | null = null;
	goalId: string | null;
	private runId: string | null = null;
	private change: string | null = null;
	private seq = -1;
	private shown = '';
	private plan: { items: PlanItem[]; timezone: string } | null = null;
	private store: Keep | null;

	constructor(goalId: string | null, greeting: string, store: Keep | null = deviceKeep()) {
		this.goalId = goalId;
		this.store = store;
		const kept: ChatMessage[] = goalId ? readSent(store, goalId).map((m) => ({ type: 'message', from: 'owner', name: 'You', text: m.text })) : [];
		this.log = [{ type: 'message', from: 'persona', name: 'Aroh', text: greeting }, ...kept];
		if (goalId) this.seed(goalId);
	}

	/**
	 * Shows the goal's run state last read on this device at once (B87): the picked and origin
	 * run views from the engine cache, display only. `open` reads the engine and replaces them.
	 */
	private seed(goalId: string) {
		const runs = engine.cachedRuns({ goalId });
		if (!runs) return;
		const id = pickRun(runs);
		const view = id ? engine.cachedRun(id) : undefined;
		if (id && !view) return;
		const originId = originRun(runs);
		this.view = view ?? null;
		this.origin = (originId ? engine.cachedRun(originId) : undefined) ?? null;
		this.known = true;
	}

	/**
	 * Every message of the thread, in order, with the open question group last. A card of a waiting
	 * node renders once: the newest one of its run and node. A plan card the owner approved shows
	 * approved; a proposal the owner applied or dismissed shows that decision.
	 */
	get messages(): ChatMessage[] {
		const later = new Set(this.live.map(keyOf).filter((k) => k !== null));
		const shown = [...this.log.filter((m) => !later.has(keyOf(m) ?? '')), ...this.live].map((m): ChatMessage => {
			if (m.type === 'plan' && m.key && this.approvedKeys.includes(m.key)) return { ...m, approved: true };
			if (m.type === 'proposal' && this.decidedKeys[m.key]) return { ...m, decided: this.decidedKeys[m.key] };
			return m;
		});
		return [...shown, ...(this.group ? [groupMessage(this.group)] : [])];
	}

	/**
	 * The thread without run state lines (progress, working, stopped), which the goal card's status
	 * header shows instead, ending in the working bubble or a failed send's error (Q87).
	 */
	get thread(): ChatMessage[] {
		const working = this.busy > 0 || runWorking(this.following, this.read, this.view?.status.status ?? null);
		const tail = bubbleOf(working, this.live, this.failed);
		const rows = this.messages.filter((m) => m.type !== 'progress' && m.type !== 'working' && m.type !== 'stopped');
		return tail ? [...rows, tail] : rows;
	}

	/** Runs `work`, the engine calls of a send before its run is followed, with the working bubble up. */
	private async working(work: () => Promise<void>) {
		this.busy++;
		try {
			await work();
		} finally {
			this.busy = Math.max(0, this.busy - 1);
		}
	}

	/**
	 * Picks up the goal card's run: the newest run of this goal that waits for the owner, else
	 * the newest run. A waiting or moving run is followed in the thread; a finished or stopped
	 * one only sets `view`, so the card's status reads it and Retry works. `origin` is the newest
	 * new-goal run, whose research and answers the card keeps showing after later replans.
	 * `read` is the goal's runs read the page already started, so it is not read twice (B87);
	 * the origin and picked run views are read at the same time.
	 */
	async open(read?: Promise<EngineResult<RunSummary[]>>) {
		if (!this.goalId) return;
		const runs = await (read ?? engine.runs({ goalId: this.goalId }));
		this.unreachable = !runs.ok;
		const id = runs.ok ? pickRun(runs.value) : null;
		const picked = runs.ok ? runs.value.find((r) => r.run_id === id) : undefined;
		if (!picked) this.known = true;
		this.originId = runs.ok ? originRun(runs.value) : null;
		const originId = this.originId;
		if (!runs.ok || !originId) this.origin = null;
		if (!runs.ok || !picked) this.view = null;
		const origin = originId && originId !== id ? engine.run(originId).then((first) => void (first.ok && this.originId === originId && (this.origin = first.value))) : Promise.resolve();
		if (!id || !picked) return origin;
		if (picked.state === 'waiting_human' || ACTIVE.includes(picked.state)) return void (await Promise.all([origin, this.watch(id)]));
		const [r] = await Promise.all([engine.run(id), origin]);
		this.unreachable = !r.ok;
		this.known = true;
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

	/** Keeps the rendered run state in the thread, so later events render after it; an older card of the same run and node is dropped. */
	private freeze() {
		const later = new Set(this.live.map(keyOf).filter((k) => k !== null));
		this.log = [...this.log.filter((m) => !later.has(keyOf(m) ?? '')), ...this.live];
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
		this.known = true;
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
			this.group = { runId: v.run_id, node: w.node, prompt: w.prompt ?? 'Answer these so the plan fits you.', questions, values: knownValues(questions), skipped: [], sending: false, error: null, card };
		}
	}

	/** Follows `runId` from the last seen event until it stops; hands a pending change text on. */
	private async watch(runId: string) {
		if (runId !== this.runId) {
			this.seq = -1;
			this.shown = '';
		}
		this.runId = runId;
		this.following++;
		this.read = false;
		let last: RunView | null;
		try {
			last = await follow(
				runId,
				(v) => {
					this.read = true;
					this.show(v);
				},
				this.seq
			);
		} finally {
			this.following = Math.max(0, this.following - 1);
		}
		this.unreachable = !last && !this.view;
		this.known = true;
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
		this.failed = null;
		this.log.push({ type: 'message', from: 'owner', name: 'You', text });
		if (this.goalId) rememberSent(this.store, this.goalId, text, Date.now());
		await this.deliver(text);
	}

	/** Sends the failed text again, once, without a second owner line. */
	async resend() {
		const f = this.failed;
		if (!f) return;
		this.failed = null;
		await this.deliver(f.text);
	}

	/** Hands owner text already in the thread to the engine; a refusal becomes the failed send with Retry. */
	private async deliver(text: string): Promise<void> {
		let run: string | null = null;
		let questions = false;
		await this.working(async () => {
			if (!this.goalId) {
				const started = await engine.startGoal(startBody({ ...blankDraft(), title: text.trim().slice(0, 200), description: text }, deviceZone()));
				if (!started.ok) {
					this.failed = { text, message: started.message };
					return;
				}
				this.goalId = started.value.goal_id;
				run = started.value.run_id;
				return;
			}
			const sent = await engine.sendChat(this.goalId, text);
			if (!sent.ok) {
				this.failed = { text, message: sent.message };
				return;
			}
			this.change = sent.value.change ?? null;
			run = sent.value.run_id;
			questions = sent.value.questions === true;
		});
		if (run) await this.watch(run);
		if (questions) this.fill(text);
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

	/**
	 * Plans the goal again (Q74) with its saved answers, or with `answers` when the owner edited
	 * them, and follows the new run, whose research and answers the card then shows.
	 */
	async runAgain(answers?: RunAnswer[]) {
		if (!this.goalId) return;
		const r = await engine.runAgain(this.goalId, answers);
		if (!r.ok) return this.say(`The goal did not start again: ${r.message}`);
		this.originId = r.value.run_id;
		await this.watch(r.value.run_id);
	}

	/** Hands a PersonaChat event (send, resend, answer, skip, submit, approve with its fixed items, change, a proposal decision, stop or retry) to the chat. */
	handle(event: unknown) {
		if ((event as { type?: string }).type === 'stop') void this.stop();
		if ((event as { type?: string }).type === 'retry') void this.retry();
		const e = event as { type: string; text?: string; id?: string; value?: unknown; skipped?: boolean; fixed?: number[]; decision?: unknown };
		if (e.type === 'proposal' && (e.decision === 'apply' || e.decision === 'reject')) void this.decideProposal(e.decision === 'apply');
		if (e.type === 'send' && e.text?.trim()) void this.send(e.text.trim());
		if (e.type === 'resend') void this.resend();
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
		const key = `${this.runId}:approve`;
		if (yes) this.approvedKeys = [...this.approvedKeys, key];
		const r = await engine.answer(this.runId, 'approve', yes);
		if (!r.ok) {
			this.approvedKeys = this.approvedKeys.filter((k) => k !== key);
			return this.say(r.message);
		}
		this.freeze();
		if (!yes) this.say('What should change? Type it below.');
		await this.watch(this.runId);
		if (yes && plan && fixed.length && this.goalId) await this.lockFixed(this.goalId, plan, fixed);
	}

	/**
	 * Applies (`yes`) or dismisses the proposal the followed run waits on, through the action the
	 * Inbox calls (B114), then follows the run to its end. A refusal makes the card decidable again.
	 */
	async decideProposal(yes: boolean) {
		const runId = this.runId;
		const record = this.view?.run_id === runId ? this.view?.waiting.find((w) => w.node === 'approve')?.data.proposal_record : undefined;
		const id = (record as { proposal_id?: string } | undefined)?.proposal_id;
		if (!runId || !id) return;
		const key = `${runId}:approve`;
		this.decidedKeys = { ...this.decidedKeys, [key]: yes ? 'apply' : 'reject' };
		const r = await decideProposal(id, this.goalId, yes, runId);
		if (!r.ok) {
			const { [key]: _, ...rest } = this.decidedKeys;
			this.decidedKeys = rest;
			return this.say(r.message);
		}
		this.freeze();
		await this.watch(runId);
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
