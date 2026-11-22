# Pod 11: App to Engine, End to End

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The app talks to the server engine and syncs with Supabase, and the whole slice 1 loop runs on a real local stack in the iPhone 15 Pro simulator: sign in, save a key, new goal chat with question cards, plan proposal, approve, actions on Today and Calendar, check-in, replan proposal, apply, and a reminder that fires.

**Architecture:** All network work of the app runs in Rust, never in the webview. `crates/aroh-sync` is rebuilt as the app's network crate: GoTrue sign-in with the session in the OS keychain (salvaged `auth.rs` and `config.rs`), a PostgREST `Remote` for `aroh_store::sync` authed with the user JWT, and one engine request helper that only reaches `/v1/...` on the configured engine URL with the bearer token attached. The Tauri app exposes thin commands over it (`commands/engine.rs`). `apps/aroh/src/lib/app/engine.ts` implements the `Engine` interface against `docs/engine-api.yaml` on top of one `app_engine` command; it maps run views to chat messages and routes chat text. At sign-in the device adopts the account id: local rows are re-owned, seeded categories re-derived, then sync pushes them. The engine writes plans, trackers and proposals only to Postgres, so the app sees them through sync, which runs after sign-in, after every run that ends, on start and resume. The e2e stack is a Kiri compose project: pgvector Postgres with Supabase roles, GoTrue, PostgREST, an nginx gateway giving them one Supabase-shaped origin, and `aroh-server worker` built with the `fake-transports` cargo feature, whose model replies come from a script file the driver writes with times relative to now.

**Tech Stack:** Rust 2021, reqwest 0.12 (rustls), keyring 3, rusqlite, tokio, axum 0.8; Svelte 5, TypeScript, vitest; Kiri compose with `pgvector/pgvector:pg17`, `supabase/gotrue`, `postgrest/postgrest`, `nginx:alpine`; Xcode simulator tools (`xcrun simctl`).

## Global Constraints

- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. TOML, SQL, YAML, shell carry no comments. No em dashes.
- One copy of every helper. Approved visual rule (Q41) and approved component markup unchanged; new surfaces use approved components only and are added to `/mockups`.
- No provider key in the app, SQLite, logs, sync payloads or the UI: the key goes from the Settings input through one command straight to `PUT /v1/keys/{setting_id}` and is dropped. The UI shows vendor and last 4 only.
- The session (access and refresh token) lives only in the keychain (service `aroh`, account `supabase-session`) and in Rust memory during a call. The webview never sees a token.
- `troupekit/` is touched only for catalog data files.
- `assert!` for internal invariants; input validation returns errors.
- Tests in Kiri. Workspace: `kiri build -f Dockerfile.test -t aroh-test-p11 .` then `kiri run --rm aroh-test-p11`. Server: `sh crates/aroh-server/tests/run.sh`. Frontend: `pnpm check` and `pnpm vitest run` in `node:24-slim` with the app folder mounted and a throwaway `node_modules` volume. Agent shells already have `HOME=/Users/chaoticqubit` and refuse a `HOME=` prefix, so commands call plain `kiri`. Remove the pod's images, volumes and networks after each run; check `df -h /` before big builds.
- Branch `feat/app-engine-e2e`, conventional commits, one commit per task, commit and never push, no `Co-Authored-By` trailer.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Where HTTP runs | Rust (`aroh-sync`), through Tauri commands. The webview makes no network calls, so CSP `connect-src` stays `ipc: http://ipc.localhost` and the engine needs no CORS. | Security first: tokens never reach JavaScript. The engine URL is a runtime setting, which a static CSP cannot name exactly, and `fetch` with a bearer header would need CORS on the engine. Deviation from the brief's CSP item, recorded. |
| Engine URL | Existing device setting `server_url`, validated by `aroh_domain::rows::check_url` (https, or http on `localhost` and `127.0.0.1`). | Pod 10 already stores it; one URL rule. |
| Supabase project | New device settings `supabase_url` (same URL rule) and `supabase_anon_key` (public key, 20 to 2048 chars). Settings gets an Account card: project URL, anon key, email, password, Sign in, Sign out, status line. Approved components only, added to `/mockups`. | No approved mockup exists for sign-in; pod 10 precedent. Needs owner review (deviation). |
| Sign-in | Email and password through GoTrue `POST /auth/v1/token?grant_type=password`; sign-up through `POST /auth/v1/signup` when the owner taps Create account. Refresh 60 s before expiry. Salvaged from MVP `aroh-sync` `auth.rs`. | MVP code with the keychain store already tested on device. |
| Account adoption | `aroh_app::local::adopt(conn, account_id)`: when the device was never adopted, re-own every synced row from the local id to the account id, repoint `goals.category_id` from each old seeded category id to `derived_id(account, slug, 0)`, delete the old seeded rows, store `local:user_id = account`, `local:account = account`, then re-seed. Signing in as another account on an adopted device is refused. `Core.user_id` is updated in memory. | Seeded ids derive from the user id; occurrence ids derive from the series id and stay. Re-owning another account's data would leak it. |
| Sync remote | `aroh_sync::rest::PgRest` implements `aroh_store::sync::Remote`: push `POST {url}/rest/v1/{table}?on_conflict=user_id,id` with `Prefer: resolution=merge-duplicates,return=minimal`; pull `GET {url}/rest/v1/{table}?select=*&server_seq=gt.{after}&order=server_seq.asc&limit={n}`. Headers `apikey` and `Authorization: Bearer <user JWT>`. RLS isolates users; the Postgres `aroh_lww` trigger keeps the newer stamp. | Brief; same wire as MVP, now over the rebuild registry. |
| Sync runs | Command `app_sync` runs `sync_all` on a blocking thread holding the core lock, then the start-up tick, then emits `aroh://changed`. Called after sign-in, after a run ends, on start and resume when signed in. | `rusqlite::Connection` cannot cross an await under a std mutex; a sync takes under a second at slice-1 sizes. |
| Engine calls | Command `app_engine(method, path, body)`: path must start with `/v1/`, contain no `..`, `//`, `?` except one query of `[A-Za-z0-9_=&-]`, method one of GET, POST, PUT, DELETE. Returns `{status, body}`. Not signed in or no engine URL returns `needs_engine`. | One trust boundary; the TS client stays a pure mapping of the API. |
| Key save | `app_save_key(provider_id, key)` reads the default `provider_settings` row for that vendor and sends `PUT /v1/keys/{row id}` with `{vendor, key, model, label, is_default}`. The key is a `String` dropped after the call and never logged; errors never echo it. | Brief: row id plus model; key over TLS to the engine only. |
| Provider rows | `save_provider` keeps one row per vendor and moves `is_default`, instead of rewriting the vendor of one row. | A key belongs to one setting row; per-provider key status needs one row per vendor. |
| Key status | Settings reads `GET /v1/keys`; a provider shows Key saved with `last4` when a key exists for its vendor's setting row. | Brief; vendor and last 4 only. |
| Chat routing | `sendChat(goalId, text)`: `GET /v1/runs?goal_id=&state=waiting_human`; with a run, answer its first waiting node: `questions` collects one answer per message in question order and posts `[{id, answer}]` after the last; `change` posts the text; `approve` treats the text as a change request (posts `false`, then the text to `change`). With no waiting run, `POST /v1/goals/{id}/replan` with the text as `reason`. Pure function `route()` in `engine.ts`, tested. | Brief; `PersonaChat` cards emit nothing, so answers go through the composer and the approved markup stays. |
| Inbox apply | A proposal whose goal has a replan run waiting at `approve` with that `proposal_id` is answered through `POST /v1/runs/{id}/answers`, then synced. Only a proposal with no waiting run is applied locally. | Applying locally too would leave the run waiting and apply twice. |
| Inbox runs | Runs in progress group reads `GET /v1/runs` (not `done`), one row per run: workflow as title, state as step. | Brief. |
| Pauses | `waiting_budget` with `unverified_price`: "Paused: this model's price is not verified, so spend cannot be checked. Pick an Anthropic model in Settings." Other budget limits: "Paused: the {limit} budget is reached. Raise it in Settings to continue." Shown as a persona message in the chat and as the run's step in Inbox. | Brief. Status shape is `{"status": "waiting_budget", "reason": {"kind": "budget", "limit": ...}}` (serde tags of `GraphStatus` and `PauseReason`). |
| Polling | While a run is queued, checkpointed or running, `GET /v1/runs/{id}?after_seq=N` every 1.5 s; new events become progress lines; on `waiting_human`, `waiting_budget`, `done` or `failed` polling stops; on `done` the app syncs. | Brief; Realtime is slice 2. |
| Log flush | When signed in, the app posts each `pending_logs` entry oldest first to `POST /v1/logs` `{text, at, timezone}`, stops at the first failure, then `app_logs_sent(sent)` removes exactly the sent entries (`aroh_app::local::remove_sent`). | Brief; a log queued during the flush is kept. |
| Fake transports | `aroh-server` cargo feature `fake-transports` (never default, never in release images): `src/fake.rs` holds `FnTransport`, the SSE reply helpers and `ScriptTransport`, which reads `AROH_FAKE_SCRIPT` (JSON rules: all `when` substrings present in the request body, first match wins, reply `text`, `tool` or `json`). The web transport answers every request 404. `tests/common` imports the helpers from `aroh_server::fake`. | Brief: test-only switch behind a feature; one copy of the helpers. |
| Worker mode | `aroh-server worker` serves the API and runs the tick loop in one process, as the pod 9 decision says. | `main.rs` ran only the loop, so a worker had no API (pod 9 finding). |
| Plan cadence | `PlanTracker` gets an optional `cadence {freq, weekdays, at}`; `write_plan` stores it; the planner persona's output type allows it. | A tracker without cadence never shows a check-in card. |
| OpenAPI params | `RunsQuery` and `EventsQuery` declare `parameter_in = Query`; `docs/engine-api.yaml` regenerated. | The committed contract listed query parameters as path parameters. |
| Local stack | `e2e/compose.yaml`: `pg` (pgvector pg17, init script creates the Supabase roles, `auth` schema owned by `supabase_auth_admin`, `extensions` schema with `vector`), `auth` (GoTrue, runs its own migrations, autoconfirm on), `migrate` (applies `supabase/migrations` after GoTrue is up), `rest` (PostgREST, schema `public`, anon role `anon`), `gateway` (nginx: `/auth/v1/` to GoTrue, `/rest/v1/` to PostgREST, port 54321), `engine` (`aroh-server worker` with `fake-transports`, port 8080). One HS256 secret for GoTrue, PostgREST and `AROH_JWT_SECRET`. `stub_auth.sql` is not used there. | Brief; GoTrue owns `auth.users`, so the stub would collide. |
| Fake script | `e2e/script.sh` writes the script with times relative to now in the device zone: one action 30 min from now (or 06:00 next day after 21:00), a daily 07:00 series, a weight tracker with a daily cadence due now, a check-in cadence two minutes ahead for the reminder; the analyst reply is a `create` change with `size: big` so the Balanced goal asks. | Times must pass `within_availability(06:00, 22:00)`; `create` needs no action id from the request. |
| Simulator | A dedicated device `aroh-p11` of type iPhone 15 Pro (no iPhone 15 Pro device existed). The debug app is built with the frontend embedded, installed with `simctl install`, launched with `SIMCTL_CHILD_AROH_DEV_SCRIPT`; the script drives the UI and the driver takes `simctl io screenshot` per step into `docs/screenshots/pod11/`. Notifications granted with `simctl privacy grant notifications io.aroh.app`, or the reminder proof uses `simctl spawn ... log` when no grant subcommand applies. | No Simulator GUI on this Mac (pod 10 and MVP demo). |
| Prices | Anthropic catalog rows checked against https://platform.claude.com/docs/en/about-claude/pricing (Model pricing table, read 2026-11-21): Opus 5.5 $4 / $20, Sonnet 5.5 $2 / $10, Haiku 5.5 $0.10 / $0.50 up to 100,000 prompt tokens and $0.50 / $2.50 above. The catalog already matches, so no edit. `ModelSpec` has `deny_unknown_fields`, so a `source` field cannot be added; the URL lives here. OpenAI https://developers.openai.com/api/docs/pricing shows `gpt-6.2-sol` Standard short context at $2 / $10, but its long-context tier is not readable from the static page and `gpt-6.1-terra` and `gpt-6-luna` appear only in the Flex table, so no OpenAI row is changed or verified. Other vendors are not checked. | Never invent prices; a row without its long-context tier would under-count. |

