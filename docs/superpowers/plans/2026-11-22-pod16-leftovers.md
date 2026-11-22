# Pod 16: Foreground Reminder Crash, Verified Prices, Realtime, Goal Chat, Leftovers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the slice 1 and slice 2 leftovers, in priority order:

1. The app no longer crashes when a local reminder fires while it is in the foreground.
2. OpenAI and Google prices are verified from the vendors' own pages.
3. Supabase Realtime on `run_events` wakes the chat instead of 1.5 s polling, with polling as the fallback.
4. A goal-scoped chat screen exists at `/goals/[id]/chat`.
5. Five UI bugs seen in screenshots are fixed.
6. A documented release iOS build works.
7. The e2e drivers share one helper file and the pod 11 and pod 14 flows pass again.

**Architecture:**
- **Crash root cause.** `tauri-plugin-notification` 2.8.1 (`ios/Sources/NotificationHandler.swift:79`) force-unwraps `notificationsMap[request.identifier]!` in `toActiveNotification`.
  - The map is in memory and filled only by `show` and `batch` in the current process.
  - A notification scheduled by an earlier process finds no entry and traps, through both `willPresent` (foreground) and `didReceive` (tap).
  - Two ways it happens: a pending reminder from an earlier launch whose `deliver` pass did not re-save it, for example after the store was wiped or a reschedule was skipped; or a tap on a reminder delivered before a relaunch.
  - 2.8.1 is the newest release (crates.io, 2026-11-10). `v2` and `v3` HEAD still carry the line. Upstream issue tauri-apps/plugins-workspace#3533 is open with no PR, so a bump does not fix it.
- **Crash fix.** `tauri-plugin-aroh-ios` installs its own `UNUserNotificationCenterDelegate` (`ReminderDelegate`) in `ArohPlugin.load`, after the notification plugin's `NotificationManager` set itself at construction.
  - The delegate is held strongly, because the center's `delegate` is weak.
  - `willPresent`: a local notification gets `[.banner, .list, .sound]`. A push-triggered one gets `[]`, the plugin's own behaviour.
  - `didReceive`: completes without calling the plugin's handler.
  - This silences the plugin's `notification` and `actionPerformed` webview events. Nothing in `apps/aroh/src` listens to either (grep), so nothing is lost.
  - No vendored patch.
- **Clearing.**
  - The mobile `deliver` cancels every pending request (`cancel_all`) before it schedules the plan. The planner owns all local notifications, so ids left by a wiped store go too. `remind::all_ids` loses its only caller and is deleted.
  - Sign-out cancels pending and delivered notifications.
  - `notify::sync` plans nothing while the store is adopted by an account and no session is stored, so a resume does not bring the reminders back. A device that never signed in keeps its reminders.
- **Prices.** `troupekit/catalog/providers.toml` gets each vendor's `price_source` (new optional `VendorSpec` field, `https://` only, validated in `Catalog::load`). Standard-tier rates go in the existing `price_verified` and `long_context` fields.
  - Pod 9's "per-model verified override" is the existing per-model `price_verified` flag, so no new mechanism is needed.
- **Realtime.**
  - The access token lives only in Rust, so the socket lives in Rust: `aroh_sync::realtime` speaks the Phoenix channel protocol over `tokio-tungstenite`. It has pure join, heartbeat and parse functions plus one reconnecting listen loop.
  - The app crate starts the listener when signed in. It emits `aroh://run` with the run id on each `run_events` insert, and `aroh://realtime` with `{ up }` when the socket opens or drops.
  - In `chat.svelte.ts`, `follow` keeps `engine.run(runId, seq)` as its only read. Only the wait changes: with the socket up, it waits for that run's wake event (at most 15 s). With the socket down, it waits the 1.5 s poll interval.
  - The e2e stack gains a `supabase/realtime` container behind the nginx gateway, and Postgres runs with `wal_level=logical`.
- **Goal chat.** `/goals/[id]/chat` renders `PersonaChat` through the existing `GoalChat` class with the goal's id. The goal page's chat button opens it, and `/goals/new` loses its `?goal=` branch. Only approved components, so no mockup.
- **Release build.** `apps/aroh/ios-release.sh` performs these steps:
  1. builds the frontend;
  2. builds the Rust static library in release mode with `tauri/custom-protocol`, so the UI is embedded and no dev server is used;
  3. copies it into `gen/apple/Externals`;
  4. runs `xcodebuild archive` with the release config and signing off, behind a no-op `pnpm` shim, because the project's "Build Rust Code" phase needs a running `tauri ios` CLI.
  - A release simulator build is launched with no dev server as proof.

