# Pod 13: Apple Health and Push

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** On iPhone, Apple Health body mass, workouts, steps and dietary energy land as tracker entries for trackers that declare a health source, imported incrementally and never twice, and synced like any other entry. The server engine sends APNs pushes for reminders, check-ins and proposals to registered devices, while the app keeps the next 24 h scheduled locally as the offline fallback, with an explicit rule so a reminder is not delivered twice.

**Architecture:** One thin Tauri iOS plugin, `tauri-plugin-aroh-ios`, holds the Swift: HealthKit authorization, anchored queries, daily totals, a debug-only sample seeder, and remote notification registration through the app delegate. All mapping, dedupe and anchor bookkeeping is pure Rust in `aroh-app::health`, tested in Kiri. A tracker declares a health source per field (`TrackerField.health`), so the declaration lives in the existing `fields` JSON column. Entry ids are `derived_id` of the tracker and the HealthKit sample uuid (or the local day for daily totals), so a re-import, a lost anchor or a second iPhone collapses under LWW; the anchor is only a cursor. The reminder planner moves into `aroh_domain::remind` as a pure function over domain rows (on top of `aroh-reminders`), so app and server compute the same reminders with the same `reminder_id`. The server stores device tokens in a server-only `devices` table, claims each push in a `push_sent` ledger before sending, signs APNs requests with an ES256 JWT from the .p8 key, and sends through the injected `Transport`, so tests use `FnTransport` as the fake APNs.

**Tech Stack:** Rust 2021, rusqlite, chrono, chrono-tz, jsonwebtoken 9 (ES256), axum 0.8, sqlx-postgres, utoipa; Swift (HealthKit, UserNotifications, UIKit); Svelte 5, TypeScript, vitest; Kiri; Xcode simulator tools (`xcrun simctl`, `xcodebuild test` for system sheets).

## Global Constraints

- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. TOML, SQL, YAML, shell carry no comments. No em dashes.
- One copy of every helper (Q42). Approved components and the Q41 visual rule only; no new component or visual pattern.
- The .p8 key, key id and team id come from the environment only and are never logged; `Debug` of the config redacts the key. Device tokens are never logged; logs name the device id.
- `assert!` for internal invariants; input validation returns errors.
- Shared files take additive edits only: root `Cargo.toml`, `lib.rs` handler list, `commands/mod.rs`, `docs/engine-api.yaml`, the server router, `Info.plist`, `project.yml`, entitlements, `capabilities/default.json`, `Dockerfile.test`.
- Tests in Kiri: `kiri build -f Dockerfile.test -t aroh-test-p13 .` then `kiri run --rm aroh-test-p13` (fallback `rust:1-slim` with the source mounted); server `sh crates/aroh-server/tests/run.sh`; frontend `pnpm check` and `pnpm vitest run` in `node:24-slim`. Remove own images and volumes after each run; `df -h /` before big builds.
- Branch `feat/health-push`, conventional commits, one commit per task, commit and never push, no `Co-Authored-By` trailer.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Health source | `TrackerField.health: Option<HealthMetric>` with `body_mass`, `workout_minutes`, `workout_energy`, `workout_type`, `steps`, `dietary_energy`. Serde default, skipped when `None`. Validation: `workout_type` needs a text field, the rest a number field. | No tracker type declares a source today. The `fields` column is JSON, so no SQL or SQLite migration. Breaks `TrackerField { .. }` literals in other branches (leader told). |
| Sample shapes | Body mass and workouts are one entry per sample. Steps and dietary energy are one entry per local day, holding the day's total from `HKStatisticsQuery` (cumulative sum, so iPhone and Watch are not double counted), recomputed for every day an anchored query reports new samples on. | Raw step samples arrive every few minutes; a daily total is what a tracker shows. |
| Entry id and dedupe | `derived_id(tracker_id, "health:<uuid>", 0)` for samples, `derived_id(tracker_id, "health:<metric>:<YYYY-MM-DD>", 0)` for days. Import writes an entry only when it is missing, deleted or holds different data; otherwise it is skipped. Deleted HealthKit samples soft-delete their entry. | The same sample can never become two entries, whatever the anchor says. Millis 0 keeps the id derivable from the uuid alone, which deletes need. |
| Anchor | One anchor per HealthKit type, base64 of the archived `HKQueryAnchor`, in `sync_state` under `local:health_anchor:<type>`, written in the same transaction as the entries. First import reads the last 30 days. | A failed import leaves the old anchor, and the derived ids absorb the repeat. Device-only, never synced. |
| Mapping place | `aroh-app::health::{plan_import, import}`: pure mapping from a `HealthBatch` (JSON from Swift) to entries per tracker, then the transaction. Swift only queries and serializes. | Testable in Kiri; Swift stays thin. |
| Units | Body mass in the field's unit when it is `kg`, `lb` or `g`, else kg. Energy in kcal. Workout minutes from duration. Workout type is the activity name. | Field units are free text; these are the ones the planner uses. |
| When import runs | On "Connect Apple Health" (switch on), on start and on resume while the `health` setting is `true`, on iOS only. No background delivery. | Background delivery needs another entitlement and an observer query; start and resume cover the MVP. |
| Settings control | The existing approved Apple Health switch in Settings, Access. Turning it on authorizes and imports; its description line shows the result ("Imported 3 entries." or the error). Off stops imports. | Approved row and switch; only the description text changes, so no mockup. |
| Permissions | Release requests read for the four types. Debug builds also request share for them so the seeder can write test samples. Info.plist and `project.yml` get `NSHealthShareUsageDescription` and `NSHealthUpdateUsageDescription`; entitlements get `com.apple.developer.healthkit` and `com.apple.developer.healthkit.access` `[]`. | Production stays read-only; the update string is required for the debug share request. |
| Device token | `tauri-plugin-aroh-ios` adds `application:didRegisterForRemoteNotificationsWithDeviceToken:` and `...didFailToRegister...` to the app delegate class at runtime and calls `registerForRemoteNotifications`; `register` resolves the hex token or rejects with the error. Desktop and Android reject `unsupported`. | Tauri has no delegate hook for remote notifications. The notification plugin's `UNUserNotificationCenter` delegate stays as is. |
| Device registration | `PUT /v1/devices/{device_id}` body `{platform: "ios", token, environment: "sandbox"|"production", zone, synced_seq, scheduled_until}`; `DELETE /v1/devices/{device_id}`. `device_id` is a uuid the app creates once (`local:device_id`). A token registered by another user moves to the new user. The app registers after every sync when signed in and deletes at sign-out. | Token stays out of the path and logs. IANA zone is needed for check-in times. |
| Push vs local dedupe | The device reports `synced_seq` (lowest pull cursor of `actions` and `trackers`) and `scheduled_until` (now plus 24 h, or the last planned fire time when the plan hit the cap, or now when notifications are not granted). The server pushes a reminder or check-in only when its row's `server_seq` is above `synced_seq` (the device never saw it) or its fire time is after `scheduled_until`. Proposals are always pushed. As a second guard the push carries `apns-collapse-id` equal to the local notification id (`reminder_id`), so iOS keeps one entry if both arrive. | A suspended app cannot cancel a local notification when a push arrives, so the server decides. Exact, uses the existing global `aroh_sync_seq`, no clock skew. |
| Local horizon | Action reminders are planned 24 h ahead (was 48 h), check-ins 24 h. | Q22: next 24 h locally; one horizon makes `scheduled_until` exact. |
| One planner | `aroh_domain::remind::plan(goals, actions, trackers, now, zone) -> Vec<Planned>` (moved from `aroh-app::remind`; in aroh-domain because it takes domain rows and aroh-reminders sits below aroh-domain), `Planned` gains `kind` and `row_id`. `aroh-app::remind::plan` loads rows and calls it; the server does the same over Postgres rows. | Q42; same ids and texts on both sides. |
| APNs auth | Env `AROH_APNS_KEY` (PEM text of the .p8), `AROH_APNS_KEY_ID`, `AROH_APNS_TEAM_ID`, `AROH_APNS_TOPIC` (bundle id). The APNs host comes from each device's registered `environment`, so one engine serves debug and release builds. All unset turns push off; a partial set fails start. JWT ES256 with `kid` and `iss`, `iat` cached and refreshed after 50 min. | Token-based auth per Apple; other pods' stacks start without keys. |
| APNs request | `POST https://api[.sandbox].push.apple.com/3/device/<token>`, headers `authorization: bearer <jwt>`, `apns-topic`, `apns-push-type: alert`, `apns-priority: 10`, `apns-collapse-id`, `apns-expiration` (fire time plus 1 h for reminders, 24 h for proposals). Payload `{aps: {alert: {title, body}, sound: "default", thread-id: <kind>}, aroh: {kind, id, goal_id}}`. | Apple's HTTP/2 provider API; `aroh` lets the app route a tap. |
| Dispatch | `Pusher::dispatch_once`: per device, reminders with fire time in the last 5 min, proposals created in the last 24 h and after the device registered; claim `(device_id, key)` in `push_sent` with `insert ... on conflict do nothing returning`, then send. 410, or 400 `BadDeviceToken`, deletes the device. 429, 5xx and transport errors release the claim for the next pass. Ledger rows older than 7 days are deleted. Worker mode runs it every 30 s; step mode calls `POST /v1/push/tick` (service role). Attached with `Engine::with_push`. | Concurrent ticks never double send; other pods' `Deps` literals untouched. |
| Push tables | Migration `20261123130000_push_devices.sql`: `devices` and `push_sent`, revoked from `anon` and `authenticated`, granted to `service_role`, like `user_keys`. | Tokens are server-only. |
| Simulator proof | Device `aroh-p13`. Build per the leader's recipe (tauri ios dev serves port 1421 for pod 13, passed at run time with `--config` and `vite --port`, `xcodebuild` builds, `simctl install`). HealthKit's sheet has no `simctl privacy` service and the Mac has no GUI session, so an XCUITest runner (`e2e/pod13/`, own tiny Xcode project) launches the app with `AROH_DEV_SCRIPT` and taps the sheet. Fallback: patch the simulator's `healthdb.sqlite` authorization rows like `e2e/grant.py`. Notifications granted with `e2e/grant.py`. Samples written by the debug seeder. Push banner: app in background, `xcrun simctl push aroh-p13 io.aroh.app e2e/pod13/payload.json`, where `payload.json` is checked by a server test to equal the builder's output. | Real flows, no direct entry seeding. |