## File Structure

| Path | Responsibility |
|---|---|
| `crates/aroh-server/Cargo.toml` | feature `fake-transports` |
| `crates/aroh-server/src/fake.rs` | `FnTransport`, SSE helpers, `ScriptTransport` (feature-gated) |
| `crates/aroh-server/src/main.rs` | fake transports when the feature and `AROH_FAKE_SCRIPT` are set; worker serves the API |
| `crates/aroh-server/src/tools.rs` | `PlanTracker.cadence` |
| `crates/aroh-server/src/api.rs` | query parameter locations |
| `crates/aroh-server/personas/goal-planner.toml` | tracker cadence in the output type |
| `crates/aroh-server/tests/common/mod.rs` | imports helpers from `aroh_server::fake` |
| `docs/engine-api.yaml` | regenerated |
| `crates/aroh-sync/src/{lib,auth,config,rest,engine}.rs` | sign-in, keychain, PostgREST remote, engine request |
| `crates/aroh-sync/tests/live.rs` | two devices converge through the real stack (ignored without `AROH_E2E_SUPABASE_URL`) |
| `crates/aroh-app/src/local.rs` | `adopt`, `remove_sent`, Supabase settings |
| `crates/aroh-app/src/write.rs` | one provider row per vendor |
| `crates/aroh-app/tests/app.rs` | adopt, remove_sent, provider rows |
| `apps/aroh/src-tauri/src/commands/engine.rs` | `app_sign_in`, `app_sign_up`, `app_sign_out`, `app_account`, `app_sync`, `app_engine`, `app_save_key`, `app_logs_sent` |
| `apps/aroh/src-tauri/src/lib.rs` | register commands, sync on start and resume |
| `apps/aroh/src/lib/app/engine.ts` | `Engine` over `app_engine`: runs, chat mapping, `route`, pauses, log flush |
| `apps/aroh/src/lib/app/engine.test.ts` | mapping, routing, flush, pause messages |
| `apps/aroh/src/lib/app/{client,catalog,views,data.svelte}.ts` | new commands, engine catalog ids, key status, runs |
| `apps/aroh/src/lib/ui/screens/SettingsScreen.svelte` | Account card |
| `apps/aroh/src/routes/{goals/new,goals/[id],inbox,settings,log}/+page.svelte` | engine wiring |
| `e2e/compose.yaml`, `e2e/initdb.sql`, `e2e/nginx.conf`, `e2e/up.sh`, `e2e/down.sh`, `e2e/script.sh`, `e2e/smoke.sh`, `e2e/drive.js` | local stack and simulator driver |
| `docs/screenshots/pod11/*.png` | one screenshot per loop step |

