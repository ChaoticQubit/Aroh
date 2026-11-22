# Pod 22: Travel and Time Zones

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal (Q54, Q53):** When the device time zone changes, flexible actions and every action series follow local wall time in the new zone (07:00 stays 07:00 where the owner is), locked actions and device busy blocks keep their real instant, a move that would clash with a locked action or a timed busy block or land in the past is not made, reminders are rescheduled, the change syncs, the server's stored zone (weekly reviews, server-started runs) follows, and the owner gets one Inbox note listing what shifted and what was left. Flying back home shifts again the same way.

**Architecture:** One pure shift planner in `aroh-domain` (`travel.rs`, Q42 one copy) that takes the old and new zone, `now`, the owner's live actions, series and busy blocks, and returns the rows to write and the note lines. `aroh-app::travel::follow` reads the store, runs the planner, writes the rows, the note and the stored zone in one transaction. The Tauri shell calls it first in `on_start_or_resume`, before the tick, the reminder reschedule and the sync. The UI reads its zone from the snapshot so the screens and the store never disagree. A new server endpoint `PUT /v1/timezone` moves every review schedule of the owner to the new zone through the existing `ensure_review`.

**Tech Stack:** Rust 2021, chrono, chrono-tz, rrule, rusqlite, sqlx/Postgres, axum; Svelte 5 runes, TypeScript, vitest; Kiri; Xcode simulator tools.

## Global Constraints

- Settled decisions: `docs/agent-decisions.md` Q34, Q41, Q42, Q53, Q54.
- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. TOML, SQL, YAML, shell carry no comments. No em dashes.
- `assert!` for internal invariants; input validation returns errors. One copy of every helper.
- No provider keys in the app. `troupekit/` stays free of "aroh" and is not edited.
- Tests first for every task, run red before code. Only small tests for critical logic.
- Tests in Kiri, always `export HOME=/Users/chaoticqubit` first: workspace `kiri build -f Dockerfile.test -t aroh-test-p22 .` then `kiri run --rm aroh-test-p22`; server `sh crates/aroh-server/tests/run.sh`; frontend `pnpm check` and `pnpm vitest run` in `node:24-slim`. Remove only your own images and volumes after each run (`kiri rmi aroh-test-p22`). Never touch `pod-a-*` or other pods' resources.
- Pod 20 works in parallel on `crates/aroh-app/src/write.rs` (`move_off`, `edit_action`), the edit sheet and the server locked-action rules, and adds `busyDayInsights` to `apps/aroh/src/lib/app/views.ts` merged in `inboxView`. Do not edit those functions. Do not change `inboxView`'s return shape; add local notes inside `insightItems`.
- `graphify-out` is a symlink to the main checkout's graph and is not ignored by the `/graphify-out/` rule: stage files by name, never `git add -A` or `git add .`.
- Simulator device `aroh-p22` (iPhone 15 Pro), dev port 1429, never committed. Screenshots to `docs/screenshots/pod22/`. `git checkout -- apps/aroh/src-tauri/gen/apple` afterwards.
- Branch `feat/travel-time-zones`, conventional commits, one commit per task, commit and never push, bring in master with `git merge master` before DONE (never rebase), no `Co-Authored-By` trailer.

## Facts from the code

