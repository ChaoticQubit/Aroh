# Pod 5: troupekit-agent

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `troupekit-agent`, the agent layer of Troupekit: persona TOML files loaded from layered sources and strictly validated, and an `AgentRun` serializable state machine that calls a model, runs allowed tools concurrently, checks typed output and evals with one repair round, enforces layered budgets and emits core events, so a host can checkpoint after every step. Before that, one transport abstraction in `troupekit-core` replaces the two copies in providers and tools.

**Architecture:** `AgentRun` is plain data (persona snapshot, messages, state, counters, usage, event `seq`, step number, format version) with `#[serde(deny_unknown_fields)]`. Everything live sits behind the `AgentIo` trait: model resolution with its `Price`, the tool registry, the eval registry, the usage ledger, the event sink and the clock. The host calls `step(&dyn AgentIo)` until a terminal or paused outcome and saves `checkpoint()` after every step (rig `AgentRun` style, driven by the caller, Q11). Personas follow testy's format (`role`, `goal`, `instructions`, `context`, `guardrails`, `evals`, `[output]` shorthand), Q4, Q9, Q33. Budgets follow Q32.

**Tech Stack:** Rust 2021, async-trait, serde, serde_json, thiserror 2, toml 1, futures-util 0.3 (`join_all`, already in the lock), schemars 1 and tokio (tests only). Schema validation reuses `troupekit_tools::{compile_schema, schema_errors}`, so the agent crate has no `jsonschema` dependency of its own.

## Global Constraints