---

### Task 1: Server: fake transports, worker API, cadence, query params

**Files:** `crates/aroh-server/{Cargo.toml,src/fake.rs,src/lib.rs,src/main.rs,src/tools.rs,src/api.rs,personas/goal-planner.toml,tests/common/mod.rs,tests/tools.rs}`, `docs/engine-api.yaml`.

- [ ] **Step 1: Failing tests:** `fake::tests::script_routes_by_substrings` (first rule whose `when` strings are all in the body answers; no match answers 400), `tests/tools.rs` `write_plan_stores_tracker_cadence`, `openapi_document_is_committed` (fails until the yaml is regenerated with query parameters).
- [ ] **Step 2:** Move `FnTransport`, `Reply`, `sse_text`, `sse_tool`, `json_reply`, `body_text` into `src/fake.rs` behind `fake-transports`; tests enable the feature through a self dev-dependency and import them. Add `ScriptTransport::from_json`.
- [ ] **Step 3:** `main.rs`: with the feature and `AROH_FAKE_SCRIPT` set, models use `ScriptTransport` and web a 404 transport; print one line saying fake transports are on. `worker` serves the API and ticks.
- [ ] **Step 4:** `PlanTracker.cadence`, `write_plan` stores it, planner output type; `parameter_in = Query`; regenerate the yaml.
- [ ] **Step 5:** `sh crates/aroh-server/tests/run.sh` passes. Commit `feat(aroh-server): fake transports feature, worker api, tracker cadence`.