- Series store `dtstart_local` with `timezone` (`aroh-domain/src/recur.rs`); occurrence ids are `occurrence_id(series_id, instant)` and `repo::materialize` skips instants already in `actions.occurrence` (tombstones included). Changing only `ActionSeries.timezone` would materialize a second row beside each old one in the 14-day window.
- The server materializes with the same domain function (`aroh-server/src/rows.rs`), so ids must stay derivable: a row with an id from the old instant would race the server's row for the new instant.
- `iana_time_zone::get_timezone` on Darwin calls `CFTimeZoneResetSystem` before `CFTimeZoneCopySystem`, so `zone()` sees a change on resume without a relaunch on a real device. `zone()` falls back to UTC when the OS reports nothing.
- The server's zone lives in `review_schedules.timezone` per goal and changes only through `ensure_review` on goal and replan requests; `goal_zone` reads it for server-started runs. Push reminders use `devices.zone`, which `PUT /v1/devices/{id}` already refreshes after each sync.
- `/v1/insights` is server-only and empty offline. The UI's zone is `Intl` in the web view (`lib/ui/clock.ts`, `lib/app/engine.ts`), read once at component init; the layout re-keys only on `data.day`.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Shared place | `aroh-domain/src/travel.rs`: `wall_instant(zone, NaiveDateTime) -> DateTime<Utc>` (gap safe: earliest of an ambiguous time, the first valid minute after a gap) and `plan_shift(from, to, now, actions, series, busy) -> Shift`. Pure, no store. `aroh-app/src/travel.rs` is the only caller. | Q42. Testable without a store; the server could reuse it. The other inline gap walkers (reminders, replan, intake) are another pod's sweep. |
| What shifts | Non-series actions: live, `planned`, not `locked`, `start_at >= now`; new start = `wall_instant(to, start_at in from)`, same length. | Past and finished rows never move. `from` is the stored owner zone. |
| Series | Every live series whose `timezone != to` gets `timezone = to` (same `dtstart_local`, same rule), so every future occurrence, materialized later on device or server, is in the new zone. | "Future occurrences of series are covered." |
| Series occurrence rows | For each live row of such a series whose old occurrence or new occurrence (`wall_instant(to, occurrence in series' old zone)`) is at or after `now`: tombstone it and write a re-keyed copy with `id = occurrence_id(series, new_occ)`, `occurrence = new_occ`, real HLC stamp. Planned, unlocked rows whose new start is at or after `now` with no clash also take the new start and end. Locked, done, missed, skipped rows and rows left by a clash keep their instant. | The re-keyed row's id matches what every engine derives, its real stamp beats any derived twin under LWW, and its `occurrence` stops materialize from adding a twin. A locked or done occurrence still holds its day. |
| Clash | A new span that overlaps a live locked action or a live timed (not all-day) busy block is not made; the row keeps its instant and the note says why. A new start before `now` is not made either. A re-keyed id that already belongs to another live row is left and reported. | Q53: never moved silently into a clash. All-day blocks are governed by day kinds (pod 18). |
| Stored zone | Device-only `local:zone` in `sync_state` (not in `SETTINGS`). Missing: record the current zone, shift nothing. Unknown OS zone (`device_zone()` is `None`): skip. Shift rows, note, `local:zone` and `local:zone_pending` written in one transaction. | No double shift after a crash; a transient OS failure never shifts everything to UTC; an upgrade never shifts. |
| Order | `on_start_or_resume`: `travel::follow`, then `tick`, then `notify::sync`, then sync. | Reminders and materialize see the new zone. |
| Inbox note | Device-local notes in `local:notes` (JSON, newest first, at most 30, uuid ids so `dismissed_insights` accepts them) through `local::notes` and `local::add_note`. Snapshot gets `notes` and `zone`. `insightItems` in `views.ts` shows them as insight cards (persona "Planner"), dismissal through the existing setting. One note per shift: title "Time zone changed", body lists the new and old zone, actions moved (title, new local time), fixed actions kept (title, new local time), and actions not moved with the reason (clash with title, would be in the past). Lists cut at 10 lines with "and N more". | Works offline; pod 20 has no note store (pod 20 answer 2026-11-22). |
| Server zone | `PUT /v1/timezone {timezone}`: `ensure_review(user, goal, tz)` for every review schedule of the user; 200 with the count. The app calls it after a successful sync while `local:zone_pending` is set, then clears it. | A device that did not travel never sends it, so a second device at home does not flip the zone back. Offline retries on the next sync. One copy of the recompute. |
| UI zone | `lib/ui/clock.ts` holds the store zone: `setDeviceZone(snapshot.zone)` on every load, `deviceZone()` returns it before `Intl`; `getClock()`, `data.svelte.ts` `localToday()` and every engine request use `deviceZone()` (the private `engine.ts` `zone()` is removed); the layout key becomes `data.day + data.zone`. The vitest checks `deviceZone()`, since `getClock()` needs a component context. | Screens and requests use the zone the store was shifted to; a resume re-renders. One copy of the zone source. The Task 0 spike shows the web view keeps the old zone, so this is required, not a nicety. |
| Re-key collision | Inside one shift a row's new id may be another row's old id (two daily times as far apart as the zone offset): the live copy is written and that id is not tombstoned. A target held by a row that is not re-keyed is reported as "its new slot is taken". | Found while writing Task 1; LWW then keeps one live row per id. |
| Debug zone | None: the Task 0 spike shows `SIMCTL_CHILD_TZ` reaches `zone()`, so `AROH_DEV_ZONE` is not added. | Simulator proof without changing the Mac's zone and without debug-only code. |
| Rejected | Shifting series rows in place with old ids (twins on the server); tombstone and plain re-materialize (loses locked and done occurrences, no clash check); server insights for the note (offline empty); storing the zone in owner `SETTINGS` (owner could break detection). | |

