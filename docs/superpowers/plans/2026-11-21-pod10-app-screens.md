# Pod 10: App Screens on the Rebuild Store

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The approved mockup screens become the real app: a five-tab shell (Today, Calendar, Goals, Inbox, Log, Settings behind the gear) reading and writing the rebuild SQLite store through Tauri commands, working offline, with AI-only actions showing a clear "needs connection to the engine" state.

**Architecture:** The approved screen files in `src/lib/ui/screens/` stay the only copy of each screen. They stop importing `mockdata/sixpack.ts` and take their data and callbacks as props; `/mockups` passes the sample data, the app routes pass real data. One typed client module, `src/lib/app/client.ts`, calls the Tauri commands; `src/lib/app/views.ts` maps the store rows to the props of each screen. The Rust app engine logic (start-up tick, writes, reminder planning, sample fixture) lives in a new workspace crate `crates/aroh-app` as plain functions over a `rusqlite::Connection` and an `Hlc`, so Kiri tests it; the Tauri commands in `src-tauri/src/commands/app.rs` are thin wrappers. AI work goes through one engine client interface, `src/lib/app/engine.ts`, whose only implementation for now is "not configured".

**Tech Stack:** Svelte 5 runes, SvelteKit 3 adapter-static, TypeScript, Tailwind v4, Tauri 2, `tauri-plugin-notification`; Rust 2021, `aroh-domain`, `aroh-store`, `aroh-reminders`, rusqlite 0.37, chrono, chrono-tz.

## Global Constraints

