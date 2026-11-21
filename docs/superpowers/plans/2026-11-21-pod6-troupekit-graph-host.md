# Pod 6: troupekit-graph and troupekit-host

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** `troupekit-graph`, durable workflows defined in TOML (agent, tool, human, router, join and end nodes, edges, fan-out and join, a typed shared run state) run by `GraphRun`, a serializable state machine the host steps and checkpoints; and `troupekit-host`, a `JobQueue` with leases plus a `Runner` that claims a job, loads the latest checkpoint, runs one step, saves it and re-enqueues or finishes, in worker mode (a loop) and step mode (one `tick()` for serverless hosts). A run killed mid-way resumes from its last checkpoint and finishes with the same result.

**Architecture:** `GraphRun` is plain data (workflow snapshot, limits, state values, branches, status, usage, event `seq`, step number, format version) with `#[serde(deny_unknown_fields)]`, like `AgentRun`. Everything live sits behind `GraphIo` (an `AgentIo`, a persona loader and the router conditions). `step(&dyn GraphIo)` does one unit of work per active branch (one `AgentRun` step or one tool call); control nodes (router, join, end, human entry) resolve inside the step that reaches them. The host saves `checkpoint()` after every step; `CheckpointStore::save` accepts only step + 1, so two workers can never both advance a run (Q11). The host crate owns scheduling only: `JobQueue` (one job per run, leased), `Runner`, `EventLog`. Step mode follows Q26 and Q27, the architecture page's step-mode tick and run lifecycle.

**Tech Stack:** Rust 2021, async-trait, serde, serde_json, thiserror 2, toml 1, futures-util 0.3 (`join_all`), tokio (`time`, `sync`, `macros` in host; tests on `rt-multi-thread`), schemars 1 (tests only). State types reuse `troupekit_agent::{compile_output, validate}`, router conditions reuse `troupekit_agent::dsl::parse_call`.

## Global Constraints