## File Structure

| Path | Change |
|---|---|
| `crates/aroh-domain/src/travel.rs`, `lib.rs` | `wall_instant`, `Shift`, `plan_shift`, unit tests |
| `crates/aroh-app/src/travel.rs`, `lib.rs`, `local.rs`, `snapshot.rs` | `follow`, stored zone, notes, snapshot `notes` and `zone` |
| `crates/aroh-app/tests/travel.rs` | store-level tests |
| `apps/aroh/src-tauri/src/lib.rs` | `device_zone`, `zone`, call order |
| `apps/aroh/src-tauri/src/commands/engine.rs` | `PUT /v1/timezone` after sync while pending |
| `crates/aroh-server/src/triggers.rs`, `api.rs`, `tests/triggers.rs`, `docs/engine-api.yaml` | endpoint, test, regenerated spec |
| `apps/aroh/src/lib/ui/clock.ts`, `lib/app/data.svelte.ts`, `engine.ts`, `views.ts`, `client.ts`, `routes/+layout.svelte`, tests | zone from snapshot, notes as insight cards |
| `e2e/pod22.js`, `e2e/pod22.sh` | simulator flow |

## Tasks

### Task 0: Simulator zone spike

- [ ] Build and install the debug app on `aroh-p22` (port 1429, recipe of `e2e/pod18.sh` and `e2e/sim.sh`). Launch with `SIMCTL_CHILD_TZ=America/New_York` and check separately what Rust `zone()` reports (log line or `app_dev_push_probe`) and what the web view's `Intl.DateTimeFormat().resolvedOptions().timeZone` reports (dev script). Try a relaunch with another zone. Write the working method into this plan under Simulator method. If Rust does not see `TZ`, add the debug-only `AROH_DEV_ZONE`.

### Task 1: Shift planner in aroh-domain

- [ ] Tests first in `travel.rs`, run red: Berlin to New York, a flexible 07:00 action stays 07:00 New York; a locked 10:00 New York call keeps its instant; a daily series gets `timezone = America/New_York`, its future occurrence rows are re-keyed to `occurrence_id(series, new_occ)` at 07:00 New York; across the New York DST end (fly 2026-10-28, occurrences to 2026-11-03) every occurrence is 07:00 local and the UTC offset changes on 2026-11-01; a 02:30 action on the New York spring gap day lands at 03:00; a move onto a locked action or a timed busy block is left and reported, an all-day block does not block; a new start before `now` is left; a locked series occurrence is re-keyed and keeps its instant; flying west, today's done occurrence is re-keyed so no twin appears (run `recur::materialize` over the result's occurrences and assert one row per local day); flying back restores the original instants; same zone gives an empty shift.
- [ ] Implement. Commit `feat(domain): shift plan for a time zone change`.