### Task 2: Local Supabase stack and engine under Kiri

**Files:** `e2e/*`.

- [ ] **Step 1:** `e2e/smoke.sh` (the failing check): sign up two users through the gateway, insert a goal as A through PostgREST, A reads 1 row, B reads 0, `GET /healthz` on the engine, `GET /v1/keys` with A's token answers 200.
- [ ] **Step 2:** compose file, init SQL, nginx config, `up.sh` (writes the fake script, builds, starts, waits for health), `down.sh` (removes containers, images, volumes, network).
- [ ] **Step 3:** `sh e2e/up.sh && sh e2e/smoke.sh` passes. Commit `test(e2e): local supabase stack with the engine in worker mode`.

### Task 3: aroh-sync on the rebuild store

**Files:** `crates/aroh-sync/*`, root `Cargo.toml`.

- [ ] **Step 1: Failing tests:** unit `pull_url_and_push_url` (table, cursor, limit, on_conflict), `engine_path_rules` (accepts `/v1/runs?goal_id=x&state=waiting_human`, refuses `/healthz`, `/v1/../x`, `//evil`, `http://x`, bad method), `session_debug_hides_tokens`; live `two_devices_converge_through_postgrest` (two SQLite stores of one user sync through the stack: edits on both converge by LWW, a tombstone wins, a second user reads none of them).
- [ ] **Step 2:** Drop `store.rs`, `merge.rs`, old `remote.rs` and the `aroh-core` dependency; keep `auth.rs` and `config.rs` (config now passed in, not loaded from env or keychain; session in keychain); add `sign_up`; add `rest.rs` and `engine.rs`.
- [ ] **Step 3:** Workspace tests pass in Kiri; the live test passes against the stack. Commit `feat(aroh-sync): postgrest remote for aroh-store, sign-in and engine requests`.

### Task 4: aroh-app adoption, log removal, provider rows

**Files:** `crates/aroh-app/src/{local.rs,write.rs}`, `crates/aroh-app/tests/app.rs`.

- [ ] **Step 1: Failing tests:** `adopt_reowns_rows_and_reseeds` (sample store: every row moves to the account, the goal's category points at the account's seeded id, the old seeded rows are gone, a second adopt is a no-op, another account is refused, rows push under the account), `remove_sent_keeps_new_logs`, `one_provider_row_per_vendor` (switching vendor keeps both rows, one default), `supabase_settings_validate`.
- [ ] **Step 2:** Implement. Kiri passes. Commit `feat(aroh-app): adopt the account id, remove sent logs, one provider row per vendor`.

### Task 5: Tauri commands

**Files:** `apps/aroh/src-tauri/{Cargo.toml,src/lib.rs,src/commands/mod.rs,src/commands/engine.rs}`.

- [ ] Commands as listed in File Structure; `app_engine` and `app_save_key` never log bodies; sync on start and resume when signed in.
- [ ] `cargo check -p aroh` on the host passes. Commit `feat(app): engine, account and sync commands`.

### Task 6: Engine client and screens

**Files:** `apps/aroh/src/lib/app/*`, `apps/aroh/src/lib/ui/screens/SettingsScreen.svelte`, `apps/aroh/src/lib/ui/mockdata/sixpack.ts`, routes.

- [ ] **Step 1: Failing tests (`engine.test.ts`):** run view to chat messages (questions to question cards, plan to a plan message, events to progress lines); `route` (questions collected in order, change text, approve then change, replan without a waiting run); pause messages for `unverified_price` and `user`; log flush stops at the first failure and reports only sent entries; `saveKey` sends the row id and model, never stores the key; key status per provider from `GET /v1/keys`.
- [ ] **Step 2:** Implement `engine.ts`, catalog ids aligned with the engine catalog, views, Account card, routes (New Goal, Goal persona chat, Inbox runs and apply routing, Settings, Log flush).
- [ ] **Step 3:** `pnpm check` 0 errors, `pnpm vitest run` passes, `AROH_MOCKUP=1 pnpm build` passes. Commit `feat(app): engine client, sign-in, sync and run progress`.