- Lives in `troupekit/`; no file may contain the host app's name (`troupekit/scripts/check-boundary.sh`, case-insensitive, including the lock file and fixtures).
- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. No `#` comments in TOML. No em dashes.
- `assert!` for internal invariants; validation returns `Err`.
- One copy of every helper. Reuse `troupekit_core::{store, event, ids, clock, error, inmem}`, `troupekit_agent::{AgentRun, AgentIo, Persona, PersonaLoader, Limits, StepOutcome, compile_output, validate, dsl, persona::valid_name, RUN_FORMAT}`, `troupekit_tools::{ToolContext, ToolRegistry, valid_tool_name, fence_untrusted}`, `troupekit_providers::{Model, CompletionRequest, CompletionResponse}`.
- `troupekit/Cargo.toml` is not edited; dependencies go in the crate manifests. The updated `troupekit/Cargo.lock` is committed.
- No live network in tests: `FnModel` (a model that is a pure function of the request) and typed tools with counters.
- Files written or changed by this pod are `rustfmt --edition 2021` clean (local `rustfmt` only; no local cargo builds).
- Tests in Kiri. Inner loop: `kiri run --rm -v "$PWD/troupekit:/src" -v tk-p6-target:/target -v tk-p6-cargo:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/target -e CARGO_PROFILE_DEV_DEBUG=0 -w /src rust:1-slim cargo test -p <crate>`. Final: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p6 troupekit`, `kiri run --rm troupekit-test-p6`, then `kiri rmi troupekit-test-p6` and remove the `tk-p6-*` volumes. Check `df` before each build (the disk is nearly full); delete only what this pod built.
- Branch `feat/troupekit-graph-host`, conventional commits, no co-author trailer, commit and never push.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Workflow file | TOML: `name` (persona name rule), `start`, optional `max_steps` (default 200, 1 to 10000), `max_retries` (default 5, 0 to 50), `[state]` (key to type in the persona `[output]` shorthand), `[[nodes]]` (`id`, `kind`, kind fields, `reads`, `writes`), `[[edges]]` (`from`, `to`, `when` only on router edges). `deny_unknown_fields` everywhere; at most 256 nodes | one shorthand for typed data (Q4), bounded size |
| Node kinds | `agent` (`persona`, optional `prompt`, `reads`, `writes`), `tool` (`tool`, `reads` as the argument object, `writes`), `human` (`ask = "approval"` or `"question"`, optional `prompt`, `reads` shown to the human, `writes`; approval writes a `bool`), `router` (`condition = "name(args)"`, no reads or writes), `join`, `end` | brief; every field a kind does not use is an error naming the node |
| Run state | a JSON object; keys declared in `[state]`; every write (input, agent output, tool output, human answer) is validated against its key's compiled schema, a mismatch fails the run (or, for input and answers, returns `Err`); reading an unset key fails the run | shared typed state, strict like personas |
| Agent input | one user message: the node `prompt`, then the read keys as a JSON object inside `fence_untrusted("workflow state", ..)`; an agent node needs a prompt or reads | state may carry web text from earlier nodes; built-in guardrails already treat fences as data |
| Tool output | content parsed as JSON, else kept as a string; `is_error` or a `ToolError` fails the run | a failed side effect must not pass silently |
| Validation (parse) | unknown node (start, edge ends); unreachable node (BFS from `start` over all edges); cycle without a router (any cycle in the graph with router nodes removed); missing join (from a fan-out node, each branch follows agent, tool or human nodes of out-degree 1 and must reach a `join` before any end, router or second fan-out; all branches reach the same join; that join has no other inputs; every join closes exactly one fan-out); router edges all have unique `when` labels, other edges none; end has no out-edges, other nodes at least one; no duplicate edges; at least one end; reads and writes declared; two branches of one fan-out never write the same key. Errors are `WorkflowError { file, field, message }` | the four brief rules spelled out, plus the cheap ones |
| Validation (setup) | `GraphRun::new` loads every persona, and checks every tool and condition is registered, and the input keys and types, before any step | like `AgentRun::new`: fail at setup, not mid-run |
| Workflow snapshot | the parsed workflow is stored in the run state | a job needs only the run id; a deploy that edits the TOML never changes a running run |
| Step | `step` does one unit of work for every branch that can work, concurrently (`join_all`): one `AgentRun::step` or one tool call. Results apply in branch order. Before and after the work the run settles: control nodes resolve in the same step, a human node pauses the branch, an end node finishes the run. One branch is exactly one node step | real parallel branches; deterministic merge |
| Statuses | `queued`, `running` (only inside `step`; a checkpoint never holds it, asserted), `checkpointed`, `waiting_human { reason }`, `waiting_budget { reason }`, `done`, `failed { error }`, matching the run-lifecycle diagram. Waiting is reported when no branch can work: a human wait before a budget wait, first branch first | the architecture page |
| Outcomes | `Continue`, `RetryAt { at_ms, error }`, `Waiting(PauseReason)`, `Done(state)`, `Failed(error)`. Stepping a waiting or final run returns its outcome and changes nothing, so the host does not save | the host maps each to queue calls |
| Step numbers | every durable change bumps the step: `step` (including one that returns `RetryAt`), `resume` and `raise_budget`; checkpoints are `step` numbered and the store refuses all but step + 1 | an answer saved through the same next-step rule cannot be applied twice |
| Nested agent | the branch holds the `AgentRun` as serde data (inside the graph's own format check); a new `AgentRun` per node entry, so a node in a loop starts fresh. Its events pass through a renumbering sink: graph run id, graph `seq`, step names prefixed `node/`, its own `run_started`, `run_finished`, `run_paused` and `run_failed` dropped (the graph emits its own) | the host knows only the graph run id; `seq` stays contiguous for `(run_id, seq)` dedupe |
| Graph budget | `GraphRun` holds `Limits`. Each agent node gets them with `run` reduced by the graph's spend so far. Agent `Paused(Budget)` makes the branch wait; `raise_budget(limits)` replaces the limits and resumes every budget-paused branch with the new remaining run budget. Concurrent branches check their budgets independently, so they can overshoot a limit by one call estimate per extra branch | Q32 "per run" means the graph run |
| Retry | agent `Retry` keeps the branch as is; the step returns `RetryAt { now + retry_after_ms or min(1000 * 2^n, 300000) }`. More than `max_retries` retrying steps in a row fail the run (`error after N retries`); a step with no retry resets the count | "running to failed: error after retries" |
| Max steps | reaching `max_steps` fails the run | a router loop cannot spin forever |
| Human resume | `resume(node, answer)` needs a branch waiting at human node `node`, validates the answer against the node's `writes` key, writes it, moves the branch on at the next step and sets `queued`. `pending_human()` lists `{ node, ask, prompt, data }` for the host's UI | `PauseReason` names the node; several branches may wait |
| Conditions | `Conditions` maps a name to `Fn(&Map, &[Value]) -> Result<String, String>`, returning an edge label; built-in `flag(key)` gives `"true"` or `"false"` for a bool key. An unknown label or an error fails the run | deterministic routing from state |
| Idempotency | `ToolContext` gains `idempotency_key: Option<String>`; one constructor `ToolContext::for_call(run_id, step, index)` builds `"{run_id}:{step}:{index}"`. The agent uses it for every call of a tool batch (its own run id, the step being run, call order); the graph tool node uses the graph run id, the graph step and the branch index. A replayed step gives the same keys, the next step new ones | at-least-once tools become safe for tools that honour the key |
| Events | emitted live during the step (text deltas must stream); a replayed step reuses the same `seq` values; `EventLog` (host) upserts on `(run_id, seq)` | brief; matches pod 5's open item |
| GraphIo | trait: `agent() -> &dyn AgentIo`, `async persona(name) -> Persona`, `conditions()`. `SimpleGraphIo { agent: Arc<dyn AgentIo>, personas: PersonaLoader, conditions }` | everything live outside the run, like `AgentIo` |
| Test fake | `troupekit_graph::testing::FnModel`: a `Model` from a closure over the request, streaming its text as one delta | the crash test needs a model whose reply does not depend on how often it was called; one fake for graph and host |
| JobQueue | trait over `RunId` jobs, one live job per run: `enqueue(run_id, due_ms)` (pending: earlier due wins; leased: remembered and applied when the lease ends), `claim_due(limit, lease_ms) -> Vec<Lease>` (due pending jobs and expired leases, earliest first, fresh token each), `complete`, `fail(error)`, `reschedule(due_ms)`. A stale lease token returns `CoreError::Conflict` and changes nothing. `InMemQueue::new(clock)` | lease expiry recovers crashed workers; tokens stop a stale worker from finishing someone else's job |
| Runner | `enqueue(&GraphRun)` saves step 0 (when absent) and enqueues now. `process(&Lease)`: load latest checkpoint (missing or unreadable fails the job), `step`, save only when the step number moved; save `Conflict` returns `Lost` and leaves the job alone; then `Continue` reschedules now, `RetryAt` reschedules at that time, `Waiting` and `Done` complete, `Failed` fails the job. Store errors reschedule after 1 s and return the error. `resume` and `raise_budget` load, apply, save, enqueue | the step-mode tick sequence diagram |
| Step mode | `tick(TickOptions { max_jobs: 16, budget_ms: 250_000 })` claims up to the remaining job count, processes the batch concurrently, and repeats until nothing is due, `max_jobs` are processed or the time budget has passed (checked before each claim). Lease default 300 s | Vercel caps a request at 300 s (Q26 risk) |
| Worker mode | `work(WorkOptions { tick, idle_ms: 1000 }, shutdown: watch::Receiver<bool>)` loops on `tick`; sleeps `idle_ms` when nothing was processed or a claim failed; returns once `shutdown` is true, after the tick in progress | graceful shutdown on deploy |
| HTTP | none in the framework: `enqueue`, `tick`, `resume`, `raise_budget` are plain async methods the host app wraps | brief |

---

## File Structure

| Path | Responsibility |
|---|---|
| `troupekit/crates/troupekit-tools/src/tool.rs` | `ToolContext::idempotency_key`, `ToolContext::for_call` |
| `troupekit/crates/troupekit-agent/src/run.rs` | tool batch calls carry `for_call` keys |
| `troupekit/crates/troupekit-graph/Cargo.toml` | crate manifest |
| `.../src/lib.rs` | module list and re-exports |
| `.../src/error.rs` | `GraphError`, `WorkflowError` |
| `.../src/workflow.rs` | `Workflow`, `Node`, `NodeKind`, `Ask`, `Edge`, `Workflow::parse`, validation |
| `.../src/condition.rs` | `Conditions`, `ConditionFn`, built-in `flag` |
| `.../src/io.rs` | `GraphIo`, `SimpleGraphIo`, the renumbering node io |
| `.../src/run.rs` | `GraphRun`, `GraphStatus`, `GraphOutcome`, `Branch`, `HumanAsk`, `GRAPH_FORMAT` |
| `.../src/testing.rs` | `FnModel` |
| `.../tests/workflow.rs` | parse and validation errors |
| `.../tests/run.rs` | runs over fake models and tools |
| `troupekit/crates/troupekit-host/Cargo.toml` | crate manifest |
| `.../src/lib.rs` | module list and re-exports |
| `.../src/error.rs` | `HostError` |
| `.../src/queue.rs` | `JobQueue`, `Lease`, `InMemQueue`, `JobState` |
| `.../src/events.rs` | `EventLog` |
| `.../src/runner.rs` | `Runner`, `Processed`, `TickOptions`, `TickReport`, `WorkOptions` |
| `.../tests/runner.rs` | step mode, worker mode, resume, crash and concurrency tests |
| `troupekit/README.md` | `troupekit-graph` and `troupekit-host` rows set to available |

---

### Task 0: Idempotency keys for tool calls

**Files:** Modify `troupekit-tools/src/tool.rs`, `troupekit-tools/src/typed.rs` (struct literal), `troupekit-agent/src/run.rs`, `troupekit-agent/tests/run.rs`.

**Interfaces:** `pub struct ToolContext { pub run_id: Option<RunId>, pub idempotency_key: Option<String> }`; `ToolContext::for_call(run_id: RunId, step: u64, index: usize) -> ToolContext`.

- [x] **Step 1: Failing tests:** tools `for_call_builds_stable_keys`; agent `tool_calls_carry_idempotency_keys` (a batch of two calls gets `"{run}:{step}:0"` and `":1"`; replaying the same checkpoint gives the same keys; a later tool step gives new ones).
- [x] **Step 2:** Run `cargo test -p troupekit-tools -p troupekit-agent`. Expected: compile failure (`for_call`, `idempotency_key` missing).
- [x] **Step 3 to 4:** implement, pass.
- [x] **Step 5:** `git commit -m "feat(troupekit-tools): idempotency keys on tool calls"`

### Task 1: Graph crate and workflow TOML validation

**Files:** Create `troupekit-graph/Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/workflow.rs`, `src/condition.rs`, `tests/workflow.rs`.

**Interfaces:**
- `pub struct WorkflowError { pub file, pub field, pub message }`; `pub enum GraphError { Workflow, Setup(String), Checkpoint(String), Resume(String), Agent(AgentError), Store(CoreError) }`.
- `pub struct Workflow { pub name, pub start, pub max_steps: u32, pub max_retries: u32, pub state: BTreeMap<String, Value>, pub nodes: Vec<Node>, pub edges: Vec<Edge> }`, `Workflow::parse(file, text) -> Result<Workflow, WorkflowError>`, `node(id)`, `next(id) -> Vec<&Edge>`.
- `pub struct Node { pub id, pub kind: NodeKind, pub reads: Vec<String>, pub writes: Option<String> }`, `pub enum NodeKind { Agent { persona, prompt }, Tool { tool }, Human { ask: Ask, prompt }, Router { condition: Call }, Join, End }`, `pub enum Ask { Approval, Question }`, `pub struct Edge { pub from, pub to, pub when: Option<String> }`.
- `pub type ConditionFn = Arc<dyn Fn(&Map<String, Value>, &[Value]) -> Result<String, String> + Send + Sync>`; `Conditions::new()` (with `flag`), `register(name, f) -> Result<(), GraphError>`, `get(name)`.

- [x] **Step 1: Failing tests:** `parses_full_workflow`, `rejects_unknown_node`, `rejects_unreachable_node`, `rejects_cycle_without_router`, `accepts_cycle_through_router`, `rejects_missing_join` (branch reaches end; branches meet at different joins; join with an outside input; join closing no fan-out), `rejects_bad_fields` (unknown field, field of another kind, undeclared read or write, approval writing a non-bool, router edge without `when`, plain edge with `when`, duplicate labels, end with out-edges, node without out-edges, no end, bad `max_steps`, duplicate node id, bad persona name, bad tool name, bad condition, two branches writing one key, a TOML syntax error), `flag_condition_routes_on_bool`.
- [x] **Step 2:** Run `cargo test -p troupekit-graph`. Expected: compile failure.
- [x] **Step 3 to 4:** implement, pass.
- [x] **Step 5:** `git commit -m "feat(troupekit-graph): workflow toml with validation"`

### Task 2: GraphRun over agent and tool nodes

**Files:** Create `src/io.rs`, `src/run.rs`, `src/testing.rs`, `tests/run.rs`.

**Interfaces:**
- `#[async_trait] pub trait GraphIo: Send + Sync { fn agent(&self) -> &dyn AgentIo; async fn persona(&self, name: &str) -> Result<Persona, AgentError>; fn conditions(&self) -> &Conditions; }`, `SimpleGraphIo`.
- `GraphRun::new(workflow, input: Map<String, Value>, limits, io) -> Result<GraphRun, GraphError>` (async), `step(&mut self, io: &dyn GraphIo) -> GraphOutcome`, `checkpoint()`, `from_checkpoint(&Checkpoint)`, `run_id()`, `status()`, `state()`, `usage()`, `steps()`.
- `pub struct FnModel` with `FnModel::new(id, f)`.