### Task 2: Follow the device zone in the app

- [ ] Tests first in `crates/aroh-app/tests/travel.rs` on a real store: first call records the zone and changes nothing; a change writes the shifted rows, the series, one note, the new `local:zone` and pending flag; a second call with the same zone changes nothing; a following `tick` adds no twin occurrence; `None` zone does nothing; the snapshot carries `notes` and `zone`; notes keep at most 30.
- [ ] Implement `aroh-app::travel::follow`, `local` notes and zone keys, snapshot fields; `device_zone()` and the call order in the Tauri shell. Commit `feat(app): follow the device time zone`.

### Task 3: Server zone

- [ ] Test first in `crates/aroh-server/tests/triggers.rs`: two goals with reviews in Berlin, `PUT /v1/timezone` with New York moves both, recomputes `next_at` at the same local weekday and time, an unknown zone is 400, another user's reviews do not change.
- [ ] Implement the endpoint, regenerate `docs/engine-api.yaml` (`aroh-server openapi`), call it from the app after sync while pending. `sh crates/aroh-server/tests/run.sh` green. Commit `feat(server): move review schedules to the owner's new zone`.

### Task 4: UI

- [ ] vitest first: `insightItems` shows a local note as an insight card and hides a dismissed one; `getClock()` uses the snapshot zone when set.
- [ ] Implement the zone source, the layout key and the note cards. `pnpm check`, vitest. Commit `feat(app): inbox note and screens in the new zone`.

### Task 5: Simulator acceptance

- [ ] `git merge master`, rerun the touched tests.
- [ ] `e2e/pod22.js` and `e2e/pod22.sh`, reusing `e2e/drive-lib.js`: seed the sample data in Berlin, lock one action, screenshot Today and Calendar; relaunch in New York, screenshot Today and Calendar (flexible actions at the same local time, the locked one at its shifted local time) and the Inbox note; relaunch in Berlin, screenshot the second note. Screenshots to `docs/screenshots/pod22/`. `git checkout -- apps/aroh/src-tauri/gen/apple`. Commit `test(e2e): pod 22 travel across time zones`.

## Simulator method

Spike 2026-11-22 on `aroh-p22` (iPhone 15 Pro), host Mac zone America/New_York, debug app built with `AROH_E2E_DEVICE=aroh-p22 AROH_DEV_PORT=1429 USER=chaoticqubit sh e2e/pod18.sh build`.

- Launch with `SIMCTL_CHILD_TZ=<zone> xcrun simctl launch --terminate-running-process <udid> io.aroh.app`. Rust `zone()` follows it: with `Europe/Berlin` the sample's 07:00 Core session materialized at 06:00Z, with `America/New_York` at 12:00Z. A relaunch with another zone is seen at once, no reinstall, no data wipe.
- The web view does not: `Intl.DateTimeFormat().resolvedOptions().timeZone` reported the host zone (America/New_York) under both launches, because WebKit's content process does not inherit the app's environment.
- So no `AROH_DEV_ZONE` is needed. The UI must take its zone from the snapshot (Task 4) for the screens to show the zone the store was shifted to; this is also what a real device needs between a zone change and the web view noticing it.
- A travel run is three launches on one store: wipe the app data before the first launch only.

## Open Items

- The clash check covers rows materialized in the 14-day window; occurrences materialized later in the new zone are planned normally.
- The server materializes series with its own `now` (`aroh-server/src/rows.rs` `materialize`). An occurrence the server stored in the old zone that the device had not pulled before it shifted stays at the old instant, and the device's next tick can add the new-zone occurrence beside it. Rare (the server's window must run ahead of the device's last tick); fix by re-keying on the server when `PUT /v1/timezone` arrives, or by a follow pass after the pull.
- `write.rs` `overlaps` and the inline gap walkers in `aroh-reminders`, `aroh-replan` and `aroh-intake` can move to `aroh_domain::travel::wall_instant` and one overlap helper after pod 20 merges.
