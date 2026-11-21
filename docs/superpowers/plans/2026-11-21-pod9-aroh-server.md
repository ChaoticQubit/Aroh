# Pod 9: aroh-server, the server engine

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** `crates/aroh-server`, one binary that runs all AI work for Aroh (Q1, Q3): an authenticated HTTPS API with a committed OpenAPI document, a key vault that encrypts provider keys at rest and decrypts them for one call only, Postgres implementations of the Troupekit store traits, a per-run host io that resolves the user's provider, key, tools and personas, the new-goal and replan workflows as TOML with Aroh personas, and worker and step modes. Proved by integration tests on a real Postgres and one end-to-end test from a new goal to written actions.

**Architecture:** The binary is a thin `main.rs` over a library. `Engine` owns the pool, the vault, the catalog, the transports and the clock. Troupekit's `Runner` holds one `GraphIo` and the io traits carry no user, so the engine runs its own claim loop: it claims leases from `PgQueue`, reads each run's `runs` row (user, goal, persona snapshot) and builds a `RunIo` and a `Runner` for that one lease, then calls `Runner::process`. Worker mode loops that tick; step mode runs it once per `POST /v1/tick`. Every store derives `user_id` from the `runs` row in SQL. Events are buffered during a step and written in the same transaction as the next checkpoint, with the step + 1 guard, the stale-tail delete and the `runs.state` update, so a step-mode request returns only after its events are durable. Domain rows are written through one generic Postgres writer driven by `aroh_domain::rows::Record` (registry columns, `validate()`, HLC stamp), always with the run's `user_id`.

**Tech Stack:** Rust 2021, axum 0.8, tokio, sqlx 0.8 (postgres, rustls, runtime queries only), aes-gcm 0.10, base64, jsonwebtoken 9 (HS256 secret and JWKS), utoipa 5 (OpenAPI from Rust types, YAML), schemars 1 (typed tools), chrono, chrono-tz, uuid. Troupekit crates by path. Tests: Postgres 17 with pgvector (`pgvector/pgvector:pg17`) as a Kiri sidecar, reqwest against the bound server, Troupekit fake transports.

## Global Constraints

- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. TOML and SQL carry no comments. No em dashes.
- `assert!` for internal invariants; input validation returns `Err`.
- One copy of every helper. Reuse `troupekit_core::{secret::Secret, store, event, ids, clock, error, http}`, `troupekit_providers::{Catalog, Price, MeteredModel, HttpModel, ReqwestTransport}`, `troupekit_tools::{GuardedHttp, web_fetch, BraveSearch, SearxngSearch, SearchToolkit, typed_tool, ToolRegistry}`, `troupekit_agent::{AgentIo, EventSink, Limits, PeriodLimit, UsageLedger, EvalRegistry, PersonaLoader, MemorySource, builtin, eval::lookup}`, `troupekit_graph::{Workflow, GraphRun, GraphIo, Conditions}`, `troupekit_host::{Runner, JobQueue, Lease}`, `aroh_domain::{rows, schema, seed, recur, hlc, derived_id}`.
- Keys never appear in logs, SQLite, sync payloads, API responses, events, checkpoints or test snapshots. A key lives in a `Secret` from decryption to the provider request.
- `troupekit/` and `apps/` are not touched. Framework gaps are open items.
- Tests in Kiri. `HOME` is already `/Users/chaoticqubit` in agent shells and the sandbox refuses a `HOME=` prefix, so commands call plain `kiri`. Inner loop: a `pgvector/pgvector:pg17` sidecar on a Kiri network plus `kiri run --rm --network <net> -v "$PWD:/src" -v aroh-p9-target:/target -v aroh-p9-cargo:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/target -e DATABASE_URL=... -w /src rust:1-slim cargo test -p aroh-server -- --include-ignored`. Final: `sh crates/aroh-server/tests/run.sh` (compose: Postgres with the stub auth schema and every migration, then the test image). Remove the pod's images, volumes and network after each run.
- Branch `feat/aroh-server`, conventional commits, no co-author trailer, commit and never push.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Crate | `crates/aroh-server`, lib plus bin `aroh-server`, in the root workspace, troupekit crates by path (`../../troupekit/crates/...`) | the root workspace already holds every `crates/*`; troupekit stays its own workspace |
| Run modes | `aroh-server serve` (step mode: API only, work happens in `POST /v1/tick`), `aroh-server worker` (API plus the tick loop in the same process), `aroh-server openapi` (prints the OpenAPI YAML). One `Engine::tick` serves both modes | Q26, Q27: one engine core, two run modes |
| Per-run io | `Runner` takes one `Arc<dyn GraphIo>` and `AgentIo::model(selector)` has no user, so the engine owns the claim loop: `PgQueue::claim_due`, then per lease read the `runs` row, build `RunIo` (user, goal, persona snapshot, tools closed over the user) and `Runner::new(queue, checkpoints, io).process(&lease)`. Tick limits copy `TickOptions` (16 jobs, 250 s) | the framework has no io factory per lease (open item) |
| Store tenancy | `CheckpointStore::save`, `JobQueue::enqueue` and event rows carry only a run id; the `runs` row is inserted before `Runner::enqueue`, and every insert takes `user_id` from `runs` in SQL | the tables require `user_id`, the traits cannot pass it |
| Schema | One additive migration `supabase/migrations/20261122000000_server_engine.sql`: `jobs.run_id` (unique, references `runs`), `jobs.lease_token`, `jobs.next_run_after`, `jobs.error`, sequence `aroh_lease_seq`; `runs.personas jsonb`; `provider_credentials.vendor`, `provider_credentials.last4`; table `user_keys` (wrapped data key per user) with RLS on and no client grants. `runs.job_id` stays unused | pod 8's tables lack a run key, a lease token, a deferred due time, a wrapped data key, a key hint and a persona snapshot |
| Checkpoints | `PgCheckpoints::save` in one transaction: lock the `runs` row, require `step == max(step) + 1` (or 0 with none), insert, upsert the buffered events on `(run_id, seq)`, delete events with `seq >= state.seq` (the stale tail of a crashed attempt), set `runs.state` from `state.status.status`. A wrong step returns `CoreError::Conflict`, a missing run `NotFound` | Q11 step + 1 rule; pod 6 open items on upsert and the stale tail |
| Queue | `PgQueue::new(pool, clock)`: one job per run (`jobs.run_id` unique); `enqueue` upserts (queued: earlier due wins; running: remembered in `next_run_after`; done or failed: queued again); `claim_due` selects due queued jobs and expired leases with `FOR UPDATE SKIP LOCKED`, sets `running`, a fresh `nextval('aroh_lease_seq')` token, `locked_until`, `attempts + 1`; `complete`, `fail`, `reschedule` match on `run_id` and token and return `Conflict` for a stale one. Times come from the injected `Clock` | same semantics as `InMemQueue`, tested the same way; no sleeps in lease tests |
| Usage | `UsageModel` wraps every resolved model and writes one `usage` row per successful call (vendor, model, tokens, cost, run, goal, `occurred_at` from the clock). `PgLedger::period_total` sums `usage.cost_usd_micros` for the calendar month (UTC) of the clock's now: key `goal:<uuid>` by goal, `user:<uuid>` by user. `PgLedger::record` writes nothing | `budget::record` calls `record` once per set key and carries no vendor or model, so the call site writes the single row |
| Limits | Built at run start from the user's `budgets` rows: `call`, `run`, `goal_month` for the goal as `scope`, `user_month` as `user`. With no `goal_month` row the scope is still set (`goal:<id>`, `u64::MAX`) | pod 6: always pass a goal scope |
| Unverified prices | With a scope always set, a catalog model with `price_verified = false` pauses the run at `waiting_budget` (`unverified_price`). Slice 1 uses verified Anthropic prices; a per-model verified override is an open item | pod 5 rule, kept |
| Events | `EventBuffer` (an `EventSink`) collects a step's events; `PgCheckpoints` writes them with the next checkpoint. A lost or failed step drops its buffer. `run_events.kind` is the body `type`, `payload` the whole event. Deltas reach Realtime per step, not per token | `emit` is sync; step mode must make events durable before the request returns |
| Vault | `AROH_MASTER_KEY` (base64, 32 bytes). Per user a random 32-byte data key, sealed with AES-256-GCM under the master key with associated data `aroh/dek/v1/<user_id>`, stored in `user_keys`. Each credential sealed under the user's data key with associated data `aroh/key/v1/<user_id>/<setting_id>`, random 96-bit nonce, `key_version = 1`. `load` returns `Secret`; key bytes are zeroized on drop | envelope encryption (Q3); associated data binds a ciphertext to its user and setting, so a copied row does not open |
| Key API | `PUT /v1/keys/{setting_id}` `{vendor, key}`; `GET /v1/keys` lists `{setting_id, vendor, last4, updated_at}`; `DELETE /v1/keys/{setting_id}`. `last4` is stored at set time, so listing never decrypts. `vendor` must be a catalog vendor or a search adapter (`brave`). A key is 8 to 512 chars, no whitespace | brief; the app holds no key (Q3) |
| Model resolution | Selector `vendor/model`: the user's setting for that vendor (default first). Alias `default`: the setting with `is_default`, else the only one. Other alias: the setting whose `label` equals it, else the default. Then `Catalog::connect_with` with the vault key and the model transport, wrapped in `UsageModel`; price from `Catalog::price`. A setting with `base_url` works only when `AROH_ALLOW_CUSTOM_BASE_URL=1` (self-hosted engine) | role aliases (pod 5 open item); a hosted engine must not call user-chosen hosts with a user key |
| Transports | Production: `ReqwestTransport` for providers, `GuardedHttp` (public addresses only) for `web_fetch` and search. Tests inject Troupekit fake transports for both | pod 5: hosts hand tools a `GuardedHttp` |
| Search | The user's enabled `search_settings` by `priority`: `brave` with its vault key, `searxng` with its `base_url`. With none, the server's `AROH_SEARCH_TOML` (keys from env). With neither, `web_search` returns an error output the model can read | Q14 |
| Personas | Built-in Aroh personas in `crates/aroh-server/personas/`. At run start every agent node's persona is resolved goal folder, then user library (`personas` rows), then Aroh built-ins, then `troupekit_agent::builtin()`, and the TOML texts are stored in `runs.personas`; the run's `PersonaLoader` reads only that snapshot | Q9 lookup order; pod 6 open item: a persona edit never reaches a running run |
| Workflows | `crates/aroh-server/workflows/new-goal.toml`: intake, question cards, research with web search, plan (evals), approval, change request and revise loop, `write_plan`. `replan.toml`: adaptation analyst, `record_proposal`, `autonomy_gate()` router, approval when asked, `apply_proposal` or `reject_proposal` | brief, Q19, Q36 |
| Plan shape | Items in local time `YYYY-MM-DDTHH:MM:SS` with `minutes` and an optional `rrule`, one `timezone` per plan; trackers with fields. `write_plan` turns rrule items into `action_series` plus materialized occurrences, others into `actions` in UTC | local times make availability checkable; series reuse `recur` |
| Domain evals | `no_overlap(path)`: occurrences of the items at `path` (rrule items expanded 14 days through `aroh_domain::recur::rule_set`) never overlap. `within_availability(path, "HH:MM", "HH:MM")`: every occurrence starts and ends inside the window on its day | Q33; an eval sees only the output, so the window is an argument |
| Autonomy gate | Condition `autonomy_gate()`: `ask` when the goal is `ask`, the proposal is `big` under `balanced`, or a move names a locked action; else `apply` | Q19: locked actions never move without asking |
| Domain writes | `rows::put_row` (stamp an HLC newer than the row's, `validate()`, upsert by `(user_id, id)` through `jsonb_populate_record` over the registry columns; the LWW trigger decides) and `rows::insert_derived` (insert or ignore). `rows::list_rows` reads `to_jsonb(t) - 'server_seq'` filtered by `user_id`. Tools take `user_id` and `goal_id` from the run, never from model output. Ids of written rows derive from the tool call's idempotency key, so a replayed step rewrites the same rows. Server HLC node `00000000000000e5` | `aroh-store` is SQLite only; one generic Postgres writer over the domain registry |
| Before reads | `prepare_reads(user)` inserts `seed::categories` and materializes every live series of the user for 14 days, before any run input or tool reads actions | brief |
| Stamp rule | `stamp_after` moves from `aroh-store` into `aroh_domain::hlc::Hlc::stamp_after`; both stores call it | one copy |
| Auth | Every route needs `Authorization: Bearer <JWT>`. HS256 with `AROH_JWT_SECRET`, and ES256 or RS256 against `AROH_JWKS_URL` (fetched at start). Only algorithms with a configured key are accepted. User routes require `aud = authenticated`, `role = authenticated`, a uuid `sub`. `/v1/tick` and `/v1/hooks/enqueue` require `role = service_role` and refuse user tokens | Q28; pg_cron sends the service-role JWT |
| Errors | `{"error": {"code", "message"}}`; 400 bad input, 401 bad token, 403 wrong role, 404 not found or another user's, 409 run not waiting or moved, 422 setup failure (no key, unknown model) | one shape for pod 10 |
| OpenAPI | utoipa derives on handlers and types; `docs/engine-api.yaml` is the output of `aroh-server openapi`, and a test fails when the committed file differs | Q28 "OpenAPI from Rust types"; the contract cannot drift |
| DB tests | `#[ignore]` integration tests that need `DATABASE_URL`; the root `Dockerfile.test` lists them as ignored and stays green; `tests/run.sh` runs them with `--include-ignored` | pod 10 shares the root test image |

## Engine HTTP API (contract for pod 10)

| Method and path | Body | Answer |
|---|---|---|
| `GET /healthz` | none | 200 `{ok}` (no auth) |
| `GET /v1/keys` | none | 200 `[{setting_id, vendor, last4, updated_at}]` (key status per setting) |
| `PUT /v1/keys/{setting_id}` | `{vendor, key, model?, label?, is_default?}` | 200 `{setting_id, vendor, last4, updated_at}`; creates the provider setting row when the server has none and `model` is given, else 422 |
| `DELETE /v1/keys/{setting_id}` | none | 204, 404 |
| `POST /v1/keys/{setting_id}/test` | none | 200 `{ok, error}` (one 1-token call, charged) |
| `POST /v1/goals` | `{goal_id?, title, context?, deadline?, category_id?, autonomy?, timezone}` | 202 `{goal_id, run_id}`, 422 when no key is usable |
| `POST /v1/goals/{goal_id}/replan` | `{reason, timezone}` | 202 `{goal_id, run_id}`, 404 |
| `POST /v1/logs` | `{text, at?, timezone}` | 202 `{goal_id: null, run_id}` (log-intake run writes `tracker_entries`) |
| `GET /v1/runs?state=&goal_id=` | none | 200 `[{run_id, goal_id, workflow, state, created_at, updated_at}]`, 50 newest |
| `GET /v1/runs/{run_id}?after_seq=N` | none | 200 `{run_id, goal_id, workflow, status, waiting, usage, events}` |
| `POST /v1/runs/{run_id}/answers` | `{node, answer}` | 202 `{run_id}`, 404, 409 (not waiting there, answer does not fit, run moved) |
| `POST /v1/tick` | none | 200 `{processed, lost, errors}` (service role) |
| `POST /v1/hooks/enqueue` | any JSON | 200 `{processed, lost, errors}` (service role) |

Human nodes: new-goal `questions` (answer `[{id, answer}]`, data `intake`), `approve` (bool, data `plan`), `change` (string); replan `approve` (bool, data `proposal_record` = `{proposal_id, summary, size, touches_locked, changes: [{op, action_id, title, before, after}]}`, spans `{start_at, end_at}` in UTC).

`status` is `{"status": "queued" | "checkpointed" | "waiting_human" | "waiting_budget" | "done" | "failed", ...}`; `waiting` lists `{node, ask: "approval" | "question", prompt, data}`; `events` are core `Event`s (`run_id`, `seq`, `at_ms`, `body` tagged by `type`). Live progress also streams from `run_events` through Supabase Realtime.

## Deployment note (Vercel step mode, not done in this pod)

- Build `aroh-server` for `x86_64-unknown-linux-gnu` and deploy it with the Vercel Rust runtime or Container runtime; the handler is `aroh-server serve` behind the platform's request adapter. A request stays under 300 s; `TickOptions` budgets 250 s.
- Env: `DATABASE_URL` (Supabase pooler, service role), `AROH_MASTER_KEY`, `AROH_JWT_SECRET` and or `AROH_JWKS_URL`, optional `AROH_SEARCH_TOML`. Never `AROH_ALLOW_CUSTOM_BASE_URL` on a hosted engine.
- Supabase: `pg_cron` every minute calls `pg_net.http_post('<engine>/v1/tick')` with the service-role JWT; a trigger or database webhook on `jobs` insert calls `/v1/hooks/enqueue`.
- Vercel Hobby is non-commercial; open sign-ups need Pro or another host (Q26 risk). Embeddings are not in this slice: when `troupekit-context` with `onnx` is linked, the build image needs `g++` (ONNX Runtime needs `libstdc++`) and Rust 1.88 (fastembed), and cold starts add seconds (pod 7).

## File Structure

| Path | Responsibility |
|---|---|
| `Cargo.toml` (root) | workspace deps for the server |
| `crates/aroh-domain/src/hlc.rs` | `Hlc::stamp_after` |
| `crates/aroh-store/src/repo.rs` | calls `Hlc::stamp_after` |
| `supabase/migrations/20261122000000_server_engine.sql` | additive server schema |
| `crates/aroh-server/Cargo.toml` | crate manifest |
| `.../src/lib.rs` | modules and re-exports |
| `.../src/main.rs` | `serve`, `worker`, `openapi` |
| `.../src/config.rs` | `Config::from_env` |
| `.../src/error.rs` | `ServerError`, API error body |
| `.../src/vault.rs` | `MasterKey`, seal and open, `Vault` (set, list, delete, load) |
| `.../src/auth.rs` | `Verifier`, `UserId` and `Service` extractors |
| `.../src/rows.rs` | `ServerClock`, `put_row`, `insert_derived`, `list_rows`, `prepare_reads` |
| `.../src/checkpoints.rs` | `PgCheckpoints`, `EventBuffer` |
| `.../src/queue.rs` | `PgQueue` |
| `.../src/ledger.rs` | `PgLedger`, `UsageModel` |
| `.../src/evals.rs` | `no_overlap`, `within_availability`, `registry()` |
| `.../src/tools.rs` | `write_plan`, `record_proposal`, `apply_proposal`, `reject_proposal`, web tools |
| `.../src/io.rs` | `RunIo` (`AgentIo` and `GraphIo`), model resolution, `autonomy_gate` |
| `.../src/workflows.rs` | built-in workflows and personas, persona snapshot |
| `.../src/engine.rs` | `Engine`: start runs, answer, get run, tick, work |
| `.../src/api.rs` | axum routes, `ApiDoc` |
| `.../personas/*.toml`, `.../workflows/*.toml` | Aroh personas and workflows |
| `.../tests/common/mod.rs` | pool, users, tokens, fake transports |
| `.../tests/*.rs` | vault, stores, api and end-to-end tests |
| `.../tests/run.sh`, `.../tests/compose.yaml`, `.../Dockerfile.test` | Kiri harness |
| `docs/engine-api.yaml` | generated OpenAPI document |

---

### Task 1: Migration, stamp rule and test harness

**Files:** Create `supabase/migrations/20261122000000_server_engine.sql`, `crates/aroh-server/{Cargo.toml,src/lib.rs,Dockerfile.test,tests/run.sh,tests/compose.yaml,tests/common/mod.rs}`; modify root `Cargo.toml`, `crates/aroh-domain/src/hlc.rs`, `crates/aroh-store/src/repo.rs`.

- [x] **Step 1: Failing test:** `hlc::tests::stamp_after_beats_previous` (a stamp from a clock that ran ahead and a non-stamp both give a strictly greater stamp); `tests/schema.rs::server_columns_exist` (ignored, DB: the new columns and `user_keys` exist, `user_keys` has RLS on).
- [x] **Step 2:** Run. Expected: compile failure, then the DB test fails on the missing columns.
- [x] **Step 3 to 4:** write the migration and `stamp_after`; `supabase/tests/run.sh` still prints `rls: ok`.
- [x] **Step 5:** `git commit -m "feat(aroh-server): crate, server schema migration and kiri harness"`

### Task 2: Key vault

**Files:** Create `src/vault.rs`, `tests/vault.rs`.

**Interfaces:** `MasterKey::from_base64(&str) -> Result<MasterKey, ServerError>`; `Vault::new(pool, master)`; `set(user, setting, vendor, Secret) -> KeyInfo`, `list(user) -> Vec<KeyInfo>`, `delete(user, setting) -> bool`, `load(user, setting) -> Option<Secret>`.

- [x] **Step 1: Failing tests:** unit `seal_open_round_trip`, `wrong_associated_data_fails`, `tampered_ciphertext_fails`, `debug_never_shows_key_bytes`; DB `round_trip_per_user`, `other_user_cannot_open` (A's row copied under B's user and setting fails to open), `tampered_row_fails`, `list_shows_vendor_and_last4_only`, `ciphertext_does_not_contain_key`.
- [x] **Step 2 to 4:** run red, implement, green.
- [x] **Step 5:** `git commit -m "feat(aroh-server): envelope-encrypted provider key vault"`

### Task 3: Postgres checkpoints, events, queue and ledger

**Files:** Create `src/rows.rs` (run row helpers), `src/checkpoints.rs`, `src/queue.rs`, `src/ledger.rs`, `tests/stores.rs`.

- [x] **Step 1: Failing tests (DB):** checkpoints `save_requires_next_step`, `latest_returns_highest`, `save_writes_events_and_drops_stale_tail`, `status_mirrors_into_runs`; queue `one_job_per_run`, `claims_due_in_order_with_leases`, `expired_lease_is_reclaimed`, `stale_token_changes_nothing`, `enqueue_while_leased_applies_on_complete`, `skip_locked_claims_are_disjoint`; ledger `period_total_is_calendar_month`, `keys_scope_by_goal_and_user`, `usage_model_writes_one_row_per_call`; runner `two_runners_never_advance_a_run_twice`, `stale_runner_loses_the_save`, `crash_mid_step_resumes`.
- [x] **Step 2 to 4:** run red, implement, green.
- [x] **Step 5:** `git commit -m "feat(aroh-server): postgres checkpoints, job queue, ledger and event log"`

### Task 4: Domain writes, evals and tools

**Files:** Modify `src/rows.rs`; create `src/evals.rs`, `src/tools.rs`, `tests/rows.rs`.

- [x] **Step 1: Failing tests:** evals unit `no_overlap_*`, `within_availability_*`, `expands_rrule_items`; DB `put_row_validates_and_scopes_user`, `insert_derived_never_overwrites`, `prepare_reads_seeds_and_materializes`, `write_plan_is_idempotent_per_key`, `apply_proposal_skips_locked`.
- [x] **Step 2 to 4:** run red, implement, green.
- [x] **Step 5:** `git commit -m "feat(aroh-server): domain writes, evals and aroh tools"`

### Task 5: Run io, workflows, personas and engine

**Files:** Create `src/io.rs`, `src/workflows.rs`, `src/engine.rs`, `personas/*.toml`, `workflows/*.toml`, `tests/engine.rs`.

- [x] **Step 1: Failing tests:** unit `workflows_parse_and_personas_load`, `autonomy_gate_routes`; DB `alias_resolves_to_user_setting`, `missing_key_fails_setup`, `persona_snapshot_survives_edit`, `replan_auto_applies_small_and_asks_big`.
- [x] **Step 2 to 4:** run red, implement, green.
- [x] **Step 5:** `git commit -m "feat(aroh-server): run io, aroh workflows and personas, engine"`

### Task 6: HTTP API, auth and OpenAPI

**Files:** Create `src/config.rs`, `src/auth.rs`, `src/error.rs`, `src/api.rs`, `src/main.rs`, `docs/engine-api.yaml`, `tests/api.rs`.

- [x] **Step 1: Failing tests:** unit `rejects_bad_tokens` (none, expired, wrong secret, wrong audience, user token on `/tick`), `openapi_matches_committed_file`; DB `users_never_see_each_other` (runs, events, keys, answers, replan); end to end `new_goal_to_written_actions` (keys, goal, tick, question, answer, research with fake search and fetch, plan, approval, written actions and series occurrences, seeded categories, usage rows, contiguous events, the fake transport saw the key in `x-api-key`, the key appears in no response, table row or event).
- [x] **Step 2 to 4:** run red, implement, green.
- [x] **Step 5:** `git commit -m "feat(aroh-server): authenticated engine api with openapi contract"`

### Task 7: Full Kiri run and DONE

- [x] `sh crates/aroh-server/tests/run.sh`: every test passes; root `Dockerfile.test` run passes with the DB tests ignored; `supabase/tests/run.sh` prints `rls: ok`.
- [x] Remove this pod's images, volumes and network.
- [x] DONE line with counts, the API summary, deviations and open items.

## Outcome

Shipped in three commits on `feat/aroh-server` (Tasks 1 to 3, Tasks 4 to 6, follow-ups). The tests named per task above live in the `src` unit tests and in `tests/store.rs`, `tests/tools.rs` and `tests/api.rs`.

Deviations:
- Tasks are grouped into fewer commits, and the tests were written alongside the code, not strictly red first.
- `sqlx-core` and `sqlx-postgres` are used directly. The `sqlx` facade's optional sqlite dependency clashes with rusqlite on `links = "sqlite3"`.
- The pod 8 RLS test expects 26 public tables (adds `user_keys`) and inserts `vendor`, `last4`.
- `Hlc::stamp_after` lives in `aroh-domain`. The Postgres writer lives in `aroh-server`, because `aroh-store` is SQLite only.
- Runner tests on Postgres cover two concurrent runners. The crash and stale-runner cases rely on Troupekit's host tests plus the queue's stale-token test.
- Log-intake runs have no goal, so no goal scope limit. `runs.job_id` is unused (jobs link by `jobs.run_id`).
- Agent shells run `kiri` without the `HOME=` prefix (the sandbox refuses it); HOME is already the owner's.

Open items:
- Troupekit: only Anthropic prices are verified, so OpenAI and Google runs pause with `unverified_price` once a limit is set (and the goal scope is always set); it needs a per-model verified-price override. The other items:
  - `Runner` holds one io, so the engine copies the tick loop per lease;
  - `EvalFn` sees no run state;
  - events are per step, not per token;
  - persona `instructions_file` is not snapshotted;
  - checkpoint retention.
- Pod 10:
  - Supabase sign-in, mapping the local user id to the auth uid, and CSP `connect-src` for the engine URL.
  - `saveKey(providerId)` must send the provider_settings row id plus `model`, not the vendor string.
  - `sendChat` goes to the goal's `waiting_human` run (`GET /v1/runs?goal_id=&state=waiting_human`), else to a replan with `reason`.
  - Drain `queue_log` texts through `POST /v1/logs`.
  - Insights are not built.
- Pod 11: an end-to-end run on the real deploy, `pg_cron` calling `/v1/tick`, a webhook on `jobs`, and per-user rate limits.
