import { invoke } from '@tauri-apps/api/core';
import type { ChatMessage } from '$lib/ui/contract';
import type { CoachFields } from '$lib/ui/screens/types';
import { formatYmd, minuteOfDay } from '$lib/ui/format';
import type { Action } from './client';
import { editError, type PersonaView } from './personas';
import { deviceZone } from '$lib/ui/clock';
import type { StartGoalBody } from './newgoal';
import { caching, engineCache, type EngineCache } from './cache';

/** Shown wherever an action needs the server engine (Q3) and the app is not connected to it. */
export const NEEDS_ENGINE = 'This needs a connection to the engine. Sign in and set the server URL in Settings. Everything else works offline.';

/** The outcome of an engine call: a value, or the reason it could not run. */
export type EngineResult<T> = { ok: true; value: T } | { ok: false; reason: 'needs_engine' | 'error'; message: string };

/**
 * Whether a read failed against a configured engine (down, 5xx, network), so a screen must say
 * it could not reach the engine instead of showing an empty or "On track" state. No engine set
 * up (`needs_engine`) and a good read are not unreachable.
 */
export const unreachable = <T>(r: EngineResult<T>): boolean => !r.ok && r.reason === 'error';

/** What the engine answered: HTTP status and JSON body. */
export type Answer = { status: number; body: unknown };

/** One engine request: method, `/v1/...` path and optional JSON body. Rejects with a string. */
export type Call = (method: 'GET' | 'POST' | 'PUT' | 'DELETE', path: string, body?: unknown) => Promise<Answer>;

/** A stored key as `GET /v1/keys` lists it: setting row, vendor and last 4 characters. */
export type KeyInfo = { setting_id: string; vendor: string; last4: string; updated_at: string };

/** One run of `GET /v1/runs`; `personas` are the slugs of the coaches its snapshot holds. */
export type RunSummary = { run_id: string; goal_id: string | null; workflow: string; state: string; created_at: string; updated_at: string; personas?: string[]; plugin_id?: string | null; plugin_version?: number | null };

/** One intake question of the new-goal run. Number questions may carry a unit and slider bounds; runs from before units have none. */
export type Question = {
	id: string;
	text: string;
	kind: 'text' | 'number' | 'choice';
	choices: string[];
	unit?: string | null;
	min?: number | null;
	max?: number | null;
	known?: { fact_id: string; value: string; source: string } | null;
};

/** One planned item of a plan proposal, in the plan's local time. */
export type PlanItem = { title: string; start: string; minutes: number; rrule: string | null; notes: string | null };

/** A human node a run waits at, with the state keys it shows. */
export type Waiting = { node: string; ask: 'approval' | 'question'; prompt: string | null; data: Record<string, unknown> };

/** A run's lifecycle status, tagged by `status`; pauses carry their reason, failed runs whether Retry continues them. */
export type RunStatus = {
	status: 'queued' | 'running' | 'checkpointed' | 'waiting_human' | 'waiting_budget' | 'done' | 'failed';
	reason?: { kind: string; limit?: string; node?: string; limit_ms?: number };
	error?: string;
	retryable?: boolean;
};

/** One run event; `body.type` is the event kind, `body.persona` the coach slug of an agent step's `step_started`. */
export type RunEvent = { seq: number; at_ms: number; body: { type: string; step?: string; persona?: string; error?: string; name?: string } };

/** A run as `GET /v1/runs/{id}` returns it; `summary` is the proposal and its outcome when there is one. */
export type RunView = {
	run_id: string;
	goal_id: string | null;
	workflow: string;
	status: RunStatus;
	waiting: Waiting[];
	events: RunEvent[];
	summary?: string | null;
	usage?: { cost_usd_micros?: number };
	research?: { summary: string; sources: { title: string; url: string }[] } | null;
	answers?: RunAnswer[];
};

/** One submitted question of a run with its answer, null when skipped. */
export type RunAnswer = { id: string; text: string; answer: string | null };

/** One insight of `GET /v1/insights`: what the engine noticed and did without being asked. */
export type Insight = { id: string; goal_id: string | null; run_id: string | null; source: string; title: string; body: string; state: string | null; created_at: string };

/** One device event as `POST /v1/calendar/triage` takes it: ids, calendar, title, times, recurrence. */
export type TriageEvent = { event_id: string; calendar_id: string; calendar: string; title: string; start_at: string; end_at: string; rrule: string | null; all_day: boolean };