- [x] **Step 1: Failing tests:** `linear_agent_then_tool_finishes` (state written, events contiguous, step names prefixed, one `run_started` and one `run_finished`), `agent_input_is_fenced_state`, `tool_node_gets_reads_and_a_key`, `tool_error_fails_the_run`, `bad_output_type_fails_the_run`, `unset_read_fails_the_run`, `new_checks_personas_tools_conditions_and_input`, `checkpoint_every_step_matches_uninterrupted`, `checkpoint_refuses_other_formats`, `final_run_steps_without_change`.
- [x] **Step 2:** Run. Expected: compile failure.
- [x] **Step 3 to 4:** implement, pass.
- [x] **Step 5:** `git commit -m "feat(troupekit-graph): checkpointable graph run over agent and tool nodes"`

### Task 3: Human nodes, routers, fan-out and join, budgets, retries

**Files:** Modify `src/run.rs`, `tests/run.rs`.

**Interfaces:** `resume(node, answer: Value) -> Result<(), GraphError>`, `raise_budget(limits) -> Result<(), GraphError>`, `pending_human() -> Vec<HumanAsk>`.

- [x] **Step 1: Failing tests:** `approval_pauses_and_resumes`, `question_answer_is_typed`, `rejection_loops_back_through_router`, `resume_needs_a_waiting_node`, `max_steps_stops_a_loop`, `fan_out_runs_branches_concurrently_and_joins` (two agent branches on a barrier under a 2 s timeout), `branch_waits_for_human_while_other_runs`, `budget_pause_becomes_waiting_budget_and_raise_resumes`, `graph_run_budget_spans_nodes`, `retry_reschedules_and_gives_up`, `paused_run_survives_checkpoint`.
- [x] **Step 2:** Run. Expected: failures on the missing methods.
- [x] **Step 3 to 4:** implement, pass.
- [x] **Step 5:** `git commit -m "feat(troupekit-graph): human nodes, routers, fan-out and join, budgets and retries"`