### Task 7: Simulator end to end and reminders

**Files:** `e2e/drive.js`, `e2e/sim.sh`, `docs/screenshots/pod11/`.

- [ ] Stack up, simulator `aroh-p11` booted, debug app installed, notifications granted.
- [ ] Drive: Settings sign up and sign in, engine URL, save key (key status shows `...last4`), New Goal "six pack by Feb 1", question cards answered, plan proposal, Approve, sync, Today and Calendar show the actions, Log check-in, chat "I missed today's workout" starts a replan, Inbox shows the proposal, Apply, Calendar shows the created action, a check-in reminder banner. One screenshot per step.
- [ ] Commit `test(e2e): simulator loop screenshots`.

### Task 8: Report

- [ ] Fill Outcome below, `graphify update .`, `docs/architecture.html` is the leader's.
- [ ] DONE line `DONE feat/app-engine-e2e <base>..<tip> VERIFIED|NOT VERIFIED`.

## Outcome

Branch `feat/app-engine-e2e`, base `d8a0e6a`. Tests: aroh-server 38 (Postgres sidecar in Kiri), workspace without the Tauri crates all green in Kiri (aroh-app 15, aroh-store, aroh-domain, aroh-sync 4 unit), aroh-sync live test 1 against the local stack, frontend 23 vitest and `svelte-check` 0 errors, `AROH_MOCKUP=1 pnpm build` passes, `cargo check -p aroh` and an iOS simulator build pass.

Simulator loop: VERIFIED at the tip on 2026-11-21 21:30 to 21:42, iPhone 15 Pro (`aroh-p11`, iOS 26.1) against the Kiri stack: sign-up and sign-in, key hand-off, new-goal question cards, plan, approve, actions synced (shown on tomorrow's Calendar because the run started after the 06:00 to 22:00 window allowed a slot today; the driver now falls back to tomorrow when Today is empty), check-in, chat-started replan, proposal in Inbox, apply, make-up action on the Calendar, and the background check-in banner "Check-in: Meals" at 21:42. Screenshots 01 to 14 in `docs/screenshots/pod11/`.

Deviations:

- All app HTTP runs in Rust (`aroh-sync`), so the CSP is unchanged instead of listing the engine URL in `connect-src`; tokens never reach the webview.
- The Account card in Settings has no approved mockup; it uses approved components only and needs owner review.
- Kiri's buildkit builder was broken, so no image builds: tests and the engine run in `rust:1-slim` with the source mounted and named volumes.
- `tauri ios build` refuses this Mac (it looks for an iOS 27.2 simulator runtime); the app was built with `xcodebuild` while `tauri ios dev aroh-p11` served the CLI options and the dev frontend.
- `simctl privacy` cannot grant notifications and this Mac has no Simulator app to tap Allow, so `e2e/grant.py` writes the grant into the shut-down simulator's BulletinBoard store.
- `simctl launch --console` captured no app output, so the driver's step marks go to `dev-marks.log` in the app data folder through the debug-only `app_dev_mark` command.
- The host was overloaded by other builds during the run (GoTrue password hashing took 10 to 40 s), so the driver retries account creation and GoTrue's request limit is 60 s in the e2e stack.
- `PersonaChat` now sends a question card's answer through the chat and scrolls to the newest message; the markup is unchanged, the behaviour is new and needs owner review.
- The check-in reminder in the Task 7 checklist was replaced by the action reminder as proof that a reminder fires.
- `saveKey` has no TypeScript test: the request body is built in Rust (`app_save_key`) from the vendor's provider row.

Open items for slice 2:

- Realtime instead of 1.5 s polling; sync after engine writes is pull-on-demand.
- Question cards answer in question order; an answer sent for a later card first is matched to the earlier question.
- Only Anthropic prices are verified; other vendors pause at `unverified_price`.
- `tauri ios build` on hosts without a matching simulator runtime; a release iOS build path.
- The goal page's chat opens `/goals/new?goal=<id>`; a goal-scoped chat screen needs a mockup.