/** A free-text log waiting in the device queue. */
export type PendingLog = { text: string; at: string };

/** States in which a run still moves on its own, so the app keeps polling it. */
export const ACTIVE = ['queued', 'running', 'checkpointed'];

/** Workflows that research and plan a goal from its answers: the first run and every Run again. */
export const PLANNING = ['new-goal', 'run-again'];

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
	apply: 'Applying the change',
	analyze: 'Looking at your edits',
	group: 'Grouping your calendar events',
	save_rules: 'Saving your calendar rules'
};

/** The owner-facing line for a paused or failed run, or null when it is neither. */
export function pauseMessage(status: RunStatus): string | null {
	if (status.status === 'waiting_budget') {
		const limit = status.reason?.limit ?? 'spend';
		if (limit === 'unverified_price') return "Paused: this model's price is not verified, so spend cannot be checked. Pick an Anthropic, OpenAI or Google model in Settings.";
		return `Paused: the ${limit} budget is reached. Raise it in Settings to continue.`;
	}
	if (status.status === 'failed') return `The run stopped: ${status.error ?? 'unknown error'}.`;
	return null;
}

/**
 * The owner-facing line for a run its time limit or its owner stopped ("Stopped after 5 min."),
 * or null when the run was not stopped that way. Both can be retried.
 */
export function stopMessage(status: RunStatus): string | null {
	if (status.status !== 'failed') return null;
	if (status.reason?.kind === 'time_limit') return `Stopped after ${Math.max(1, Math.round((status.reason.limit_ms ?? 0) / 60_000))} min.`;
	if (status.reason?.kind === 'cancelled') return 'Stopped. Nothing more runs or is spent.';
	return null;
}

/**
 * The owner-facing line for a run that failed on a transient error (a store or network error,
 * or provider retries used up), which Retry continues from the failed step; null otherwise.
 */
export function retryMessage(status: RunStatus): string | null {
	if (status.status !== 'failed' || !status.retryable || status.reason) return null;
	return pauseMessage(status);
}