### Task 4: Host queue, event log and runner

**Files:** Create `troupekit-host/Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/queue.rs`, `src/events.rs`, `src/runner.rs`, `tests/runner.rs`.

**Interfaces:**
- `pub struct Lease { pub run_id: RunId, pub token: u64, pub attempts: u32 }`; `#[async_trait] pub trait JobQueue: Send + Sync { enqueue, claim_due, complete, fail, reschedule }`; `InMemQueue::new(Arc<dyn Clock>)`, `job(run_id) -> Option<JobState>`.
- `EventLog` (`EventSink`), `events(run_id) -> Vec<Event>`.
- `Runner::new(queue, checkpoints, io)`, `with_lease_ms`, `enqueue`, `process`, `tick`, `work`, `resume`, `raise_budget`; `pub enum Processed { Stepped(GraphOutcome), Unchanged(GraphOutcome), Lost }`.

- [x] **Step 1: Failing tests:** queue unit tests (`one_job_per_run`, `claims_due_in_order_with_leases`, `expired_lease_is_reclaimed`, `stale_token_changes_nothing`, `enqueue_while_leased_applies_on_complete`); runner tests `tick_runs_a_workflow_to_done`, `tick_respects_max_jobs_and_budget`, `human_pause_completes_job_and_resume_requeues`, `retry_reschedules_with_not_before`, `failed_run_fails_the_job`, `events_dedupe_on_run_and_seq`, `worker_mode_stops_on_shutdown`.
- [x] **Step 2:** Run `cargo test -p troupekit-host`. Expected: compile failure.
- [x] **Step 3 to 4:** implement, pass.
- [x] **Step 5:** `git commit -m "feat(troupekit-host): job queue with leases, runner, step and worker modes"`

