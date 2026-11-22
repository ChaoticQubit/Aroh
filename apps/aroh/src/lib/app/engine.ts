import { invoke } from '@tauri-apps/api/core';
import type { ChatMessage, ComponentSpec } from '$lib/ui/contract';

/** Shown wherever an action needs the server engine (Q3) and the app is not connected to it. */
export const NEEDS_ENGINE = 'This needs a connection to the engine. Sign in and set the server URL in Settings. Everything else works offline.';

/** The outcome of an engine call: a value, or the reason it could not run. */
export type EngineResult<T> = { ok: true; value: T } | { ok: false; reason: 'needs_engine' | 'error'; message: string };

/** What the engine answered: HTTP status and JSON body. */
export type Answer = { status: number; body: unknown };

/** One engine request: method, `/v1/...` path and optional JSON body. Rejects with a string. */
export type Call = (method: 'GET' | 'POST' | 'PUT' | 'DELETE', path: string, body?: unknown) => Promise<Answer>;

/** A stored key as `GET /v1/keys` lists it: setting row, vendor and last 4 characters. */
export type KeyInfo = { setting_id: string; vendor: string; last4: string; updated_at: string };

/** One run of `GET /v1/runs`. */
export type RunSummary = { run_id: string; goal_id: string | null; workflow: string; state: string; created_at: string; updated_at: string };

/** One intake question of the new-goal run. */
export type Question = { id: string; text: string; kind: 'text' | 'number' | 'choice'; choices: string[] };

/** One planned item of a plan proposal, in the plan's local time. */
export type PlanItem = { title: string; start: string; minutes: number; rrule: string | null; notes: string | null };

/** A human node a run waits at, with the state keys it shows. */
export type Waiting = { node: string; ask: 'approval' | 'question'; prompt: string | null; data: Record<string, unknown> };

/** A run's lifecycle status, tagged by `status`; pauses carry their reason. */
export type RunStatus = {
	status: 'queued' | 'running' | 'checkpointed' | 'waiting_human' | 'waiting_budget' | 'done' | 'failed';
	reason?: { kind: string; limit?: string; node?: string };
	error?: string;
};

/** One run event; `body.type` is the event kind. */
export type RunEvent = { seq: number; at_ms: number; body: { type: string; step?: string; error?: string; name?: string } };

/** A run as `GET /v1/runs/{id}` returns it. */
export type RunView = { run_id: string; goal_id: string | null; workflow: string; status: RunStatus; waiting: Waiting[]; events: RunEvent[] };

/** A free-text log waiting in the device queue. */
export type PendingLog = { text: string; at: string };

/** States in which a run still moves on its own, so the app keeps polling it. */
export const ACTIVE = ['queued', 'running', 'checkpointed'];

const STEP_TEXT: Record<string, string> = {
	intake: 'Reading your goal',
	research: 'Researching how to reach it',
	plan: 'Writing the plan',
	revise: 'Revising the plan',
	write: 'Putting the plan on your calendar',
	map: 'Reading your log',
	save: 'Saving the entries',
	adapt: 'Looking at what changed',
	record: 'Writing the proposal',
	apply: 'Applying the change'
};

/** The owner-facing line for a paused or failed run, or null when it is neither. */
export function pauseMessage(status: RunStatus): string | null {
	if (status.status === 'waiting_budget') {
		const limit = status.reason?.limit ?? 'spend';
		if (limit === 'unverified_price') return "Paused: this model's price is not verified, so spend cannot be checked. Pick an Anthropic model in Settings.";
		return `Paused: the ${limit} budget is reached. Raise it in Settings to continue.`;
	}
	if (status.status === 'failed') return `The run stopped: ${status.error ?? 'unknown error'}.`;
	return null;
}

/** Progress lines from run events: one per step, done once the step finished. */
export function progressLines(events: RunEvent[]): { text: string; done: boolean }[] {
	const lines = new Map<string, boolean>();
	for (const e of events) {
		const step = e.body.step;
		if (!step || !STEP_TEXT[step]) continue;
		if (e.body.type === 'step_started' && !lines.has(step)) lines.set(step, false);
		if (e.body.type === 'step_finished') lines.set(step, true);
	}
	return [...lines].map(([step, done]) => ({ text: STEP_TEXT[step], done }));
}

/** The question cards of a `questions` node, empty for any other node. */
export function questionsOf(waiting: Waiting | undefined): Question[] {
	if (waiting?.node !== 'questions') return [];
	const intake = waiting.data.intake as { questions?: Question[] } | undefined;
	return intake?.questions ?? [];
}

/** One plan item as a line: title, local day and time, length and repeat. */
function itemLine(i: PlanItem): string {
	const when = i.start.replace('T', ' ').slice(0, 16);
	return `${i.title}, ${when}, ${i.minutes} min${i.rrule ? `, repeats (${i.rrule})` : ''}`;
}

/**
 * The chat messages for one run view: progress, then what the run waits for (question cards,
 * a plan to approve, a change request prompt, a replan proposal), a pause or failure line, or
 * the done line.
 */