**Tech Stack:** Rust 2021, tokio, tokio-tungstenite 0.29, serde_json; Swift 5 (UserNotifications); Svelte 5, TypeScript, vitest; Supabase Realtime (self-hosted image) in Kiri compose; Xcode 27.2, iOS 26.1 simulator runtime.

## Global Constraints

- No code comments. Docstrings only, directly above a declaration, 3 to 5 lines. TOML, SQL, YAML and shell files carry no comments. No em dashes.
- One copy of every helper. Compose only from approved components. A new component or visual pattern goes to open items (calendar edit sheet, all-day row).
- `assert!` for internal invariants; input validation returns `Err`.
- Shared files take additive edits: root `Cargo.toml`, the `lib.rs` handler list, `commands/mod.rs`, `docs/engine-api.yaml`, `e2e/compose.yaml`.
- No provider key or access token in the webview, SQLite, logs, sync payloads or the UI. The Realtime join carries the token from Rust only.
- `troupekit/` stays free of the host app name; `sh troupekit/scripts/check-boundary.sh` stays green.
- Simulator: device `aroh-p16`, dev port 1425 passed at run time with `--config` and never committed, e2e stack `AROH_E2E_POD=aroh-p16` with its own ports. Run `git checkout -- apps/aroh/src-tauri/gen/apple` after each build. Screenshots go in `docs/screenshots/pod16/`.
- Tests:
  - Workspace: `kiri build -f Dockerfile.test -t aroh-test-p16 .`, then `kiri run --rm aroh-test-p16`. If buildkit fails, use `rust:1-slim` with the source mounted.
  - Server: `sh crates/aroh-server/tests/run.sh`.
  - Troupekit: `kiri build -f troupekit/Dockerfile.test troupekit`.
  - Frontend: `node:24-slim`.
  - Remove own images and volumes after each run. Never two own builds at once (pod 17 builds in parallel).
- Branch `fix/slice2-leftovers`. Conventional commits, one per task, no co-author trailer. Commit, never push. Master comes in with `git merge`.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Crash fix | Own notification-center delegate in `tauri-plugin-aroh-ios` | No upstream release fixes it; a vendored patch drifts silently |
| Push in foreground | No banner, as before | Keeps the plugin's push behaviour; only local reminders change |
| Stale pending ids | Cancel every pending request before each schedule, through the app's own `reminders_cancel` Swift command | The store that named old ids may be gone. The plugin's `cancel_all` (2.8.1) fails to decode its empty arguments on iOS and its `pending` list fails to parse in Rust, so it cancelled nothing |
| Signed-out device | Plans no reminders while adopted and signed out | A sign-out clear that the next resume undoes is no clear; local-only devices keep reminders |
| Price tier | Standard rates; long-context tier where the vendor lists one (OpenAI over 272,000 input tokens, Gemini 3.1 Pro over 200,000) | The engine calls the standard endpoints |
| Dated price | `gemini-3.8-flash` at $1.50 / $7.50, the rate from 2027-01-01, not the $0.75 / $3.75 promotion that ends 2026-12-31 | A verified price must never under-count; over-counting for six weeks is safe |
| Price source | `price_source` URL per vendor in the catalog | TOML takes no comments; models are `deny_unknown_fields` |
| Realtime role | Wake-up only; the HTTP run read stays the one data path. The socket lives in Rust so the token never reaches the webview | One code path, and the fallback is the same loop |
| e2e Postgres | `aroh-e2e-pg:17` = pgvector image plus `postgresql-17-wal2json`, `wal_level=logical`, `output_plugin_libraries=wal2json` | Realtime `postgres_changes` needs wal2json, and Postgres 17.12 refuses output plugins not listed in `output_plugin_libraries` |
| Goal chat | New route reusing `GoalChat` and `PersonaChat`; `/goals/new` loses `?goal=` | One chat per goal without a mockup |
| Persona fallback | The planner persona's display name (`Goal planner`, from the `new-goal` workflow entry) instead of `Aroh` | Task item; one source for the name |

## File Structure