### Task 5: Crash and concurrency proofs

**Files:** Modify `troupekit-host/tests/runner.rs`.

- [x] **Step 1: Tests:** `crash_mid_step_resumes_from_last_checkpoint` (a runner steps a claimed run and is dropped without saving; the clock passes the lease; a new runner on the same stores finishes with the same final state as an uninterrupted run, the replayed tool call reuses its idempotency key, and the event log has the same contiguous `seq` set), `crash_between_steps_resumes`, `two_runners_never_advance_a_run_twice` (multi-thread runtime, several runs, two runners ticking at once: model and tool call counts equal the single-runner counts, every run done), `stale_runner_loses_the_save` (lease expires mid-step, a second runner advances the run, the first gets `Lost` and leaves the job alone).
- [x] **Step 2:** Run. Expected: pass, or a fix in the runner with a red run first.
- [x] **Step 3:** `git commit -m "test(troupekit-host): crash resume and two-runner concurrency"`

### Task 6: Full Kiri run, lock, README, DONE

- [x] `troupekit/Cargo.lock` only adds entries.
- [x] README rows `troupekit-graph` and `troupekit-host` set to available.
- [x] rustfmt on touched files.
- [x] Full Kiri run, then `kiri rmi troupekit-test-p6` and remove the `tk-p6-*` volumes. Expected: `boundary ok`, every test passes.
- [x] DONE line with counts, boundary output, deviations and open items.