export function chatFromRun(view: RunView): ChatMessage[] {
	const out: ChatMessage[] = [];
	const lines = progressLines(view.events);
	if (lines.length) out.push({ type: 'progress', lines });
	const pause = pauseMessage(view.status);
	if (pause) return [...out, { type: 'message', from: 'persona', name: 'Aroh', text: pause }];
	const w = view.waiting[0];
	if (view.status.status === 'waiting_human' && w) {
		const questions = questionsOf(w);
		if (questions.length) {
			out.push({ type: 'message', from: 'persona', name: 'Intake interviewer', text: w.prompt ?? 'Answer these so the plan fits you.' });
			for (const q of questions) {
				const input: ComponentSpec =
					q.kind === 'number'
						? { kind: 'number_input', label: 'Your answer', unit: 'count', value: null, step: 1 }
						: q.kind === 'choice'
							? { kind: 'choice', label: 'Your answer', options: q.choices, multi: false, value: [] }
							: { kind: 'note', label: 'Your answer', placeholder: 'Type your answer below', value: '' };
				out.push({ type: 'card', spec: { kind: 'question_card', persona: 'Intake interviewer', question: q.text, input } });
			}
		} else if (w.node === 'approve' && w.data.plan) {
			const plan = w.data.plan as { summary: string; items: PlanItem[] };
			out.push({ type: 'plan', title: plan.summary, items: plan.items.map(itemLine) });
		} else if (w.node === 'approve' && w.data.proposal_record) {
			const p = w.data.proposal_record as { summary: string };
			out.push({ type: 'message', from: 'persona', name: 'Adaptation analyst', text: `${p.summary} It is in your Inbox to apply.` });
		} else {
			out.push({ type: 'message', from: 'persona', name: 'Aroh', text: w.prompt ?? 'Your answer is needed.' });
		}
	}
	if (view.status.status === 'done') {
		const text = view.workflow === 'new-goal' ? 'Approved. The plan is on your calendar.' : 'Done.';
		out.push({ type: 'message', from: 'persona', name: 'Aroh', text });
	}
	return out;
}

/** What one chat message does to a goal's runs, as `route` decides it. */
export type Route =
	| { kind: 'answer'; node: string; answer: unknown; held: string[] }
	| { kind: 'hold'; held: string[] }
	| { kind: 'replan'; reason: string };

/**
 * Routes owner text for a goal: with no waiting node it starts a replan with the text as its
 * reason. At `questions` each message answers the next question; after the last one every held
 * answer is posted in question order. At `change` the text is the change. At `approve` the text
 * rejects the plan and is held for the `change` node that follows.
 */
export function route(waiting: Waiting | undefined, held: string[], text: string): Route {
	if (!waiting) return { kind: 'replan', reason: text };
	const questions = questionsOf(waiting);
	if (questions.length) {
		const answers = [...held, text];
		if (answers.length < questions.length) return { kind: 'hold', held: answers };
		return { kind: 'answer', node: 'questions', answer: questions.map((q, i) => ({ id: q.id, answer: answers[i] })), held: [] };
	}
	if (waiting.node === 'change') return { kind: 'answer', node: 'change', answer: text, held: [] };
	if (waiting.node === 'approve') return { kind: 'answer', node: 'approve', answer: false, held: [text] };
	return { kind: 'answer', node: waiting.node, answer: text, held: [] };
}

/** Key status per vendor: the last 4 of the key stored for that vendor's provider row, or null. */
export function keyStatus(keys: KeyInfo[], providers: { id: string; vendor: string }[]): Record<string, string | null> {
	const out: Record<string, string | null> = {};
	for (const p of providers) out[p.vendor] = keys.find((k) => k.setting_id === p.id && k.vendor === p.vendor)?.last4 ?? out[p.vendor] ?? null;
	return out;
}

/** The error text of a failed answer: the engine's message, else the status. */
function failure(a: Answer): string {
	const body = a.body as { error?: { message?: string } } | null;
	return body?.error?.message ?? `The engine answered ${a.status}.`;
}

/**
 * Posts queued logs oldest first to `POST /v1/logs` and stops at the first failure, so order is
 * kept. Returns the logs the engine accepted, which the caller removes from the queue, and the
 * log-intake runs they started.
 */
export async function flushLogs(call: Call, pending: PendingLog[], timezone: string): Promise<{ sent: PendingLog[]; runs: string[] }> {
	const sent: PendingLog[] = [];
	const runs: string[] = [];
	for (const log of pending) {
		try {
			const a = await call('POST', '/v1/logs', { text: log.text, at: log.at, timezone });
			if (a.status !== 202) break;
			sent.push(log);
			runs.push((a.body as { run_id: string }).run_id);
		} catch {
			break;
		}
	}
	return { sent, runs };
}

/** The default transport: the `app_engine` command, which adds the account token in Rust. */
const tauriCall: Call = (method, path, body) => invoke<Answer>('app_engine', { method, path, body: body ?? null });

/** The device's IANA zone, sent with every start so local times are planned in it. */
function zone(): string {
	return Intl.DateTimeFormat().resolvedOptions().timeZone;
}

/**
 * The AI side of the app, served by the server engine. Every method resolves, never throws.
 * The app holds no provider keys: `saveKey` hands the key to Rust, which sends it to the engine
 * and keeps nothing. `call` is injectable for tests.
 */