| Path | Change |
|---|---|
| `crates/tauri-plugin-aroh-ios/ios/Sources/ReminderDelegate.swift` | new: foreground and tap handling |
| `crates/tauri-plugin-aroh-ios/ios/Sources/ArohPlugin.swift` | install the delegate in `load` |
| `apps/aroh/src-tauri/src/notify.rs` | cancel pending before each schedule, `clear`, signed-out gate |
| `crates/tauri-plugin-aroh-ios/ios/Sources/ReminderDelegate.swift`, `ArohPlugin.swift` | `reminders_cancel` command |
| `apps/aroh/src-tauri/src/commands/engine.rs` | sign-out clears; realtime start |
| `crates/aroh-app/src/remind.rs` | `reminders_allowed`, `all_ids` removed |
| `troupekit/crates/troupekit-providers/src/catalog.rs`, `troupekit/catalog/providers.toml` | `price_source`, prices |
| `crates/aroh-sync/src/realtime.rs` | new: Phoenix channel client |
| `apps/aroh/src-tauri/src/realtime.rs` | new: listener task and events |
| `apps/aroh/src/lib/app/chat.svelte.ts`, `apps/aroh/src/lib/app/realtime.ts` | wait on wake events or the poll interval |
| `e2e/compose.yaml`, `e2e/nginx.conf`, `e2e/initdb.sql`, `e2e/pg.Dockerfile` | Realtime service, wal2json Postgres |
| `crates/aroh-server/tests/store.rs` | tamper flips a byte, 512 iterations |
| `e2e/drive-lib.js`, `e2e/drive-music.js` | one `planGoal` with a goal parameter |
| `apps/aroh/src/routes/goals/[id]/chat/+page.svelte` | new route |
| `apps/aroh/src/routes/goals/[id]/+page.svelte`, `apps/aroh/src/routes/goals/new/+page.svelte` | chat button target, `?goal=` removed |
| `apps/aroh/src/lib/ui/screens/CalendarGrid.svelte`, `apps/aroh/src/lib/ui/chart.ts` or `ChartFrame.svelte`, `NewGoalScreen.svelte` / `PersonaChat.svelte`, `apps/aroh/src/lib/app/views.ts` | UI bugs |
| `apps/aroh/ios-release.sh`, `README.md` | release build |
| `e2e/pod12.js`, `e2e/pod12.sh`, `e2e/sim.sh` | pod 12 flow on `drive-lib.js` |

### Task 1: Reproduce the foreground crash

- [x] **Step 1:** Build the current master app for `aroh-p16`.
- [x] **Step 2:** Same-process variant: schedule a reminder a minute out with the app open and watch whether it traps.
- [x] **Step 3:** Earlier-process variant: schedule, terminate, wipe the store, relaunch, and keep the app in the foreground when it fires.
- [x] **Step 4:** Record which variant traps, with the crash report frame. Shot `01-crash-before`.

### Task 2: Own notification delegate

- [x] **Step 1:** Add `ReminderDelegate.swift` and install it in `ArohPlugin.load`.
- [x] **Step 2:** Rerun the trapping variant from Task 1. Expect a banner (shot `02-foreground-banner`), the same app pid alive afterwards, no new crash report, and the delegate still ours. Commit `fix(ios): own notification delegate so foreground reminders do not crash`.

### Task 3: Clear pending reminders