## Pod 5 open items

| Item | Where |
|---|---|
| graph saves the agent checkpoint after every step | here: the agent run is embedded in the graph state, saved every step |
| `Paused(Budget)` and `Retry` | here: `waiting_budget` with `raise_budget`; `RetryAt` rescheduled by the runner |
| idempotency keys, events upsert on `(run_id, seq)` | here: `ToolContext::for_call`, `EventLog`; Postgres upsert is pod 9 |
| hosts hand tools a `GuardedHttp` | pod 9 |
| host `AgentIo`: role aliases, per-user keys, Postgres `UsageLedger`, `run_events` | pod 9 |
| `unverified_price` only clears with no limits | pod 9 (a verified price override per model) |
| ledger written only under set `scope` and `user` keys | pod 9 (always pass a goal `scope`) |
| hand-edited state can trip an `assert!` | unchanged; the graph keeps `deny_unknown_fields` and both format checks |
| function guardrails, persona `extends` | follow-up |

## Deviations

| Deviation | Reason |
|---|---|
| No "expected compile failure" run for Tasks 1 to 5: tests and code were written together, then run once (the checked Step 2 boxes mean the step was covered, not run red). Tasks 1 to 3 landed in one commit (`9225bc1`). Task 0 did run red first | volumes were removed after every run, so each red run was a cold build; `lib.rs` and `run.rs` were built together and split commits would not compile |
| `troupekit_agent::persona::toml_error` is now public | workflow files reuse its field naming for TOML errors (one copy) |
| `Workflow::check_value` wraps a key's schema in a one-field object before `validate` | `validate` compiles only object schemas |
| `troupekit_graph::testing` also holds `FnTool` (async closure tool, any-object schema) and `text_reply` | graph and host tests share one tool fake and one reply builder |
| Task 5 tests live in `troupekit-host/tests/crash.rs`, fixtures in `tests/common/mod.rs` | separate commit per task, one copy of the fixtures |
| Extra validation: branch nodes have in-degree 1, `flag(key)` must name a declared bool key, `reads` may not repeat, human nodes need `writes` | a router loop-back into a branch would park a join forever; cheap parse-time checks |
| `settle` stops after `2 * nodes + 4` passes and fails the run | a loop of routers only would otherwise spin inside one step |
| Tool nodes emit `<node>/tool` step events and tool-call events with the idempotency key as `call_id` | one key ties replayed calls together in the event log |
| `Processed::Lost` is also returned when the queue call after a successful save hits a stale lease; `TickReport` carries `lost` and `errors` | the job belongs to another worker then |
| `planner_model`, `LINEAR` and `PRICE` exist in both `troupekit-graph/tests/run.rs` and `troupekit-host/tests/common/mod.rs` | test data across two crates; moving it into the public `testing` module would ship fixtures to users |
| Kiri volumes were removed after every inner-loop run (coordinator), so each run was a cold build | disk pressure |

## Open items for pod 9

| Item | Where |
|---|---|
| Postgres `JobQueue` (leases with a token column, `SKIP LOCKED` claims), `CheckpointStore` and `run_events` upsert on `(run_id, seq)` | pod 9 |
| HTTP routes wrapping `enqueue`, `tick`, `resume`, `raise_budget`; the step-mode cron | pod 9 |
| host `GraphIo`: per-user `AgentIo`, personas from the database, host conditions | pod 9 |
| a `RetryAt` from one branch delays every branch of the run | follow-up if it matters in practice |
| concurrent branches can overshoot the graph run budget by one call estimate per extra branch | accepted (decision table) |
| personas are loaded at setup and again at node entry, so a persona edit reaches a running run (only the workflow is snapshotted) | follow-up: snapshot personas at setup |
| a hand-edited checkpoint can trip an `assert!` (for example a join that can never close) | unchanged from pod 5 |
| `Runner::store_failed` ignores a failed reschedule; lease expiry recovers the job | pod 9 with the durable queue |
| a crashed attempt that emitted more events than its replay (streamed deltas can differ) leaves extra `seq` rows past the run's final event; the durable log should drop rows above the saved checkpoint's `seq` on replay | pod 9 |