export class Engine {
	constructor(private call: Call = tauriCall) {}

	/** Runs one call and maps transport errors and error statuses to a result. */
	private async request<T>(method: 'GET' | 'POST' | 'PUT' | 'DELETE', path: string, body?: unknown, want = [200, 202]): Promise<EngineResult<T>> {
		try {
			const a = await this.call(method, path, body);
			if (!want.includes(a.status)) return { ok: false, reason: 'error', message: failure(a) };
			return { ok: true, value: a.body as T };
		} catch (e) {
			const message = String(e);
			if (message.startsWith('needs_engine')) return { ok: false, reason: 'needs_engine', message: NEEDS_ENGINE };
			return { ok: false, reason: 'error', message };
		}
	}

	/** Starts the new-goal run for one line of owner text. */
	startGoal(text: string): Promise<EngineResult<{ goal_id: string; run_id: string }>> {
		return this.request('POST', '/v1/goals', { title: text.trim().slice(0, 200), context: text, timezone: zone() });
	}

	/** Reads a run with the events after `afterSeq`. */
	run(runId: string, afterSeq?: number): Promise<EngineResult<RunView>> {
		return this.request('GET', `/v1/runs/${runId}${afterSeq === undefined ? '' : `?after_seq=${afterSeq}`}`);
	}

	/** Lists the 50 newest runs, optionally of one goal and one state. */
	runs(filter: { goalId?: string; state?: string } = {}): Promise<EngineResult<RunSummary[]>> {
		const q = [filter.goalId && `goal_id=${filter.goalId}`, filter.state && `state=${filter.state}`].filter(Boolean).join('&');
		return this.request('GET', `/v1/runs${q ? `?${q}` : ''}`);
	}

	/** Answers a run's human node. */
	answer(runId: string, node: string, answer: unknown): Promise<EngineResult<{ run_id: string }>> {
		return this.request('POST', `/v1/runs/${runId}/answers`, { node, answer });
	}

	/** Starts a replan of a goal with `reason`. */
	replan(goalId: string, reason: string): Promise<EngineResult<{ goal_id: string; run_id: string }>> {
		return this.request('POST', `/v1/goals/${goalId}/replan`, { reason, timezone: zone() });
	}

	/**
	 * Sends owner text for a goal: answers the goal's waiting run (see `route`) or starts a replan.
	 * Returns the run it went to and the answers still held for the next node.
	 */
	async sendChat(goalId: string, text: string, held: string[] = []): Promise<EngineResult<{ run_id: string | null; held: string[] }>> {
		const waitingRuns = await this.runs({ goalId, state: 'waiting_human' });
		if (!waitingRuns.ok) return waitingRuns;
		const runId = waitingRuns.value[0]?.run_id;
		let waiting: Waiting | undefined;
		if (runId) {
			const view = await this.run(runId);
			if (!view.ok) return view;
			waiting = view.value.waiting[0];
		}
		const r = route(waiting, held, text);
		if (r.kind === 'hold') return { ok: true, value: { run_id: runId ?? null, held: r.held } };
		if (r.kind === 'replan') {
			const started = await this.replan(goalId, r.reason);
			return started.ok ? { ok: true, value: { run_id: started.value.run_id, held: [] } } : started;
		}
		const answered = await this.answer(runId!, r.node, r.answer);
		return answered.ok ? { ok: true, value: { run_id: runId!, held: r.held } } : answered;
	}

	/** Hands a key for `vendor` to the engine through Rust; the app keeps nothing. Returns the last 4. */
	async saveKey(vendor: string, key: string): Promise<EngineResult<KeyInfo>> {
		try {
			const a = await invoke<Answer>('app_save_key', { vendor, key });
			if (a.status !== 200) return { ok: false, reason: 'error', message: failure(a) };
			return { ok: true, value: a.body as KeyInfo };
		} catch (e) {
			const message = String(e);
			return message.startsWith('needs_engine') ? { ok: false, reason: 'needs_engine', message: NEEDS_ENGINE } : { ok: false, reason: 'error', message };
		}
	}

	/** Lists the stored keys: setting row, vendor and last 4 only. */
	keys(): Promise<EngineResult<KeyInfo[]>> {
		return this.request('GET', '/v1/keys');
	}

	/** Runs the engine's one-token key test for a provider row and returns the status line. */
	async testConnection(settingId: string): Promise<EngineResult<string>> {
		const r = await this.request<{ ok: boolean; error: string | null }>('POST', `/v1/keys/${settingId}/test`);
		if (!r.ok) return r;
		return { ok: true, value: r.value.ok ? 'Connected. The engine reached the provider with your key.' : `The provider refused the key: ${r.value.error ?? 'unknown error'}.` };
	}

	/** Posts the queued logs (see `flushLogs`); returns the accepted ones and their runs. */
	flushLogs(pending: PendingLog[]): Promise<{ sent: PendingLog[]; runs: string[] }> {
		return flushLogs(this.call, pending, zone());
	}
}

/** The engine client the app uses. */
export const engine = new Engine();