- [x] **Step 1: Failing test** `reminders_wait_for_sign_in_on_an_adopted_store` in `crates/aroh-app/tests/app.rs`: a fresh store allows reminders; an adopted store allows them only with a session.
- [x] **Step 2:** Implement `remind::reminders_allowed`, cancelling every pending request in the mobile `deliver` (through `reminders_cancel` in `tauri-plugin-aroh-ios`, since the plugin's `cancel_all` fails on iOS), `notify::clear` on sign-out, and the gate in `notify::sync`. Delete `all_ids` and its assertion.
- [x] **Step 3:** On the simulator: after a wipe, pending is empty; after sign-out and a resume, pending is still empty. Commit `fix(reminders): clear pending on sign-out and on a wiped store`.

### Task 4: Verified OpenAI and Google prices

- [x] **Step 1: Failing tests** in `catalog.rs`: `price_source` must be `https://`, and the bundled catalog has OpenAI and Google models verified with the source set.
- [x] **Step 2:** Add `price_source` and fill in the prices.
  - Source: <https://developers.openai.com/api/docs/pricing>, Standard tier.

    | Model | Input / output per 1M tokens | Above 272,000 input tokens |
    |---|---|---|
    | gpt-6.2-sol | $2.00 / $10.00 | $4.00 / $15.00 |
    | gpt-6.1-terra | $1.00 / $5.00 | $2.00 / $7.50 |
    | gpt-6-luna | $0.10 / $0.50 | $0.20 / $0.75 |

  - Source: <https://ai.google.dev/gemini-api/docs/pricing>, paid tier.

    | Model | Input / output per 1M tokens | Above 200,000 input tokens |
    |---|---|---|
    | gemini-3.8-flash | $1.50 / $7.50 (rate from 2027-01-01) | none |
    | gemini-3.5-flash-lite | $0.30 / $2.50 | none |
    | gemini-3.1-pro-preview | $2.00 / $12.00 | $4.00 / $18.00 |

- [x] **Step 3:** Run the troupekit container (boundary check plus tests) and the server tests. Commit `feat(troupekit): verified OpenAI and Google prices with sources`.

### Task 5: Realtime wake-ups with polling fallback

- [x] **Step 1: Failing tests.**
  - `aroh_sync::realtime`: join frame shape, filter and token; heartbeat frame; parsing an insert into a run id; ignoring replies and other tables.
  - vitest: `waitForRun` resolves on a matching wake event, and on the poll interval when the socket is down.
- [x] **Step 2:** Implement the client, the app listener and the frontend wait.
- [x] **Step 3:** Add the Realtime container to the e2e stack.
- [x] **Step 4:** On the simulator, run a goal chat with the socket up, then stop the Realtime container mid-run and see polling finish the run. Record the wake and fallback marks.
- [x] **Step 5:** Commit `feat(realtime): run_events wake-ups with polling fallback`.

### Task 6: Goal chat screen

- [x] **Step 1: Failing test:** the goal page's chat target is `/goals/<id>/chat`.
- [x] **Step 2:** Add the route and remove `?goal=` from `/goals/new`. Shot `goal-chat`. Commit `feat(goals): goal chat screen`.

### Task 7: UI bugs

- [x] **Step 1: Failing tests where logic allows:**
  - calendar hour labels include the start hour;
  - a one-value scale has distinct ticks and labels that fit;
  - the persona fallback is the planner persona name.
- [x] **Step 2:** Fix all five bugs:
  - the 06:00 label;
  - the clipped y-axis labels;
  - the answered question card closing;
  - the "Messag" placeholder;
  - the persona name.

  Take shots, then commit `fix(ui): hour label, chart axis, answered card, composer placeholder, persona name`.

### Task 8: Release iOS build

- [x] **Step 1:** Write `apps/aroh/ios-release.sh` and the README section.
- [x] **Step 2:** Run the archive. Launch a release simulator build with no dev server and take shot `release-sim`.
- [x] **Step 3:** Commit `build(ios): documented release archive path`.

### Task 9: e2e hygiene

- [x] **Step 1:** Port `e2e/pod12.js` onto `drive-lib.js` and run it through `sim.sh` (`AROH_E2E_FLOW=pod12.js`). Delete `pod12.sh` if `sim.sh` covers it.
- [x] **Step 2:** Rerun `drive.js`, `drive-calendar.js` and `pod12.js` on `aroh-p16`.
- [x] **Step 3:** Commit `test(e2e): pod 12 flow on drive-lib`.

### Task 10: Flaky tamper test

- [x] **Step 1:** `tampered_row_fails` XORs byte 0 of the ciphertext with 1 and asserts the crypto error, 512 times per run. Commit `test(server): tamper always flips a ciphertext byte`.

### Task 11: One planGoal

- [x] **Step 1:** `planGoal(shots, planMs, goal)` takes a goal (title, first question, answers, plan text); the music flow passes its goal and `planMusic` is deleted. Commit `test(e2e): one planGoal for the six-pack and music flows`.

### Task 12: Report

- [x] Write the Outcome below: per-item verdict, test counts, shots, deviations, open items.

## Outcome

DONE. Base `cf945b6`, branch `fix/slice2-leftovers`.

| # | Item | Verdict | Evidence |
|---|---|---|---|
| 1 | Foreground reminder crash | VERIFIED | Earlier-process reminder trapped in `NotificationHandler.toActiveNotification:79` before; with `ReminderDelegate` the banner shows, same pid alive, no crash report (`01-foreground-reminder-banner.png`) |
| 1b | Pending reminders cleared on sign-out and data reset | VERIFIED | Simulator, no reinstall: 5 reminders pending from the previous run; after the data wipe and relaunch 0, signed in 5, after sign-out 0, after a signed-out resync 0 |
| 2 | Verified OpenAI and Google prices | VERIFIED | Prices and sources in Task 4; troupekit 280 tests green, boundary ok |
| 3 | Realtime wake-ups with polling fallback | VERIFIED | Socket up: 32 `run_events` wake-ups, plan in 14 s (`06-realtime-plan.png`). Realtime container stopped mid-run: `up=false`, the replan finished over 1.5 s polling (`07-realtime-fallback.png`) |
| 4 | Goal chat screen | VERIFIED | `/goals/[id]/chat` from `GoalChat` and `NewGoalScreen`; pod 11 replan runs through it (`04-goal-chat.png`, `e2e-pod11/10-replan.png`) |
| 5 | UI bugs | VERIFIED, composer NOT REPRODUCED | 06:00 label (`03`), one-value chart axis (`05`), answered cards close after typed answers (`02`, and `e2e-music/04-music-plan.png` on the final `QuestionCard` path), persona fallback `Goal planner` (vitest). "Messag" did not reproduce on this build, before or after the change; `min-w-0` and `placeholder:truncate` are defensive, and the placeholder is full in every post-send shot |
| 6 | Release iOS build | VERIFIED | `apps/aroh/ios-release.sh`: device archive succeeded (arm64, signing off); release simulator app runs with no dev server (`08-release-build-no-dev-server.png`) |
| 7 | e2e hygiene | VERIFIED | `pod12.js` on `drive-lib.js`, `pod12.sh` removed; pod 11 (with the background reminder) and pod 14 pass on a fresh `aroh-p16` stack with Realtime up throughout; pod 12 and music pass on the same build (`e2e-pod11/`, `e2e-pod12/`, `e2e-pod14/`, `e2e-music/`) |
| 8 | Flaky `tampered_row_fails` | VERIFIED | Tamper XORs byte 0 with 1, 512 iterations per run; server suite green |
| 9 | One `planGoal` | VERIFIED | `planGoal(shots, planMs, goal)` serves the six-pack and music flows; `planMusic` deleted |

### Tests

- Workspace (Kiri `aroh-test-p16`): 209 passed, 0 failed. The first run caught `aroh-plugins` contract drift from `question_card.answered`; the flag moved to the chat message (`89d4154`).
- Server (`run.sh`): 81 passed, 0 failed.
- Troupekit (container): 280 passed, 0 failed, boundary ok.
- Frontend (`node:24-slim`): vitest 47 passed (8 files), `svelte-check` 0 errors 0 warnings, `pnpm build` ok.

### Deviations

- No `graphify-out/` in this repo; the graph gate was skipped.
- Reminder cancelling moved from the plugin's `cancel_all` to a `reminders_cancel` command in `tauri-plugin-aroh-ios`, found during simulator verification.
- The e2e Postgres became a small built image (`e2e/pg.Dockerfile`) for wal2json.
- Simulator builds use the README hand build (Rust staticlib, then `xcodebuild` with a `pnpm` shim) once `tauri ios dev` was stopped; `TAURI_CONFIG` sets the dev URL to port 1425 so nothing is committed.
- The first music run failed once on a sync request (`remote: network: error sending request`). Cause unknown; Realtime and the gateway were restarted before the rerun, which passed.
- The 272K OpenAI threshold is read as 272,000 input tokens.

### Open items

- Calendar edit sheet and the all-day events row need mockups before they can be built.
- `gemini-3.8-flash` is priced at the 2027-01-01 rate; the promotion ends 2026-12-31.
- Production Supabase needs Realtime enabled with `run_events` in the `supabase_realtime` publication (the migration adds it when the publication exists).
- pg_cron and pg_net deploy is still open from pod 15.
- A Realtime channel that joins but whose Postgres subscription then fails reports `up` while no events arrive, so runs fall back to the 15 s safety net (seen once, before wal2json was allowed: plan in 57 s against 14 s). Treating a subscription error after the join as a drop would close it.
- Upstream: `tauri-plugin-notification` 2.8.1 iOS `cancel_all` decode failure and `PendingNotification` missing `schedule`; drop `reminders_cancel` once fixed.
