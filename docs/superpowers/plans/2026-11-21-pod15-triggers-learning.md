# Pod 15: Adaptation Triggers, Scheduled Reviews, Learning From Edits, Inbox Insights

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The server engine adapts plans without the owner asking (Q20). Three triggers each start a replan run that respects the goal's autonomy (Q19): action events (missed, skipped, done late), tracker rules ("weight flat for 14 days", "3 missed check-ins") and weekly scheduled reviews. Owner messages already start replans. Each trigger is deduped per goal and window. Reviews run on the server tick with the app closed: pg_cron and pg_net call `/v1/tick` in step mode, and the worker loop does the same in worker mode. Owner edits to actions (move, resize, delete) are recorded. After 3 or more similar edits, an analysis persona proposes a preference (Q37). Confirmed preferences become user profile memory and feed planning. The Inbox shows insights and preference proposals.

**Architecture:**
- **Edit capture.** A Postgres trigger on `actions` writes every owner change (any writer whose HLC node is not the server's) into `action_events`, with the row before and after: a move, resize, delete, or a status change to missed, skipped or done. Owner edits made offline reach Postgres through sync, so the capture happens when sync pushes the row. No new synced table is needed.
- **Missed actions.** The app marks an action missed only while it is open. So the scan also derives missed events from planned actions that ended more than `MISSED_GRACE_MIN` ago. It never rewrites the owner's row.
- **The scan.** `Engine::tick` runs `triggers::scan` before it claims jobs, at most once every 30 s per process. One pass:
  1. inserts the derived missed events;
  2. starts one replan per goal that has pending status events, a breached tracker rule or a due review;
  3. starts one learning run per group of 3 or more similar pending edits.
- **Dedupe and insights.** Every start inserts an `adaptations` row first, unique on `(user_id, goal_id, window_key)`, so a concurrent cron tick and enqueue hook start one run. The same rows back `GET /v1/insights`.
- **Shared start path.** Every replan, from the owner or a trigger, goes through one `Engine::replan` path. That path:
  - claims the goal's pending status events;
  - passes confirmed preferences as `profile`;
  - stores the timezone on the goal's review schedule, so server-started runs know the owner's zone.
- **Learning.** A learning run (`learn-preference` workflow, `preference-analyst` persona) writes one unconfirmed `preferences` row with an id derived from the edit pattern. The app already shows unconfirmed preferences as Inbox suggestions and confirms or rejects them through sync.

**Tech Stack:** Rust 2021, sqlx 0.8 (runtime queries), chrono, chrono-tz, axum 0.8, utoipa 5; Postgres 17 with pgvector for tests; pg_cron and pg_net on Supabase; Svelte 5, TypeScript, vitest; Kiri; Xcode simulator tools.

## Global Constraints

- No code comments. Docstrings only, directly above a declaration, 3 to 5 lines. TOML, SQL, YAML and shell files carry no comments. No em dashes.
- One copy of every helper. Compose only from approved components (`InsightCard`, `ProposalCard`, Inbox groups); no new visual pattern, so no mockup.
- `assert!` for internal invariants; input validation returns `Err`.
- Pods 12 to 14 run in parallel, so edits to shared files are additive only:
  - root `Cargo.toml`, the `lib.rs` handler list, `commands/mod.rs`;
  - `docs/engine-api.yaml`, the server router;
  - the `WORKFLOWS` and `PERSONAS` arrays;
  - migrations: new file `supabase/migrations/20261123150000_triggers_learning.sql`, later than pod 13's `20261123130000`.
- No provider key in the app, SQLite, logs, sync payloads, insights or the UI.
- Tests:
  - Kiri workspace: `kiri build -f Dockerfile.test -t aroh-test-p15 .`, then `kiri run --rm aroh-test-p15`; when buildkit is broken, `rust:1-slim` with the source mounted.
  - Server: `sh crates/aroh-server/tests/run.sh`, or a `pgvector/pgvector:pg17` sidecar with `rust:1-slim`.
  - Frontend: `node:24-slim`.
  - Remove own images and volumes after each run; check `df -h /` first.
- Branch `feat/triggers-learning`. Conventional commits, one per task, no co-author trailer. Commit, never push.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Edit capture | `aroh_action_events()` runs after an update on `actions` and is skipped when `right(new.updated_at, 16)` is the server node `00000000000000e5`. It inserts `kind` = `delete` (deleted became true), `move` (start moved, same length), `resize` (length changed), or the new status (`missed`, `skipped`, `done`), with `before` and `after` `{start_at, end_at, status, done_at, title}`. It is `security definer` with an empty `search_path`, and its execute grant is revoked from clients. | The trigger sees OLD and NEW of every synced push, so offline edits are captured at sync. No new synced table, no registry or parity change. Server writes (apply, materialize) are not owner edits. |
| Missed with the app closed | The scan inserts `missed` events for live planned actions of active goals with `end_at` in `[now - 48 h, now - MISSED_GRACE_MIN]`. A partial unique index on `(user_id, action_id, kind)` for `missed` and `skipped` makes the derived event and the app's later synced status one event. `MISSED_GRACE_MIN` moves to `aroh_domain::time`, and `aroh-app` re-exports it. | The server must not overwrite the owner's row: a done marked offline could lose to a newer server stamp. The 48 h bound keeps old history from firing on first deploy. One copy of the grace. |
| Done late | A `done` event counts as late only when `done_at > end_at + MISSED_GRACE_MIN`. Rust decides, and other done events are marked handled. | The grace lives in Rust only. |
| Event claim | `Engine::replan` claims the goal's pending status events by setting `handled_by = run`. First it inserts the derived missed events of that goal, so every action past due when the run starts counts as seen. | An owner replan already covers what is past due; without this, a later scan replans the same missed action again. |
| Tracker rules | `RuleOp` gains `Flat` and `Missed`. `Flat`: at least 2 entries of `field` in the last `window_days`, spanning at least `window_days - 1` days, with `max - min <= value`. `Missed`: in the last `window_days`, the count of local cadence days (ended before `now - grace`) without an entry of the tracker is at least `value`; it needs a cadence, a whole `value >= 1`, and a `field` of any kind. `Above` and `Below` stay. Rules are evaluated in Rust (`triggers::breaches`). | The existing average rule cannot say "flat" or "missed check-ins". The enum change is additive, so rows already stored still parse. |
| Rules from the planner | `PlanTracker.rules` is optional: `list<{field, op, value: number, window_days: int}>` in the planner `[output]` and the `new-goal` `plan` type, stored by `write_plan`. | `write_plan` always wrote `rules: []`. |
| Review schedule | Server-only `review_schedules(user_id, goal_id, weekday 0-6, at 'HH:MM', timezone, enabled, next_at)`. `start_goal` creates Sunday 18:00 in the request zone. Every replan request updates `timezone`. `GET` and `PUT /v1/goals/{goal_id}/review` read and set `{weekday, at, enabled}`. `triggers::next_review` computes the next local slot after `now`; a nonexistent local time moves to the next valid instant. | Weekly per goal, configurable (Q20). The zone must be known with the app closed. |
| Windows | `adaptations` rows are unique on `(user_id, goal_id, window_key)`, with nulls not distinct. Keys:<br>• `actions:<UTC date>`: one event replan per goal per day; unclaimed events wait for the next day.<br>• `rule:<tracker>:<index>:<now_days / window_days>`: a rule that stays true fires once per window.<br>• `review:<local slot date>`<br>• `edits:<pattern>:<UTC date>` | One run per goal and window, enforced by Postgres, so concurrent ticks start one. |
| Busy goals | A goal with a replan run in `queued`, `running`, `checkpointed`, `waiting_human` or `waiting_budget` is skipped; its triggers stay pending. Only `active` goals trigger. | No pile of proposals; draft and paused goals never adapt. |
| Start failures | A start that fails (no usable key: `Setup`) keeps its `adaptations` row with `error` set and no run, and is never retried in that window. It is shown as an insight. It never goes to `TickSummary.errors`. | A user without a key must not fail every tick. |
| Scan throttle | `Engine` keeps the last scan time in memory (`Mutex<Option<i64>>`, injected clock) and scans when 30 s have passed. | Worker mode ticks every second, while cron ticks once a minute. Dedupe keeps several processes correct, so no table is needed. |
| Edit similarity | `learning::edit_key(kind, before, after, tz)`:<br>• move: `move:<part>-><part>`, where the parts are morning 05-12, afternoon 12-17, evening 17-22 and night;<br>• resize: `resize:shorter` or `resize:longer`;<br>• delete: `delete:<weekday>`.<br>Pending edit events of the last 30 days are grouped per user by key, and a group of 3 or more starts a learning run that claims those events. The zone is the goal's review schedule zone, else UTC. | Deterministic, testable similarity. A preference is about the owner, so edits group across goals. |
| Single edit | `aroh_app::write::edit_action` (move, resize, delete) sets `locked = true` and the `app_edit_action` command exposes it. The replan gate already asks before touching a locked action. | Q37: one edit only locks that action; Q19: hand-edited actions never move without asking. |
| Profile memory | Profile memory is the synced `preferences` table. The learning run writes it with `insert_derived` and the id `derived_id(user, "preference:<pattern>", now)`. A confirmed preference is profile memory. Up to 20 confirmed facts go into `new-goal` and `replan` as state `profile` (`list<string>`), read by the planner and the analyst. | The app already lists unconfirmed preferences in the Inbox and confirms them through sync. Troupekit `FactBook` would need a Postgres `MemoryLog`, a second store and a server call to confirm. A rejected preference is a tombstone, so the same pattern is never proposed again. |
| No-change replans | When the analyst proposes no change, `record_proposal` writes no proposal and returns `proposal_id: ""`. `autonomy_gate()` then answers `none`, which routes to `done`. | A review or a done-late event may need no change, which is an insight, not an empty approval. |
| Insights | `GET /v1/insights?goal_id=` returns the 30 newest `adaptations` rows of the user as `{id, goal_id, run_id, source, title, body, state, created_at}`. `body` is the reason plus the run summary. `RunView.summary` is new: the proposal summary and outcome (applied N, waiting for approval, rejected, no change), or the proposed preference. | Q38 Inbox insights. The query parameters fit the `app_engine` path rule. |
| Scheduling | Migration `do` block: when `pg_available_extensions` has `pg_cron` and `pg_net` and a `vault` schema exists, it creates both extensions and schedules `aroh-tick` every minute. The job runs `net.http_post` to `<aroh_engine_url>/v1/tick`, with the service-role JWT `aroh_service_jwt` and the URL read from `vault.decrypted_secrets`, and `timeout_milliseconds` 290000. Errors raise a notice, so plain Postgres skips it. Worker mode: `aroh-server worker` loops `Engine::tick` every second, with the same scan throttle. | Q26 and Q27. No secret in a migration; test and e2e Postgres have no pg_cron. |
| Table count | `supabase/tests/rls_rebuild.sql` expects 31 public tables: pod 13's 28 plus 3 server-only ones (`action_events`, `adaptations`, `review_schedules`). Each has RLS on, no client policy, all rights revoked from `anon` and `authenticated`, and grants only to `service_role`. The app reads them through `/v1/insights`. | Changed from 29 after pod 13 merged first. No client policy because no client reads these tables directly. |

## Engine API additions

| Method and path | Body | Answer |
|---|---|---|
| `GET /v1/insights?goal_id=` | none | 200 `[{id, goal_id, run_id, source, title, body, state, created_at}]`, 30 newest |
| `GET /v1/goals/{goal_id}/review` | none | 200 `{goal_id, weekday, at, timezone, enabled, next_at}`, 404 |
| `PUT /v1/goals/{goal_id}/review` | `{weekday, at, enabled, timezone}` | 200 the schedule, 400, 404 |
| `GET /v1/runs/{run_id}` | none | adds `summary: string?` |

`source` is `actions`, `rule`, `review` or `edits`.

## File Structure

| Path | Responsibility |
|---|---|
| `supabase/migrations/20261123150000_triggers_learning.sql` | tables, capture trigger, guarded cron job |
| `supabase/tests/rls_rebuild.sql` | table count 31 |
| `crates/aroh-domain/src/rows.rs` | `RuleOp::Flat`, `RuleOp::Missed`, rule validation |
| `crates/aroh-domain/src/time.rs` | `MISSED_GRACE_MIN` |
| `crates/aroh-app/src/{tick.rs,write.rs}` | re-export grace, `edit_action` |
| `crates/aroh-server/src/triggers.rs` | rule evaluation, review slots, window keys, reasons, `scan` |
| `crates/aroh-server/src/learning.rs` | `edit_key`, grouping, `propose_preference` toolkit |
| `crates/aroh-server/src/insights.rs` | `Insight`, `run_summary`, list query |
| `crates/aroh-server/src/{engine.rs,api.rs,io.rs,tools.rs,workflows.rs,lib.rs}` | replan path, scan in tick, routes, gate `none`, plan rules, workflow registration |
| `crates/aroh-server/workflows/{learn-preference.toml,replan.toml,new-goal.toml}` | learning workflow, `profile`, no-change edge, plan rules |
| `crates/aroh-server/personas/{preference-analyst.toml,goal-planner.toml,adaptation-analyst.toml}` | analysis persona, profile and rules instructions |
| `crates/aroh-server/tests/triggers.rs` | Postgres integration tests |
| `docs/engine-api.yaml` | regenerated |
| `apps/aroh/src-tauri/src/commands/learning.rs` | `app_edit_action` |
| `apps/aroh/src/lib/app/{engine.ts,views.ts}`, `apps/aroh/src/routes/inbox/+page.svelte` | insights client and Inbox group |
| `e2e/pod15.js`, `docs/screenshots/pod15/` | simulator driver and screenshots |

---

### Task 1: Domain: rule ops, grace, edit action

**Files:** `crates/aroh-domain/src/{rows.rs,time.rs}`, `crates/aroh-app/src/{tick.rs,write.rs}`, `crates/aroh-app/tests/*`.

- [x] **Step 1: Failing tests.**
  - `rows` unit test `flat_and_missed_rules_validate`: a `missed` rule on a text field with a cadence is valid; without a cadence, or with value 2.5, it is invalid; a `flat` rule needs a number field.
  - aroh-app `edit_action_moves_resizes_deletes_and_locks`.
- [x] **Step 2:** Implement; workspace tests pass in Kiri. Commit `feat(domain): flat and missed tracker rules, owner action edits lock`.

### Task 2: Migration and capture trigger

**Files:** migration, `supabase/tests/rls_rebuild.sql`, `crates/aroh-server/tests/triggers.rs`.

- [x] **Step 1: Failing test** `owner_edits_are_captured_and_server_writes_are_not`: an update with a device stamp records a move; a server `put_row` records nothing.
- [x] **Step 2:** Write the migration. Run `sh supabase/tests/run.sh` and the server tests. Commit `feat(supabase): action events, adaptations, review schedules, tick cron`.

### Task 3: Triggers core (pure)

**Files:** `crates/aroh-server/src/triggers.rs`.

- [x] **Step 1: Failing unit tests:**
  - `above_below_flat_missed_rules_evaluate`
  - `next_review_slot_across_dst`
  - `window_keys_dedupe_per_goal_and_window`
  - `done_late_only_after_grace`
  - `event_reason_names_actions_in_local_time`
- [x] **Step 2:** Implement. Commit `feat(aroh-server): trigger rules, review slots and windows`.

### Task 4: Scan, replan path, reviews

**Files:** `triggers.rs`, `engine.rs`, `io.rs`, `tools.rs`, `replan.toml`, `new-goal.toml`, personas.

- [x] **Step 1: Failing integration tests:**
  - `missed_action_produces_a_proposal_without_an_owner_message`, with a second scan in the same window that starts nothing;
  - `scheduled_review_runs_from_tick`, where `next_at` moves a week;
  - `tracker_rule_breach_starts_a_replan`;
  - `no_change_replan_writes_no_proposal`.
- [x] **Step 2:** Implement. The existing api tests still pass. Commit `feat(aroh-server): adaptation triggers and weekly reviews on the tick`.

### Task 5: Learning from edits

**Files:** `learning.rs`, `workflows.rs`, `learn-preference.toml`, `preference-analyst.toml`, `io.rs`.

- [x] **Step 1: Failing tests:**
  - unit `edit_keys_group_similar_edits`;
  - integration `three_similar_edits_produce_a_preference_proposal`: two edits do nothing, the third starts a run that writes an unconfirmed preference;
  - integration `confirmed_preferences_reach_planning`: the profile is in the analyst request.
- [x] **Step 2:** Implement. Commit `feat(aroh-server): preference proposals from repeated edits`.

### Task 6: Insights API

**Files:** `insights.rs`, `api.rs`, `engine.rs`, `docs/engine-api.yaml`.

- [x] **Step 1: Failing tests:**
  - unit `run_summary_per_outcome`;
  - integration `insights_list_triggers_with_run_summaries` (user isolation included);
  - `openapi_document_is_committed`.
- [x] **Step 2:** Implement, regenerate the YAML. Commit `feat(aroh-server): insights and review schedule endpoints`.

### Task 7: App: edit command and Inbox insights

**Files:** `commands/learning.rs`, `commands/mod.rs`, `lib.rs`, `engine.ts`, `views.ts`, `views.test.ts`, `engine.test.ts`, the Inbox route.

- [x] **Step 1: Failing vitest:** `insights map to insight cards with goal titles`, and `engine.insights reads GET /v1/insights`.
- [x] **Step 2:** Implement. `pnpm check` reports 0 errors and `pnpm vitest run` passes in `node:24-slim`; `cargo check -p aroh` passes. Commit `feat(app): inbox insights and action edit command`.

### Task 8: Simulator end to end

- [x] Stack up (`e2e/up.sh`, script with a missed action and a preference analyst reply), on device `aroh-p15`.
- [x] Drive:
  1. Sign in and create the goal; the plan syncs.
  2. A past planned action: one tick makes a proposal with no owner message, shown in the Inbox.
  3. Three similar edits through `app_edit_action`, then sync; the preference suggestion appears.
  4. Insights show in the Inbox.
- [x] Screenshots go in `docs/screenshots/pod15/`. Commit `test(e2e): pod 15 triggers and learning screenshots`.

### Task 9: Report

- [x] Fill Outcome, run `graphify update .`, write the DONE line.

## Outcome

Built and verified on `feat/triggers-learning`, merged with master `23c2736` (pods 12, 13 and 14).

**Tests**

| Check | Result |
|---|---|
| `sh crates/aroh-server/tests/run.sh` (real Postgres) | 71 passed: lib 35, api 14, push 3, store 11, tools 3, triggers 5 |
| Domain, app and plugins unit tests (`rust:1-slim`) | all pass, including `flat_and_missed_rules_validate`, `edit_action_moves_resizes_deletes_and_locks`, `dismissed_insights_hold_at_most_30_uuids` |
| `pnpm vitest run`, `pnpm check`, `pnpm build` (`node:24-slim`) | 37 passed, 0 errors, build ok |
| `sh supabase/tests/run.sh` | ok, 31 tables |
| Simulator e2e on `aroh-p15` (`AROH_E2E_FLOW=pod15.js`) | 13 screenshots in `docs/screenshots/pod15/` |

The e2e covers the three integration paths on the real app: a missed workout becomes a proposal and an insight with no owner message (07, 08); three plank moves become a preference suggestion (09 to 11); confirming it and dismissing an insight both survive a reload (12, 13).

**Deviations**

- Server tasks 3 to 6 landed in one commit, with their unit and integration tests written alongside the code, as were the domain rule tests. The frontend insight tests and the insight dismissal went red first. Commit subjects differ from the ones the tasks above name.
- `aroh-domain` (rule ops `flat` and `missed`, `MISSED_GRACE_MIN`) and `aroh-app` (`edit_action`, the `dismissed_insights` setting) were changed additively, since the trigger rules and edits are shared rows.
- No edit UI: owner edits go through the `app_edit_action` command, which the e2e drives. A move, resize and delete sheet is a new component and needs a mockup first.
- Dismissed insights are a device setting (`dismissed_insights`, at most 30 ids), not synced.
- The e2e uses pod 14's `AROH_E2E_*` names and `e2e/drive-lib.js`. The pod 15 fake script is a `pod15.js` branch of `e2e/script.py`. `drive-lib.js` gained `planGoal`, replacing three copies of the goal steps, `openTomorrow` now steps to the next week when tomorrow is in it, and `show` scrolls to the first visible match, not a hidden copy. `drive.js` and `drive-calendar.js` were edited for these helpers but only syntax-checked (`node --check`), not re-run on the simulator.

**Open items**

- Calendar edit sheet (move, resize, delete) for owners: needs a mockup and an owner decision.
- The notification plugin crashes the app when a reminder fires in the foreground (`NotificationHandler.swift:79`, an assertion in `toActiveNotification`). Pending reminders also survive an app data wipe, so e2e runs uninstall the app first. Owner: reminders.
- The pg_cron and pg_net block is skipped on plain Postgres and not yet run on Supabase. Deploy needs the vault secrets `aroh_engine_url` and `aroh_service_jwt`.
- Proposal cards show the persona "Aroh" when the goal has no persona row (existing `personaFor` fallback).
- `e2e/pod12.js` still carries its own copy of the driver helpers.