/** A spend in micro-dollars as dollars and cents, "$0.01"; zero when unknown. */
export function formatCost(micros: number | undefined): string {
	return `$${((micros ?? 0) / 1_000_000).toFixed(2)}`;
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

/** The question cards of a question node (`questions` or `limit_choice`), empty for any other node. */
export function questionsOf(waiting: Waiting | undefined): Question[] {
	const source = waiting ? QUESTION_SOURCES[waiting.node] : undefined;
	if (!waiting || !source) return [];
	const held = waiting.data[source] as { questions?: Question[] } | undefined;
	return held?.questions ?? [];
}

/** The state key that holds the questions of each question node: intake questions, and the limits card (Q64). */
const QUESTION_SOURCES: Record<string, string> = { questions: 'intake', limit_choice: 'limits' };

/** One plan item as a line: title, local day and time, length and repeat. */
function itemLine(i: PlanItem): string {
	const when = i.start.replace('T', ' ').slice(0, 16);
	return `${i.title}, ${when}, ${i.minutes} min${i.rrule ? `, repeats (${i.rrule})` : ''}`;
}

/**
 * What a waiting run needs from the owner, as the Inbox and the goal show it: "3 questions to
 * answer", "Plan to approve", "Change to approve" or "What to change in the plan". Null when the
 * run waits for nothing.
 */
export function waitLabel(view: RunView): string | null {
	const w = view.waiting[0];
	if (view.status.status !== 'waiting_human' || !w) return null;
	const n = questionsOf(w).length;
	if (n) return n === 1 ? '1 question to answer' : `${n} questions to answer`;
	if (w.node === 'approve' && w.data.plan) return 'Plan to approve';
	if (w.node === 'approve') return 'Change to approve';
	if (w.node === 'change') return 'What to change in the plan';
	return w.prompt ?? 'Your answer';
}

/**
 * The ids of the actions the plan items at indexes `fixed` became: same goal, no series, same
 * title and local start (minute) in the plan's `zone`. Repeating items and items with no
 * written action give none.
 */
export function fixedActionIds(actions: Action[], goalId: string, items: PlanItem[], fixed: number[], zone: string): string[] {
	const local = (iso: string) => {
		const m = minuteOfDay(iso, zone);
		return `${formatYmd(iso, zone)}T${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
	};
	return fixed.flatMap((i) => {
		const item = items[i];
		if (!item || item.rrule) return [];
		const hit = actions.find((a) => a.goal_id === goalId && a.series_id === null && a.title === item.title && local(a.start_at) === item.start.slice(0, 16));
		return hit ? [hit.id] : [];
	});
}

/**
 * The chat messages for one run view: progress, then what the run waits for (a plan to approve,
 * a change request prompt, a replan proposal), a pause or failure line, or the done line. Open
 * questions are not messages here: the chat renders them as one group (see `questions.ts`).
 */
export function chatFromRun(view: RunView): ChatMessage[] {
	const out: ChatMessage[] = [];
	const lines = progressLines(view.events);
	if (lines.length) out.push({ type: 'progress', lines, working: ACTIVE.includes(view.status.status) });
	const stop = stopMessage(view.status) ?? retryMessage(view.status);
	if (stop) return [...out, { type: 'stopped', text: stop, retry: true }];
	if (ACTIVE.includes(view.status.status)) {
		return [...out, { type: 'working', step: lines.findLast((l) => !l.done)?.text ?? null, cost: formatCost(view.usage?.cost_usd_micros) }];
	}
	const pause = pauseMessage(view.status);
	if (pause) return [...out, { type: 'message', from: 'persona', name: 'Aroh', text: pause }];
	const w = view.waiting[0];
	if (view.status.status === 'waiting_human' && w) {
		if (w.node === 'approve' && w.data.plan) {
			const plan = w.data.plan as { summary: string; items: PlanItem[]; timezone?: string };
			const items = plan.items.map(({ title, start, minutes, rrule }) => ({ title, start, minutes, rrule }));
			out.push({ type: 'plan', title: plan.summary, items: plan.items.map(itemLine), fixable: plan.items.map((i) => !i.rrule), plan: { items, timezone: plan.timezone ?? null }, key: `${view.run_id}:${w.node}`, approved: false });
		} else if (w.node === 'approve' && w.data.proposal_record) {
			const p = w.data.proposal_record as { summary: string };
			out.push({ type: 'message', from: 'persona', name: 'Adaptation analyst', text: `${p.summary} It is in your Inbox to apply.` });
		} else if (!questionsOf(w).length) {
			out.push({ type: 'message', from: 'persona', name: 'Aroh', text: w.prompt ?? 'Your answer is needed.' });
		}
	}
	if (view.status.status === 'done') {
		const text = PLANNING.includes(view.workflow) ? 'Approved. The plan is on your calendar.' : 'Done.';
		out.push({ type: 'message', from: 'persona', name: 'Aroh', text });
	}
	return out;
}

/** What one chat message does to a goal's runs, as `route` decides it. */
export type Route = { kind: 'answer'; node: string; answer: unknown; change?: string } | { kind: 'questions' } | { kind: 'replan'; reason: string };

/**
 * Routes owner text for a goal: with no waiting node it starts a replan with the text as its
 * reason. At `questions` the text belongs in the open question group, so nothing is posted. At
 * `change` the text is the change. At `approve` the text rejects the plan and is the change the
 * `change` node that follows gets.
 */
export function route(waiting: Waiting | undefined, text: string): Route {
	if (!waiting) return { kind: 'replan', reason: text };
	if (questionsOf(waiting).length) return { kind: 'questions' };
	if (waiting.node === 'change') return { kind: 'answer', node: 'change', answer: text };
	if (waiting.node === 'approve') return { kind: 'answer', node: 'approve', answer: false, change: text };
	return { kind: 'answer', node: waiting.node, answer: text };
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

/** The failed result of a call that rejected: `needs_engine` when the app is not connected. */
function offline(e: unknown): { ok: false; reason: 'needs_engine' | 'error'; message: string } {
	const message = String(e);
	return message.startsWith('needs_engine') ? { ok: false, reason: 'needs_engine', message: NEEDS_ENGINE } : { ok: false, reason: 'error', message };
}

/** The `GET /v1/runs` path for `filter`: one goal and one state, either or both optional. */
function runsPath(filter: { goalId?: string; state?: string }): string {
	const q = [filter.goalId && `goal_id=${filter.goalId}`, filter.state && `state=${filter.state}`].filter(Boolean).join('&');
	return `/v1/runs${q ? `?${q}` : ''}`;
}

/** The default transport: the `app_engine` command, which adds the account token in Rust. */
const tauriCall: Call = (method, path, body) => invoke<Answer>('app_engine', { method, path, body: body ?? null });

/**
 * The AI side of the app, served by the server engine. Every method resolves, never throws.
 * The app holds no provider keys: `saveKey` hands the key to Rust, which sends it to the engine
 * and keeps nothing. `call` is injectable for tests.
 */
export class Engine {
	private inflight = new Map<string, Promise<EngineResult<unknown>>>();
	private call: Call;

	/** `call` is the transport; with `cache`, allowlisted reads are kept and writes drop what they change (B87). */
	constructor(
		call: Call = tauriCall,
		private cache: EngineCache | null = null
	) {
		this.call = cache ? caching(call, cache) : call;
	}

	/** The kept answer of `GET path`, read synchronously so a page renders it in its first frame (B87). */
	private kept<T>(path: string): T | undefined {
		return this.cache?.get<T>(path);
	}

	/** The ticket of a read starting now (see `EngineCache.current`), for a view built from several reads. */
	ticket(): string | null {
		return this.cache?.current() ?? null;
	}

	/** Keeps `view`, a run's full view merged from several reads started under `ticket`, as its cached view. */
	keepRun(ticket: string | null, view: RunView) {
		this.cache?.put(ticket, `/v1/runs/${view.run_id}`, view);
	}

	/** The last runs list read for `filter`, or undefined. */
	cachedRuns(filter: { goalId?: string; state?: string } = {}): RunSummary[] | undefined {
		return this.kept(runsPath(filter));
	}

	/** The last full view read of run `runId`, or undefined. */
	cachedRun(runId: string): RunView | undefined {
		return this.kept(`/v1/runs/${runId}`);
	}

	/** The last persona library read, or undefined. */
	cachedPersonas(): PersonaView[] | undefined {
		return this.kept('/v1/personas');
	}

	/** The last run time limit read, or undefined. */
	cachedRunLimit(): { minutes: number } | undefined {
		return this.kept('/v1/settings/run-limit');
	}

	/** The last insights read, or undefined. */
	cachedInsights(goalId?: string): Insight[] | undefined {
		return this.kept(`/v1/insights${goalId ? `?goal_id=${goalId}` : ''}`);
	}

	/** Whether the last account read had the AI consent on file, or undefined. */
	cachedConsent(): boolean | undefined {
		const a = this.kept<{ ai_consent_at?: string | null }>('/v1/account');
		return a ? typeof a.ai_consent_at === 'string' && a.ai_consent_at !== '' : undefined;
	}

	/** What each waiting run of `runs` waits for, from the kept run views only (see `waits`). */
	cachedWaits(runs: RunSummary[]): Record<string, string> {
		const out: Record<string, string> = {};
		for (const r of runs.filter((x) => x.state === 'waiting_human')) {
			const v = this.cachedRun(r.run_id);
			const label = v ? waitLabel(v) : null;
			if (label) out[r.run_id] = label;
		}
		return out;
	}

	/**
	 * Runs one call and maps transport errors and error statuses to a result. A GET of a path
	 * already in flight shares that request instead of sending a second one (B87).
	 */
	private request<T>(method: 'GET' | 'POST' | 'PUT' | 'DELETE', path: string, body?: unknown, want = [200, 202]): Promise<EngineResult<T>> {
		if (method !== 'GET') return this.send<T>(method, path, body, want);
		const held = this.inflight.get(path);
		if (held) return held as Promise<EngineResult<T>>;
		const sent = this.send<T>(method, path, body, want).finally(() => this.inflight.delete(path));
		this.inflight.set(path, sent);
		return sent;
	}

	/** Sends one call and maps transport errors and error statuses to a result. */
	private async send<T>(method: 'GET' | 'POST' | 'PUT' | 'DELETE', path: string, body: unknown, want: number[]): Promise<EngineResult<T>> {
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

	/** Creates the goal from the New Goal form (`startBody`) and starts its new-goal run. */
	startGoal(body: StartGoalBody): Promise<EngineResult<{ goal_id: string; run_id: string }>> {
		return this.request('POST', '/v1/goals', body);
	}

	/** Reads a run with the events after `afterSeq`. */
	run(runId: string, afterSeq?: number): Promise<EngineResult<RunView>> {
		return this.request('GET', `/v1/runs/${runId}${afterSeq === undefined ? '' : `?after_seq=${afterSeq}`}`);
	}

	/** Lists the 50 newest runs, optionally of one goal and one state. */
	runs(filter: { goalId?: string; state?: string } = {}): Promise<EngineResult<RunSummary[]>> {
		return this.request('GET', runsPath(filter));
	}

	/** What each `waiting_human` run of `runs` waits for (`waitLabel`), by run id; unread runs are left out. */
	async waits(runs: RunSummary[]): Promise<Record<string, string>> {
		const waiting = runs.filter((r) => r.state === 'waiting_human');
		const views = await Promise.all(waiting.map((r) => this.run(r.run_id)));
		const out: Record<string, string> = {};
		views.forEach((v, i) => {
			const label = v.ok ? waitLabel(v.value) : null;
			if (label) out[waiting[i].run_id] = label;
		});
		return out;
	}

	/** Lists the 30 newest insights, optionally of one goal. */
	insights(goalId?: string): Promise<EngineResult<Insight[]>> {
		return this.request('GET', `/v1/insights${goalId ? `?goal_id=${goalId}` : ''}`);
	}

	/** Stops a run now: nothing more runs or is spent until `retry`. Stopping a finished run is a no-op. */
	cancel(runId: string): Promise<EngineResult<{ run_id: string }>> {
		return this.request('POST', `/v1/runs/${runId}/cancel`);
	}

	/**
	 * Plans goal `goalId` again (Q74): fresh research and plan with its saved answers, or with
	 * `answers` (all of them, edited) when given; the new plan waits for approval.
	 */
	runAgain(goalId: string, answers?: RunAnswer[]): Promise<EngineResult<{ goal_id: string; run_id: string }>> {
		const body = answers ? { timezone: deviceZone(), answers: answers.map(({ id, answer }) => ({ id, answer })) } : { timezone: deviceZone() };
		return this.request('POST', `/v1/goals/${goalId}/run-again`, body);
	}

	/** Continues a stopped run, or one failed on a transient error, where it stopped, with a fresh time limit. */
	retry(runId: string): Promise<EngineResult<{ run_id: string }>> {
		return this.request('POST', `/v1/runs/${runId}/retry`);
	}

	/** Reads the owner's run time limit: minutes of active work per run. */
	runLimit(): Promise<EngineResult<{ minutes: number }>> {
		return this.request('GET', '/v1/settings/run-limit');
	}

	/** Saves the owner's run time limit, 1 to 120 minutes, for runs started or retried after. */
	setRunLimit(minutes: number): Promise<EngineResult<{ minutes: number }>> {
		return this.request('PUT', '/v1/settings/run-limit', { minutes });
	}

	/** Whether the owner's AI consent is on file: `ai_consent_at` of `GET /v1/account` is set. */
	async consent(): Promise<EngineResult<boolean>> {
		const r = await this.request<{ ai_consent_at?: string | null }>('GET', '/v1/account');
		return r.ok ? { ok: true, value: typeof r.value.ai_consent_at === 'string' && r.value.ai_consent_at !== '' } : r;
	}

	/**
	 * Records the owner's consent to send goal text to their AI provider (`PUT /v1/account/consent`),
	 * required before the first agent run. Resolves true when stored, false when this server has no
	 * account endpoints (404), so older self-hosted servers keep working.
	 */
	async giveConsent(): Promise<EngineResult<boolean>> {
		try {
			const a = await this.call('PUT', '/v1/account/consent', { ai: true });
			if (a.status === 404) return { ok: true, value: false };
			if (a.status !== 200 && a.status !== 201 && a.status !== 204) return { ok: false, reason: 'error', message: failure(a) };
			return { ok: true, value: true };
		} catch (e) {
			return offline(e);
		}
	}

	/** Asks the server to delete the account (15-day grace, Q76): 202 with the purge date; the app signs out after. */
	deleteAccount(): Promise<EngineResult<{ purge_after: string }>> {
		return this.request('DELETE', '/v1/account', undefined, [202]);
	}

	/** Answers a run's human node. */
	answer(runId: string, node: string, answer: unknown): Promise<EngineResult<{ run_id: string }>> {
		return this.request('POST', `/v1/runs/${runId}/answers`, { node, answer });
	}

	/** Starts a replan of a goal with `reason`. */
	replan(goalId: string, reason: string): Promise<EngineResult<{ goal_id: string; run_id: string }>> {
		return this.request('POST', `/v1/goals/${goalId}/replan`, { reason, timezone: deviceZone() });
	}

	/** Reads where the replan after the newest owner edit of action `actionId` stands (Q77). */
	editReplan(actionId: string): Promise<EngineResult<import('./replan').EditReplan>> {
		return this.request('GET', `/v1/actions/${actionId}/replan`);
	}

	/** Reads the replan a miss of action `actionId` started, if any (Q78). */
	missReplan(actionId: string): Promise<EngineResult<{ run_id: string | null }>> {
		return this.request('GET', `/v1/actions/${actionId}/miss-replan`);
	}

	/** Starts the card coach on action `actionId` for `scope` with the owner's `text` (Q71). */
	cardChat(actionId: string, scope: 'this' | 'future', text: string): Promise<EngineResult<{ goal_id: string; run_id: string }>> {
		return this.request('POST', `/v1/actions/${actionId}/chat`, { scope, text });
	}

	/**
	 * Sends owner text for a goal (see `route`): answers the goal's waiting run or starts a replan.
	 * Returns the run it went to, the change text to hand the `change` node next, and whether the
	 * run waits at questions, where the text fills the question group instead.
	 */
	async sendChat(goalId: string, text: string): Promise<EngineResult<{ run_id: string | null; change?: string; questions?: boolean }>> {
		const waitingRuns = await this.runs({ goalId, state: 'waiting_human' });
		if (!waitingRuns.ok) return waitingRuns;
		const runId = waitingRuns.value[0]?.run_id;
		let waiting: Waiting | undefined;
		if (runId) {
			const view = await this.run(runId);
			if (!view.ok) return view;
			waiting = view.value.waiting[0];
		}
		const r = route(waiting, text);
		if (r.kind === 'questions') return { ok: true, value: { run_id: runId!, questions: true } };
		if (r.kind === 'replan') {
			const started = await this.replan(goalId, r.reason);
			return started.ok ? { ok: true, value: { run_id: started.value.run_id } } : started;
		}
		const answered = await this.answer(runId!, r.node, r.answer);
		return answered.ok ? { ok: true, value: { run_id: runId!, change: r.change } } : answered;
	}

	/** Starts a calendar-triage run for unmatched device events (at most 300, one per series). */
	triage(events: TriageEvent[]): Promise<EngineResult<{ goal_id: null; run_id: string }>> {
		return this.request('POST', '/v1/calendar/triage', { timezone: deviceZone(), events });
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

	/** Lists every coach (persona): built-ins, replaced by the owner's library copy when there is one. */
	personas(): Promise<EngineResult<PersonaView[]>> {
		return this.request('GET', '/v1/personas');
	}

	/** Copies built-in persona `name` into the owner's library; `exists` when a copy is already there. */
	async copyPersona(name: string): Promise<EngineResult<PersonaView> | { ok: false; reason: 'exists'; message: string }> {
		try {
			const a = await this.call('POST', `/v1/personas/${encodeURIComponent(name)}/copy`);
			if (a.status === 201) return { ok: true, value: a.body as PersonaView };
			return { ok: false, reason: a.status === 409 ? 'exists' : 'error', message: failure(a) };
		} catch (e) {
			return offline(e);
		}
	}

	/** Saves the edited fields of the owner's copy of `name`; a refusal comes back as owner text (`editError`). */
	async editPersona(name: string, fields: CoachFields): Promise<EngineResult<PersonaView>> {
		try {
			const a = await this.call('PUT', `/v1/personas/${encodeURIComponent(name)}`, fields);
			if (a.status === 200) return { ok: true, value: a.body as PersonaView };
			return { ok: false, reason: 'error', message: editError(failure(a)) };
		} catch (e) {
			return offline(e);
		}
	}

	/** Posts the queued logs (see `flushLogs`); returns the accepted ones and their runs. */
	flushLogs(pending: PendingLog[]): Promise<{ sent: PendingLog[]; runs: string[] }> {
		return flushLogs(this.call, pending, deviceZone());
	}
}

/** The engine client the app uses, with its reads kept in the device cache. */
export const engine = new Engine(tauriCall, engineCache);