- Lives in `troupekit/`; no file may contain the host app's name (`troupekit/scripts/check-boundary.sh`), including built-in personas and the lock file.
- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. No `#` comments in TOML. No em dashes.
- `assert!` for internal invariants; validation returns `Err`.
- One copy of every helper. Reuse `troupekit_core::{message, event, store, clock, ids, error, http}`, `troupekit_providers::{Model, CompletionRequest, Price, valid_model_id}`, `troupekit_tools::{ToolRegistry, ToolContext, ToolOutput, compile_schema, schema_errors, valid_tool_name, fence_untrusted}`.
- `troupekit/Cargo.toml` is not edited; dependencies go in the crate manifests. The updated `troupekit/Cargo.lock` is committed.
- No live network in tests: fake models, fake tools, `ScriptedTransport`.
- Files written or changed by this pod are `rustfmt --edition 2021` clean; older core files keep their style.
- Tests in Kiri: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p5 troupekit` then `kiri run --rm troupekit-test-p5`, then `kiri rmi troupekit-test-p5` (disk is nearly full). Inner loop: `kiri run --rm -v "$PWD/troupekit:/src" -v tk-p5-target:/target -v tk-p5-cargo:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/target -w /src rust:1-slim cargo test -p <crate>`; remove both volumes when done.
- Branch `feat/troupekit-agent`, conventional commits, no co-author trailer, commit and never push.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Transport home | `troupekit_core::http`: `Method`, `HttpRequest`, `HttpResponse`, `Body`, `ChunkBody`, `Transport`, `HttpError`, `HttpErrorKind`, `read_limited`, `media_type`. Pure types and one trait, no I/O. `ReqwestTransport` and `ScriptedTransport` stay in providers, `GuardedHttp` and `FakeHttp` stay in tools | the lowest crate both need; one copy |
| Request shape | superset: `method`, `url`, `headers`, `body: Option<Vec<u8>>`, `accept: Vec<String>`; `HttpRequest::get(url)` and `HttpRequest::post(url, headers, body)`; Debug prints header names only | providers only POST, tools need methods and an accept list |
| Response shape | `status`, final `url`, `headers`, streaming `body: Box<dyn Body>`, `truncated`; `header(name)` and `content_type()` helpers | providers stream; tools need final URL, media type and the truncation flag |
| Guarded body | `GuardedHttp` still reads the body eagerly inside its total timeout and size cap, then hands back a `ChunkBody` with `truncated` set; tools read answers with `receive(&dyn Transport, req) -> Received` | a lazily streamed body would move both limits outside the guard |
| Transport error | `HttpError { kind, message }`, kinds `Invalid`, `Blocked`, `Network`, `Timeout`, `Rejected`. `From<HttpError> for ProviderError` in core: `Timeout` to `Stalled`, `Network` to `Network`, `Invalid`, `Blocked` and `Rejected` to `BadRequest` (not retryable). `From<HttpError> for ToolError` in tools: `Invalid` to `InvalidArgs`, `Blocked` to `Blocked`, else `Http`. Messages unchanged | core cannot name `ToolError`; a refused address must never be retried |
| Method on the wire | `Method::as_str()` in core; `ReqwestTransport` and `GuardedHttp` both build `reqwest::Method::from_bytes(m.as_str())` | `ReqwestTransport` hard-coded POST; no second match table |
| Old paths | `troupekit_providers::transport` and `troupekit_tools::http` re-export the core types. Public API change: the tools name `HttpTransport` becomes `Transport`, `Scripted::Fail` takes `HttpError` | existing imports keep compiling; one name per type |
| Tools additions | `registry.rs` exports `compile_schema`, `schema_errors` and `valid_tool_name` (the registry's own checks, now public); `tool.rs` gains `From<HttpError> for ToolError`; `http.rs` gains `Received` and `receive` | one copy of schema validation and tool-name rules |
| Test edits | struct literals and error matches in existing tests change mechanically (`body` is `Option`, errors are `HttpError`, `send` becomes `receive`); assertions keep their meaning; per-crate counts compared before and after | "keep all existing tests green" |
| Run driver | `AgentRun::step(&mut self, io: &dyn AgentIo) -> StepOutcome`, async; one model call or one tool batch per step; the host saves `checkpoint()` after every step whatever the outcome; `drive(run, io, store)` does that loop | caller-driven, checkpointable through core `CheckpointStore` |
| States | `CallModel`, `CallTools { calls }`, `Check { answer }`, `Repair { errors }`, `Done { output }`, `Failed { error }`, `Paused { reason, resume }` | `Check` keeps judge model calls in their own pausable step; one more state than the brief lists |
| Outcomes | `Continue`, `Retry { error, retry_after_ms }` (retryable provider error or ledger read failure, state unchanged), `Paused(PauseReason)`, `Done(Value)`, `Failed(String)`. Stepping a terminal or paused run returns the same outcome and changes nothing | a transient outage must not kill a durable run |
| Max turns | total agent model calls, the repair call included; default 8, persona range 2 to 50; reaching it is `Failed("max_turns 8 reached")`. Judge calls do not count toward `max_turns`; they count toward usage and budgets | rig's default of 1 breaks tool loops; judges are not turns |
| Max tokens | persona `max_tokens`, default 4096, range 1 to 200000; the output side of every budget estimate | a tight `call_usd` needs a smaller cap (4096 tokens at 20 USD per million output tokens is 0.08 USD) |
| Format | `RUN_FORMAT = 1`, checked by `AgentRun::from_checkpoint` with the checkpoint's run id and step; unknown fields refused | safe resume across deploys |
| Tool batches | calls of one turn run with `futures_util::future::join_all`; results keep call order in one `Role::Tool` message (all four wire encoders accept several `ToolResult` parts). A crash mid batch re-runs the batch on resume: at-least-once | ordered history, concurrent I/O |
| Allowed tools | only allowed specs go to the model; a call to any other name gets an error result and the tool is never invoked; unknown persona tools fail at `AgentRun::new` | guardrail enforced before any call (Q33) |
| Typed output | `[output]` shorthand compiles to strict JSON Schema; sent as `output_schema` and also written into the system prompt (encoders drop it under `Unsupported` and `JsonObject`). The answer is parsed with `parse_json` (one surrounding code fence is stripped); not JSON is a validation failure. Without `[output]` the output is the answer text as a JSON string | Q33 strict typed output |
| Repair | one repair round per run, shared by schema, eval and judge failures; it is a model call; a second failure is `Failed` with every reason | Q33 |
| Evals | `EvalRegistry` maps a name to `Fn(&Value, &[Value]) -> Result<(), String>`; built-in `min_items(path, n)`; the host registers domain evals (`no_overlap`, ...). `judge('criteria', min_score[, 'model'])` asks a model for `{score, reason}` JSON, score 0 to 10, the answer fenced as untrusted. Persona evals are parsed at load; names and judge models are resolved at `AgentRun::new`. Evals run only on schema-valid output | deterministic first, judge optional |
| Eval DSL | `name(arg, ...)`: args are quoted strings, numbers or bare words (`[A-Za-z0-9_./-]+`, paths like `actions.minutes`); a path walks objects and maps over arrays; `is_ident` is the one `[a-z_][a-z0-9_]*` rule | testy section 11.1, without `!` and kwargs |
| Guardrails | plain text sentences, appended to the system prompt under `Rules:` | brief; function guardrails are a follow-up |
| Budget layers | `Limits { call, run, scope, user }`, each optional; `AgentRun::new` applies `limits.with_budget(&persona.budget)`, so each of `call` and `run` is the stricter of host and persona; `scope` and `user` are `PeriodLimit { key, max_usd_micros }` given by the host. The ledger defines the period (month, day) | no date math in the framework (Q32) |
| Budget check | before every model call (agent and judge): estimate = `Price::cost_usd_micros` of input tokens `ceil(bytes / 4)` of system, messages JSON and tool specs JSON, plus `max_tokens` output tokens. Pause when `estimate > call`, `run_spent + estimate > run`, `ledger(scope) + estimate > scope`, `ledger(user) + estimate > user`. Names: `call`, `run`, `scope`, `user` | worst case, never under-counts |
| Unverified price | when any limit is set and the price is not verified, the run pauses with limit `unverified_price` | pod 3 P9: an unverified zero is not free |
| Actual cost | after each call the run prices the reply with the handle's `Price` (idempotent with `MeteredModel`), adds it to run usage and records it in the ledger under the scope and user keys that are set | one pricing function (`Price::cost_usd_micros`) |
| Resume | `resume(limits)` is valid only when paused; it replaces the limits and returns to the paused state; the next step re-checks | an owner approval raises a limit |
| Events | `EventSink::emit(&self, Event)` is synchronous so the stream delta closure can call it. First step: `RunStarted { workflow: persona name }`. Every step: `StepStarted`, `StepFinished { usage }`; `Delta::TextDelta` becomes `TextDelta`; tools emit `ToolCallStarted` and `ToolCallFinished` when they run; terminal `RunFinished`, `RunFailed`, `RunPaused`. `seq` lives in the run, so it stays contiguous across a resume | pod 3 open item; one event enum |
| AgentIo | trait: `async model(selector) -> ModelHandle { model, price }`, `tools()`, `evals()`, `ledger()`, `events()`, `clock()`. `SimpleIo` is a field-holding implementation for tests and simple hosts | `Box<dyn Model>` hides `MeteredModel::price()`; secrets never enter run state |
| Persona sources | `PersonaSource` async trait (`label()`, `read(path) -> Option<String>`), `DirSource`, `MemorySource`; `PersonaLoader` tries sources in order (scoped, user, built-in) and the first hit wins, reading `instructions_file` from the same source; `builtin()` is a `MemorySource` of the crate's `personas/*.toml` | Q10 folders are virtual |
| Persona validation | `deny_unknown_fields`; name `[a-z0-9-]{1,64}` checked before any path join and equal to the file stem; `instructions` xor `instructions_file`, the file relative, no `..`, no leading `/`; model is `vendor/model` (`valid_model_id`) or a role alias `[a-z][a-z0-9_-]*`; tools by `valid_tool_name`, no repeats; every error is `PersonaError { file, field, message }`, TOML type errors named by the key on the error's line | agent-generated personas need strict checks (Q9) |
| Built-ins | `assistant` (no tools) and `researcher` (`web_search`, `web_fetch`, typed `summary` and `sources`), model alias `default` | brief |

---

## File Structure

| Path | Responsibility |
|---|---|
| `troupekit/crates/troupekit-core/src/http.rs` | the one transport abstraction |
| `troupekit/crates/troupekit-providers/src/transport.rs` | re-exports, `read_body`, `ScriptedTransport` |
| `troupekit/crates/troupekit-providers/src/reqwest_transport.rs` | honours the method, maps to `HttpError` |
| `troupekit/crates/troupekit-tools/src/http.rs` | re-exports, `GuardedHttp`, `Received`, `receive`, `FakeHttp` |
| `troupekit/crates/troupekit-tools/src/registry.rs` | `compile_schema`, `schema_errors`, `valid_tool_name` made public |
| `troupekit/crates/troupekit-tools/src/tool.rs` | `From<HttpError> for ToolError` |
| `troupekit/crates/troupekit-agent/Cargo.toml` | crate manifest |
| `.../src/lib.rs` | module list and re-exports |
| `.../src/error.rs` | `AgentError`, `PersonaError` |
| `.../src/schema.rs` | `[output]` shorthand to strict JSON Schema, `validate`, `parse_json` |
| `.../src/dsl.rs` | `Call`, `parse_call`, `is_ident` |
| `.../src/persona.rs` | `Persona`, `Budget`, `parse`, `with_instructions`, `system_prompt`, `valid_name`, `valid_relative_path` |
| `.../src/loader.rs` | `PersonaSource`, `DirSource`, `MemorySource`, `PersonaLoader`, `builtin` |
| `.../src/eval.rs` | `EvalRegistry`, `EvalFn`, `lookup`, `judge_request`, `judge_verdict`, `repair_message` |
| `.../src/budget.rs` | `Limits`, `PeriodLimit`, `UsageLedger`, `InMemoryLedger`, `estimate`, `check`, `record` |
| `.../src/io.rs` | `AgentIo`, `ModelHandle`, `EventSink`, `VecSink`, `SimpleIo` |
| `.../src/run.rs` | `AgentRun`, `RunState`, `StepOutcome`, `PendingCall`, `drive`, `RUN_FORMAT` |
| `.../personas/assistant.toml`, `.../personas/researcher.toml` | built-in personas |
| `.../tests/run.rs` | end-to-end runs over fake models and tools |
| `troupekit/README.md` | `troupekit-agent` row set to available |

---

### Task 0: One transport abstraction in core

**Files:** Create `troupekit-core/src/http.rs`. Modify `troupekit-core/src/lib.rs`, providers `transport.rs`, `reqwest_transport.rs`, `model.rs`, providers tests, tools `http.rs`, `fetch.rs`, `search.rs`, `declarative.rs`, `text.rs`, `tool.rs`, `lib.rs`.

**Interfaces (core):**
- `pub enum Method { Get, Post, Put, Patch, Delete }` (Deserialize upper case) with `as_str()`.
- `pub struct HttpRequest { pub method, pub url, pub headers: Vec<(String, String)>, pub body: Option<Vec<u8>>, pub accept: Vec<String> }`, `get(url)`, `post(url, headers, body)`, `header(name)`; Debug hides header values.
- `#[async_trait] pub trait Body: Send { async fn chunk(&mut self) -> Result<Option<Vec<u8>>, HttpError>; }`, `pub struct ChunkBody` with `new(Vec<Vec<u8>>)`.
- `pub struct HttpResponse { pub status: u16, pub url: String, pub headers, pub body: Box<dyn Body>, pub truncated: bool }`, `header(name)`, `content_type()`.
- `#[async_trait] pub trait Transport: Send + Sync { async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError>; }`
- `pub struct HttpError { pub kind: HttpErrorKind, pub message: String }` with `new(kind, message)`; `impl From<HttpError> for ProviderError`.
- `pub async fn read_limited(body: &mut dyn Body, limit: usize) -> Result<(Vec<u8>, bool), HttpError>`: bytes up to `limit` and whether more arrived. `pub fn media_type(value) -> String`.

**Tools:** `pub struct Received { status, url, content_type, body: Vec<u8>, truncated }`, `pub async fn receive(http: &dyn Transport, req) -> Result<Received, ToolError>` (reads with `HttpLimits::default().max_body_bytes` as a backstop); `fetch`, `search`, `declarative` use it; `response_text(&Received)`.

- [x] **Step 1: Failing tests** in `core/src/http.rs`: `request_debug_hides_header_values`, `content_type_is_lowercase_media_type`, `read_limited_flags_overflow`, `http_error_maps_to_provider_kinds`. In providers `tests/reqwest_transport.rs`: `honours_the_request_method` (a PUT reaches the loopback server as PUT).
- [x] **Step 2 to 4:** written with the code (no separate red run), then all green.
- [x] **Step 5:** `git commit -m "refactor(troupekit-core): one http transport abstraction for providers and tools"`

### Task 1: Crate, output schema shorthand, eval DSL

**Files:** Create `Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/schema.rs`, `src/dsl.rs`.

**Interfaces:**
- `pub fn compile_output(fields: &toml::Table) -> Result<Value, String>`: `string`, `int`, `number`, `bool`, `list`, `list<T>`, `{a, b: T}`, `enum(x, y)`, trailing `?` nullable; every object strict (`additionalProperties: false`, all properties `required`).
- `pub fn validate(schema: &Value, value: &Value) -> Vec<String>` over `troupekit_tools::{compile_schema, schema_errors}`; `pub fn parse_json(text) -> Result<Value, String>`.
- `pub struct Call { pub name: String, pub args: Vec<Value> }`, `pub fn parse_call(src: &str) -> Result<Call, String>`, `pub fn is_ident(s) -> bool`.

- [x] **Step 1: Tests:** `compiles_testy_shorthand`, `nullable_and_enum`, `rejects_bad_types`, `parse_json_strips_one_fence`, `validate_lists_errors`; `parses_calls`, `rejects_bad_calls`.
- [x] **Step 2 to 4:** written with the code, then green.
- [x] **Step 5:** `git commit -m "feat(troupekit-agent): output schema shorthand and eval call parser"`

### Task 2: Persona TOML and loader

**Files:** Create `src/persona.rs`, `src/loader.rs`, `personas/assistant.toml`, `personas/researcher.toml`.

**Interfaces:**
- `pub struct Persona { name, display_name, role, goal, instructions, instructions_file, context, model, tools, guardrails, evals: Vec<Call>, output: Option<Value>, max_turns: u32, max_tokens: u32, budget: Budget, source: String }`; `Persona::parse(file, text) -> Result<Persona, PersonaError>` (with `instructions_file` set, `instructions` stays empty), `with_instructions(text)`, `system_prompt()`.
- `pub struct Budget { pub call_usd_micros: Option<u64>, pub run_usd_micros: Option<u64> }` from TOML `budget = { call_usd = 0.05, run_usd = 0.5 }`.
- `#[async_trait] pub trait PersonaSource: Send + Sync { fn label(&self) -> &str; async fn read(&self, path: &str) -> Result<Option<String>, AgentError>; }`, `DirSource::new(dir)`, `MemorySource::new(label, files)`, `builtin()`, `PersonaLoader::new(Vec<Arc<dyn PersonaSource>>)`, `async load(name) -> Result<Persona, AgentError>`.

- [x] **Step 1: Tests:** `parses_full_persona`, `system_prompt_order`, `errors_name_file_and_field` (unknown field, missing role, bad model, `max_turns = 1`, `max_turns = "8"`, `max_tokens = 0`, `tools = "x"`, a syntax error, negative budget, bad eval calls, bad output type, repeated tool, bad name, unsafe `instructions_file`, both instruction keys), `instructions_file_is_relative_only`, `lookup_order_scoped_user_builtin`, `rejects_unsafe_names_before_io`, `name_must_equal_file_stem`, `reads_instructions_file_from_same_source`, `dir_source_reads_files`, `builtin_personas_load`.
- [x] **Step 2 to 4:** written with the code, then green.
- [x] **Step 5:** `git commit -m "feat(troupekit-agent): persona toml with layered loader"`

### Task 3: Evals and budgets

**Files:** Create `src/eval.rs`, `src/budget.rs`.

**Interfaces:**
- `pub type EvalFn = Arc<dyn Fn(&Value, &[Value]) -> Result<(), String> + Send + Sync>`; `EvalRegistry::new()` (with `min_items`), `register(name, f) -> Result<(), AgentError>`, `get(name)`; `lookup(value, path)`; `judge_request(criteria, answer) -> CompletionRequest`, `judge_verdict(text, min_score) -> Result<(), String>`, `repair_message(errors) -> Message`.
- `pub struct PeriodLimit { pub key: String, pub max_usd_micros: u64 }`, `pub struct Limits { pub call, pub run: Option<u64>, pub scope, pub user: Option<PeriodLimit> }`, `Limits::with_budget(self, &Budget)`, `is_empty()`.
- `#[async_trait] pub trait UsageLedger: Send + Sync { async fn period_total(&self, key) -> Result<u64, CoreError>; async fn record(&self, key, usage: Usage) -> Result<(), CoreError>; }`, `InMemoryLedger`.
- `estimate(req, price) -> u64`; `async check(limits, price, run_spent, estimate, ledger) -> Result<Option<&'static str>, CoreError>`; `async record(limits, ledger, usage)`.

- [x] **Step 1: Tests:** `min_items_counts_path`, `register_rejects_duplicates`, `judge_verdict_scores`, `judge_request_fences_the_answer`; `estimate_is_worst_case`, `each_layer_pauses_by_name`, `persona_budget_only_tightens`, `unverified_price_pauses_only_with_limits`, `ledger_sums_per_key`.
- [x] **Step 2 to 4:** written with the code, then green.
- [x] **Step 5:** `git commit -m "feat(troupekit-agent): eval registry, judge and layered budgets"`

### Task 4: AgentRun state machine

**Files:** Create `src/io.rs`, `src/run.rs`, `tests/run.rs`.

**Interfaces:**
- `pub struct ModelHandle { pub model: Arc<dyn Model>, pub price: Price }`; `#[async_trait] pub trait AgentIo: Send + Sync { async fn model(&self, selector: &str) -> Result<ModelHandle, AgentError>; fn tools(&self) -> &ToolRegistry; fn evals(&self) -> &EvalRegistry; fn ledger(&self) -> &dyn UsageLedger; fn events(&self) -> &dyn EventSink; fn clock(&self) -> &dyn Clock; }`; `pub trait EventSink: Send + Sync { fn emit(&self, event: Event); }`, `VecSink`, `SimpleIo::new(tools).with_model(selector, model, price)`.
- `AgentRun::new(persona, input: Vec<Message>, limits, io) -> Result<AgentRun, AgentError>` (async, checks model, judge models, tools, evals), `step`, `resume(limits)`, `run_id()`, `state()`, `usage()`, `messages()`, `model_calls()`, `checkpoint() -> Checkpoint`, `from_checkpoint(&Checkpoint) -> Result<AgentRun, AgentError>`; `pub async fn drive(run, io, store) -> Result<StepOutcome, AgentError>`.

- [x] **Step 1: Failing tests** (`tests/run.rs`, fake `Model` scripted per call, fake tools with call counters): `answers_without_tools_and_streams_events`, `tool_turn_then_answer`, `tools_run_concurrently_in_order` (two tools on one barrier under a 2 s timeout), `disallowed_tool_is_never_called`, `schema_failure_repairs_once`, `repair_then_eval_failure_fails`, `non_json_is_a_validation_failure`, `judge_eval_runs_and_counts_usage`, `low_judge_score_repairs`, `max_turns_counts_model_calls`, `budget_pauses_and_resumes` (run, call, `unverified_price`, user), `checkpoint_every_step_matches_uninterrupted`, `paused_run_survives_checkpoint_and_resumes`, `max_tokens_fits_a_tight_call_budget`, `checkpoint_refuses_other_formats`, `retryable_error_keeps_state`, `unknown_tool_or_eval_fails_at_new`, `http_model_end_to_end` (`HttpModel` over `ScriptedTransport`, OpenAI stream).
- [x] **Step 2:** Run. Result: compile failure (`troupekit_agent::io` and `run` missing).
- [x] **Step 3 to 4:** implement, pass.
- [x] **Step 5:** `git commit -m "feat(troupekit-agent): checkpointable agent run state machine"` (the README row landed in this commit)

### Task 5: Full Kiri run, lock, README, DONE

- [x] `troupekit/Cargo.lock` only adds entries.
- [x] README row `troupekit-agent` set to available.
- [x] Review fixes: one copy of the identifier, tool-name and media-type rules; `Blocked` and `Rejected` not retryable; span-named TOML errors; persona `max_tokens`; paused checkpoint round trip; rustfmt on touched files.
- [x] Full Kiri run, then `kiri rmi troupekit-test-p5` and remove the `tk-p5-*` volumes. Expected: `boundary ok`, every test passes.
- [x] DONE line with counts, boundary output, deviations and open items.

## Deviations

- Tasks 0 to 3 had tests and code written together; only Task 4 had a separate failing run first.
- `Check` is one state more than the brief lists; it holds judge calls so they can pause on budget.
- `max_tokens` is a persona field the brief did not list; without it a tight `call_usd` can never pass.
- Existing provider and tool tests were edited mechanically for the new types; their assertions are unchanged.
- Public API: `troupekit_tools::HttpTransport` is now `Transport`, `Scripted::Fail` takes `HttpError`, `HttpError` kinds `Blocked` and `Rejected` map to `BadRequest`.

## Open items for pod 6 (graph and host)

- The unguarded `ReqwestTransport` now type-checks as a tool transport; hosts must hand tools a `GuardedHttp`.
- A host `AgentIo` resolves role aliases (built-ins use `default`) and per-user keys, implements `UsageLedger` on Postgres with the calendar month as period, and writes events to `run_events`.
- The graph saves `AgentRun::checkpoint()` after every step and maps `StepOutcome::Paused(Budget)` to an approval node; `Retry` is rescheduled after `retry_after_ms`.
- `unverified_price` clears only by resuming with no limits at all, which also drops the scope and user caps; a host may prefer a verified price override per model.
- The ledger is written only under the `scope` and `user` keys that are set, so spend tracking per goal needs a `scope` limit.
- Tool batches and events are at-least-once across a crash: a crash between a step and its save re-runs the step and reuses its `seq` values. Tools with side effects need idempotency keys (`ToolContext` carries `run_id` only); event stores should upsert on `(run_id, seq)`.
- `from_checkpoint` checks format, run id and step only. A hand-edited state that breaks an invariant (an empty tool batch, a judge without criteria) panics on an `assert!` or `expect` in `step`.
- Function guardrails (`public_urls_only`, health and finance safety rules) and `extends` for personas are follow-ups.