- Port, do not rewrite: screen markup and classes stay as approved. A screen file changes only where its data comes from. The visual rule (Q41) is approved and not changed.
- `/mockups` keeps working and is built with `AROH_MOCKUP=1 pnpm build`.
- One copy of every helper: formatting in `src/lib/ui/format.ts`, tokens in `tokens.css`, components in `src/lib/ui/components`. Shared Rust logic in `aroh-domain`, `aroh-store` or `aroh-app`.
- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. No em dashes.
- Rust: `assert!` for internal invariants, `Err` for input validation.
- The app holds no provider keys. No Claude key in the app, the keychain, SQLite, logs or the UI.
- Do not touch `troupekit/`.
- Tests: Rust in Kiri (`HOME=/Users/chaoticqubit kiri build -f Dockerfile.test -t aroh-test-p10 .`, then `HOME=/Users/chaoticqubit kiri run --rm aroh-test-p10`); frontend `pnpm check` and `pnpm vitest run` in `node:24-slim` with the app folder mounted and a throwaway `node_modules` volume. Images and volumes are deleted after each run.
- Branch `feat/app-screens`, conventional commits, one commit per task, commit but never push, no `Co-Authored-By` trailer.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Screen data | Each screen in `src/lib/ui/screens/` takes props (data plus `on*` callbacks). `mockdata/sixpack.ts` exports the sample props; `/mockups` passes them. | One copy of every screen; the mockup stays the visual reference for the real screen. |
| Rust home | New crate `crates/aroh-app` (app engine): start-up tick, reads, writes, reminder planning, sample fixture. `src-tauri` keeps only command wrappers, notification delivery and app start. | `Dockerfile.test` excludes the Tauri crate, so logic there is untestable in Kiri. |
| Database file | `aroh-store.sqlite3` in the app data dir, opened with `aroh_store::db::open`. The MVP `aroh.sqlite3` is left untouched and unread. | The MVP file has `user_version` 1 with other tables, which the rebuild `migrate` would accept as current. The rebuild does not carry MVP data over. |
| Local user id | A uuid v7 created on first start and kept in `sync_state` under `local:user_id`. Every local row and the seeded categories use it. | Offline first: rows exist before any Supabase account. Adopting the account id at sign-in is an open item. |
| Device settings | `sync_state` keys `local:<name>` hold device-only values: `user_id`, `server_url`, `goal_budget_usd`, `calendars`, `health`, `pending_logs`. Writes go through a whitelist. | Existing local key-value table; no migration in pod 8's crate. |
| Sync | Off. The MVP sync commands and the `aroh-sync` link are removed from the app; no sync runs. | The MVP session has no user id and local rows use the local id; adoption at sign-in plus the PostgREST `Remote` is not small. Listed as an open item. |
| Start-up tick | On start, on resume and when the UI sees the local date change: seed categories (insert-or-ignore), materialize every live series for `[now, now + 14 days)` (derived rows never start in the past, so none is missed on creation), mark planned actions whose end plus 15 min grace has passed as missed, then reschedule reminders. | Q34 window, Q35 offline; deterministic missed detection replaces the MVP AI replan on device. |
| Reminders | MVP reminder timing kept (`aroh-reminders`: soon 10 min before, start, nudge 15 min after end, 48 h horizon, cap 60) over rebuild actions; plus one check-in reminder per tracker cadence time in the next 24 h. Permission is asked at the first tap, not at launch; nothing is scheduled until it is granted. | Q22 local fallback, salvaged per Q29. |
| Check-ins | A tracker with a cadence due today (daily, or weekly on today's weekday, at or after its `at` time) and no `check_in` entry today shows a question card with its first number field. Sending writes a `tracker_entries` row with source `check_in`, validated by `validate_entry`. | Q21. |
| Free logging | Text with one number and a unit that matches the unit of exactly one number field of a live tracker writes a `log` entry with the raw text. Any other text is queued in `local:pending_logs` (at most 100) for the intake agent and shown on the Log tab as waiting for the engine. | Mapping free text is AI work (Q21); offline it queues (Q35). `jobs` is server only. |
| Category colour | Category to slot by seeded name: Learning 1, Health and Fitness 2, Financial 3, Hobby and Skill 4, Career and Projects 5, Habit 6; owner-defined categories take slot `1 + index mod 6` in id order. | Matches the approved mock order; the DB hex colour is not a design token. |
| Goal dashboard | Until plugin bundles carry a view tree, the app derives a `ViewSpec` from the goal's rows: countdown (deadline), "This week" ring of actions done versus planned, one line chart per number tracker field with points, streak heatmap of the last 28 days, next 4 planned actions. | Plugin view TOML is pod 9 output; the derived view uses only approved components. |
| Proposal diff | The app reads `proposals.diff` as `{ "changes": [{ "op": "move"\|"resize"\|"create"\|"delete", "action_id": uuid or null, "title", "before": text or null, "after": text or null, "start_at": RFC 3339 or null, "end_at": RFC 3339 or null }] }`. Apply runs the changes on `actions` in one transaction and stores the previous values in `undo` as `{ "actions": [<action row before, or null for created>] }`; reject only sets the status. | Applying is deterministic and works offline; pod 9 writes this shape. |
| Inbox groups | Needs your approval: proposals with status `proposed`. Suggestions: unconfirmed preferences (Apply confirms, Reject tombstones). Insights and Runs in progress: empty until the engine writes them; empty groups are not shown. | No local table for insights or runs. |
| Engine client | `src/lib/app/engine.ts`: `Engine` interface (`status`, `startGoal`, `sendChat`, `replan`, `saveKey`, `testConnection`) with one `notConfigured` implementation that resolves every call to `{ ok: false, reason: 'needs_engine' }`. New goal, persona chat, provider key and Test connection use it. | Pod 9 is not built; one seam to swap later. |
| Needs-engine state | Expressed with approved surfaces: a persona message from "Aroh" in the chat thread, the Settings status line, and the persona-chat button's status line. | No new visual element. |
| Provider list | `src/lib/app/catalog.ts` holds the offline vendor and model list (ids, names, models, self-hosted). `mockdata/sixpack.ts` derives `providerCatalog` from it plus its sample key flags. The chosen vendor and model are a `provider_settings` row; the key goes only to the engine client. | One copy; the engine's catalog replaces it later. |
| Budgets | "Monthly budget, all goals" is the `user_month` row of `budgets`. "Monthly budget per goal" is the device setting `goal_budget_usd`, applied by the engine per goal. | `goal_month` rows need a goal id. |
| Surfaces with no mockup | Log tab (quick-log bar, waiting entries, recent entries), Goals list and empty states are composed only of approved components and markup, and added to `/mockups` for the owner to review. The quick-log bar moves from `TodayScreen` into `ui/components/QuickLog.svelte`, unchanged, and is used by Today and Log. | Q38 has five tabs; the six mocked screens cover four of them. |
| Old code | Deleted from the app: MVP routes and `src/lib/{calendar,firstrun,goals,log,plugins,replan}`, `apps/aroh/plugins/`, MVP command modules, the Claude key commands and the dev key hook, old token aliases no longer used. The crates `aroh-core`, `aroh-sync`, `aroh-intake`, `aroh-agents`, `aroh-plugins`, `aroh-replan` stay in the workspace, unlinked from the app. | Q29; `aroh-replan` (planner to salvage in pod 9) depends on `aroh-core` and `aroh-agents`. |
| Dev data | Debug builds only: command `app_dev_seed` loads the six-pack fixture from `aroh_app::sample` when there are no goals. The simulator check calls it through `AROH_DEV_SCRIPT`. | An empty store shows nothing to compare with the mockup. |

## File Structure

| Path | Responsibility |
|---|---|
| `Cargo.toml` (root) | workspace deps `aroh-store`, `aroh-app`, `aroh-reminders` |
| `crates/aroh-app/Cargo.toml` | crate manifest |
| `crates/aroh-app/src/lib.rs` | `Error`, `Result`, module list |
| `.../src/local.rs` | local user id and device settings in `sync_state` |
| `.../src/tick.rs` | seed, materialize window, mark missed |
| `.../src/snapshot.rs` | `Snapshot`: every row the screens read |
| `.../src/write.rs` | action status, entries, free log, proposals, preferences, autonomy, provider, budget |
| `.../src/remind.rs` | reminder planning over rebuild rows |
| `.../src/sample.rs` | six-pack fixture for tests and debug builds |
| `crates/aroh-app/tests/app.rs` | tick, check-in, free log, proposal apply, reminders |
| `apps/aroh/src-tauri/Cargo.toml` | app deps on `aroh-app`, `aroh-store`, `aroh-domain`, `aroh-reminders` only |
| `apps/aroh/src-tauri/src/lib.rs` | open store, tick on start and resume, register commands |
| `apps/aroh/src-tauri/src/commands/app.rs` | thin `#[tauri::command]` wrappers |
| `apps/aroh/src-tauri/src/notify.rs` | local notification delivery (MVP code, new planner) |
| `apps/aroh/src/lib/app/client.ts` | typed command client and row types |
| `apps/aroh/src/lib/app/views.ts` | rows to screen props |
| `apps/aroh/src/lib/app/views.test.ts` | mapping tests |
| `apps/aroh/src/lib/app/engine.ts` | engine client interface and `notConfigured` |
| `apps/aroh/src/lib/app/catalog.ts` | offline provider list |
| `apps/aroh/src/lib/app/data.svelte.ts` | loaded snapshot, reload on change and day change |
| `apps/aroh/src/lib/ui/screens/*.svelte` | approved screens, data as props; new `GoalsScreen`, `LogScreen` |
| `apps/aroh/src/lib/ui/components/QuickLog.svelte` | quick-log bar moved out of `TodayScreen` |
| `apps/aroh/src/lib/ui/mockdata/sixpack.ts` | sample props for every screen |
| `apps/aroh/src/routes/+layout.svelte` | phone or desktop shell around the routes |
| `apps/aroh/src/routes/{today,calendar,goals,goals/[id],goals/new,inbox,log,settings}/+page.svelte` | one route per screen |
| `apps/aroh/vite.config.ts` | vitest glob `src/lib/**/*.test.ts` |

---

### Task 1: Screens take props, mockups pass sample data

**Files:** `src/lib/ui/screens/*.svelte`, `src/lib/ui/components/QuickLog.svelte`, `src/lib/ui/mockdata/sixpack.ts`, `src/routes/mockups/+page.svelte`, `src/lib/app/catalog.ts`.

- [ ] Each screen's script replaces its `mockdata` import with typed props. Markup changes only to hide a block whose data is absent (no insight, no check-in, no target).
- [ ] Move the quick-log form into `QuickLog.svelte` with an `onlog(text)` prop; `TodayScreen` renders it.
- [ ] Add `GoalsScreen` (goal rows: category dot, title, deadline, autonomy; New goal button) and `LogScreen` (QuickLog, waiting entries, `log_list` of recent entries, due check-in cards), both from approved components; add Goals and Log to the mockup screen list.
- [ ] `AROH_MOCKUP=1 pnpm build` passes, `pnpm check` 0 errors, existing vitest passes.
- [ ] Commit `refactor(ui): screens take data as props, mockups pass sample data`.

### Task 2: aroh-app crate: local settings, tick and snapshot

**Files:** `crates/aroh-app/*`, root `Cargo.toml`.

- [ ] Tests first (`tests/app.rs`): a fresh store gets one local user id that survives a second call; `set_setting` refuses an unknown key and a non-https server URL; `tick` on the sample seeds six categories, materializes 14 days of each series from now, a second tick inserts nothing, a planned action that ended more than 15 min ago becomes missed and a done one stays done; `snapshot` returns only live rows of the range. Run: fail (no crate).
- [ ] Implement `local.rs`, `tick.rs`, `snapshot.rs`, `sample.rs`.
- [ ] Kiri tests pass. Commit `feat(aroh-app): local settings, start-up tick and snapshot`.

### Task 3: aroh-app writes and reminders

**Files:** `crates/aroh-app/src/{write.rs,remind.rs}`, `tests/app.rs`.

- [ ] Tests first: a check-in writes one `tracker_entries` row with source `check_in` and the value under the field key, and an unknown field is refused with nothing written; "82.4 kg" logs to the weight tracker with the raw text, "had pasta" is queued in `pending_logs`; applying a proposal moves its action, stores `undo` and sets `applied`, a bad diff is refused with nothing changed; rejecting only sets `rejected`; setting status `done` stamps `done_at`; reminders plan soon, start and nudge for a planned action and a check-in reminder at the cadence time.
- [ ] Implement.
- [ ] Kiri tests pass. Commit `feat(aroh-app): action, entry, proposal writes and reminder planning`.

### Task 4: Tauri app on the rebuild store

**Files:** `apps/aroh/src-tauri/{Cargo.toml,src/lib.rs,src/commands/*,src/notify.rs}`.

- [ ] Replace the MVP command modules with `commands/app.rs`: `app_snapshot`, `app_tick`, `app_set_action_status`, `app_add_entry`, `app_log_text`, `app_decide_proposal`, `app_decide_preference`, `app_set_autonomy`, `app_save_provider`, `app_save_budget`, `app_set_setting`, `app_notification_permission`, and debug-only `app_dev_seed`. Every write emits `aroh://changed` and reschedules reminders.
- [ ] `lib.rs` opens `aroh-store.sqlite3`, keeps the HLC node file, runs `tick` on start and resume. Remove the Claude key commands and the dev key hook; keep `AROH_DEV_SCRIPT`.
- [ ] `cargo check -p aroh` on the host passes. Commit `feat(app): tauri commands over aroh-store, mvp commands removed`.

### Task 5: Client, views, engine and routes

**Files:** `src/lib/app/*`, `src/routes/*`, `vite.config.ts`; delete MVP routes and `src/lib/{calendar,firstrun,goals,log,plugins,replan}`, `apps/aroh/plugins/`, unused token aliases.

- [ ] Tests first (`views.test.ts`): an action row maps to an `ActionView` with its goal title and category slot; a daily tracker due at 07:00 with no entry today yields a check-in card after 07:00 and none once an entry exists; a proposal diff maps to `ChangeView`s; a goal with a deadline and weight entries yields a countdown and a weight line chart; the engine client resolves every call to `needs_engine`.
- [ ] Implement the client, views, engine, data store, layout and routes.
- [ ] `pnpm check` 0 errors, `pnpm vitest run` passes, `AROH_MOCKUP=1 pnpm build` passes.
- [ ] Commit `feat(app): five-tab shell and screens on real data`.

### Task 6: Simulator check

- [ ] `USER=chaoticqubit pnpm tauri ios dev "iPhone 15 Pro"`, seed through `AROH_DEV_SCRIPT`, screenshot each tab and Settings, compare with the phone mockups, fix drift.
- [ ] Commit `fix(app): simulator visual fixes` when anything changed.

### Task 7: Report

DONE line: `DONE feat/app-screens <base>..<tip> VERIFIED` with test counts, screenshot paths, deviations and open items for pods 9 and 11.

## Open Items

- Sync: adopt the Supabase user id at sign-in (re-own local rows or create the account before the first write), then a PostgREST `Remote` for `aroh-store` in `aroh-sync`, and a Settings sync section (needs a mockup).
- Pod 9 engine API the app expects: catalog, save key, test connection, start goal chat with question cards and progress, persona chat, replan, insights and run progress, mapping of queued free-text logs, the proposal diff shape above, per-goal budget from `goal_budget_usd`. Engine calls need a Rust command or a CSP `connect-src` for the engine URL.
- Plugin view trees replace the derived goal dashboard.
- Undo of an applied proposal (data is stored; no approved control yet).
- Voice and photo in the quick-log bar.
- Delete `aroh-core`, `aroh-sync`, `aroh-intake`, `aroh-agents`, `aroh-plugins` once pod 9 salvages the planner from `aroh-replan`.