## File Structure

| Path | Responsibility |
|---|---|
| `crates/aroh-domain/src/rows.rs` | `HealthMetric`, `TrackerField.health`, its validation |
| `crates/aroh-domain/src/remind.rs` | pure `plan` over domain rows, `Planned` with `kind` and `row_id`, `HORIZON_HOURS = 24`, `scheduled_until` |
| `crates/aroh-app/src/remind.rs` | loads rows, calls the shared planner, `synced_seq` |
| `crates/aroh-store/src/sync.rs` | `pulled_seq`, the one reader of a table's pull cursor |
| `crates/aroh-app/src/health.rs` | `HealthBatch`, `plan_import`, `import`, `wanted_types` |
| `crates/aroh-app/src/local.rs` | health anchors, device id |
| `crates/aroh-app/tests/health.rs` | mapping, dedupe, anchors, day totals, deletes |
| `crates/tauri-plugin-aroh-ios/` | Swift HealthKit and remote notification bridge, one `call` wrapper, desktop answers `unsupported` |
| `apps/aroh/src-tauri/src/commands/health.rs` | `app_health_connect`, `import_if_enabled`, `app_dev_health_seed` |
| `apps/aroh/src-tauri/src/commands/push.rs` | `register` after each sync, `unregister` before sign-out |
| `apps/aroh/src-tauri/{Cargo.toml,src/lib.rs,src/commands/mod.rs,capabilities/default.json}` | wiring |
| `apps/aroh/src-tauri/gen/apple/{project.yml,aroh_iOS/Info.plist,aroh_iOS/aroh_iOS.entitlements}` | usage strings, HealthKit and `aps-environment` |
| `apps/aroh/src/lib/app/client.ts`, `apps/aroh/src/routes/settings/+page.svelte`, `apps/aroh/src/lib/ui/screens/{SettingsScreen.svelte,types.ts}`, `apps/aroh/src/lib/app/views.ts` | Connect Apple Health status line |
| `supabase/migrations/20261123130000_push_devices.sql` | `devices`, `push_sent` |
| `crates/aroh-server/src/push.rs` | config, signer, payload and request builders, dedupe rule, device store, `Pusher` |
| `crates/aroh-server/src/{api.rs,engine.rs,main.rs,lib.rs,config.rs}` | routes, `with_push`, worker loop |
| `crates/aroh-server/tests/push.rs` | registration and dispatch against Postgres with fake APNs; route and role checks in `tests/api.rs` |
| `docs/engine-api.yaml` | regenerated |
| `Dockerfile.test` | exclude `tauri-plugin-aroh-ios` |
| `e2e/pod13/` | `sim.sh`, `drive.js`, `payload.json`, XCUITest project |
| `docs/screenshots/pod13/` | screenshots |

---

### Task 1: Simulator spikes

- [x] Build and install the current app on `aroh-p13` with the leader's recipe; record the commands in `e2e/pod13/sim.sh`.
- [x] XCUITest runner from `xcodebuild test` launches `io.aroh.app` headless and taps a system alert. Fallback: `healthdb.sqlite` authorization patch. Neither: the Health leg is NOT VERIFIED.
- [x] `codesign -d --entitlements -` on the simulator build shows the HealthKit entitlement once added.
- [x] `registerForRemoteNotifications` in the simulator: token or failure callback reached through the added delegate methods.
- [x] `simctl push` with the app in the background shows a banner in a `simctl io screenshot`.

### Task 2: Health source on tracker fields

- [x] Failing tests in `rows.rs`: a field with `health: steps` round-trips and is omitted when `None`; `workout_type` on a number field and `steps` on a text field are invalid.
- [x] Implement; fix every `TrackerField` literal in the workspace. Kiri green. Commit `feat(aroh-domain): health source on tracker fields`.

### Task 3: One reminder planner

- [x] Failing tests in `aroh_domain::remind`: `plan` over rows gives the same reminders and ids as before, `kind` and `row_id` set, nothing after 24 h.
- [x] Move the planner, keep `aroh-app::remind::plan` as the loader, add `scheduled_until`. Kiri green. Commit `refactor(aroh-domain): one reminder planner for app and server, 24 h horizon`.

### Task 4: Health import mapping

- [x] Failing tests in `crates/aroh-app/tests/health.rs`: body mass to a kg and an lb field; workout to minutes, energy and type; steps day total; importing the same batch twice writes once; a changed day total updates the entry; a deleted sample soft-deletes; anchors stored with the entries and absent after a failed import; trackers without health fields get nothing; `wanted_types` lists only declared types.
- [x] Implement `health.rs` and the local anchor and device id helpers. Kiri green. Commit `feat(aroh-app): apple health import with anchors and derived ids`.

### Task 5: Server push

- [x] Failing unit tests in `push.rs`: config all-or-nothing and redacted `Debug`; JWT ES256 verifies with the public key and is reused for 50 min; payload and request headers for reminder, check-in and proposal; dedupe rule cases (unknown seq, beyond `scheduled_until`, known and covered); `e2e/pod13/payload.json` equals the builder's check-in payload.
- [x] Failing integration tests in `tests/push.rs`: register, re-register by another user moves the token, delete; dispatch sends one push for an unknown action reminder with the right headers, sends nothing on the second pass, skips a covered reminder, sends a proposal, deletes the device on 410, releases the claim on 500; `/v1/push/tick` needs the service role; drift test against `docs/engine-api.yaml`.
- [x] Implement migration, `push.rs`, routes, `Engine::with_push`, worker loop, config; regenerate `docs/engine-api.yaml`. `sh crates/aroh-server/tests/run.sh` green. Commit `feat(aroh-server): device tokens and apns push with dedupe`.

### Task 6: iOS plugins and app wiring

- [x] `tauri-plugin-aroh-ios` (`health_authorize`, `health_fetch`, `health_seed` debug, `push_register`), commands `health.rs` and `push.rs`, start and resume import, register after sync, delete at sign-out, plist, project.yml, entitlements, capabilities, `Dockerfile.test` excludes.
- [x] Settings status line; `pnpm check` 0 errors, `pnpm vitest run` green; `cargo check -p aroh` and the simulator build pass. Commit `feat(app): apple health connect, import and push registration`.

### Task 7: Simulator end to end

- [x] `e2e/pod13/sim.sh`: grant notifications, launch through the XCUITest runner, drive seeds a health tracker and HealthKit samples, turns on Apple Health, the sheet is tapped, entries appear on the Log or goal screen; app to background, `simctl push` the payload, banner screenshot. Screenshots in `docs/screenshots/pod13/`.
- [x] Commit `test(e2e): pod 13 health import and push banner on the simulator`.

### Task 8: Report

- [x] Fill Outcome, `graphify update .` if a graph exists, DONE line.

## Outcome

Done on `feat/health-push`, commits 3091409 (plan) to the Task 8 commit.

- **Tests.** Workspace without the server: 155 passed, 1 ignored (Kiri, `rust:1-slim`). Server with Postgres: 48 passed (23 unit, 8 api including the device routes and the OpenAPI drift test, 3 push, 11 store, 3 tools). `supabase/tests/run.sh`: rls ok. Frontend: `svelte-check` 0 errors, vitest 23 passed. `cargo check -p aroh` clean; the simulator build passes.
- **Simulator.** `sh e2e/pod13/sim.sh build` then `sh e2e/pod13/sim.sh` on `aroh-p13` passed: the HealthKit sheet was answered, 4 entries imported (two weights, one workout with minutes and type, one steps day total) and show on the Log and goal screens, `registerForRemoteNotifications` returned a 160 hex char token through the delegate hook (debug probe, token never printed), and a `simctl push` banner showed with the app in the background. Each run erases `aroh-p13` so the HealthKit sheet appears again. Screenshots in `docs/screenshots/pod13/`.
- **Deviations.**
  - One plugin crate, `tauri-plugin-aroh-ios`, instead of two, so the Rust call wrapper exists once.
  - The planner lives in `aroh_domain::remind`, not `aroh-reminders`: it takes domain rows and aroh-reminders sits below aroh-domain.
  - No `AROH_APNS_ENV`: each device registers its own `environment`, so one engine serves debug and release builds.
  - `TrackerField` literals in other crates gained `health: None`.
  - The Tauri window was not key on a fresh launch, so HealthKit loaded its sheet but never showed it. The bridge makes the window key before asking.
  - Simulator runs need `tauri ios dev` on port 1421, passed at run time.
  - Push registration after sync runs in a spawned task and the Swift side gives up after 20 s, so sync never waits on APNs.
  - The RLS fixture loop now skips tables without `user_id`; `push_sent` is keyed by device.
- **Open for the owner.** Real APNs needs: the APNs auth key (.p8) as `AROH_APNS_KEY`, its key id as `AROH_APNS_KEY_ID`, the team id as `AROH_APNS_TEAM_ID`, the topic `io.aroh.app` as `AROH_APNS_TOPIC`, the Push Notifications and HealthKit capabilities on the App ID, and `aps-environment` set to `production` for release builds.
- **Follow-ups.** The goal planner and plugins do not yet emit `health` on tracker fields, so only trackers created with it import. No HealthKit background delivery; imports run at connect, start and resume.
