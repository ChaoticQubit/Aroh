# Pod 23: Location Awareness, Places, Nudges and Learned Routines

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On iPhone, Aroh learns the places the owner visits, asks once what a place is ("Is this your gym?") and links it to a goal or action, nudges near a linked place when an action is due today or was missed, proposes a repeating action from repeated visits plus repeated actions, keeps routines without a goal in a built-in Routines goal, and gives the owner pause, clear, ignore and a location log (Q56).

**Architecture:** Swift in `tauri-plugin-aroh-ios` only talks to Core Location, MapKit and UserNotifications and buffers raw events; every rule is a pure Rust function in a new crate `aroh-location`, tested in Kiri. Raw visits, unnamed place candidates and unapproved patterns live in local-only SQLite tables that are not in `SYNCED`, so the sync engine cannot see them. Named places and their links are two new synced tables (`places`, `place_links`) with RLS. An approved pattern becomes an ordinary repeating action (`action_series`) in the Routines goal or the linked goal, plus a `place_links` row; that is how a learned pattern syncs and feeds planning. The server engine adds `places` (names and links, no coordinates) to the plan input next to `profile`. Nudges: Rust computes a 7-day nudge plan per watched place and hands it to Swift; Swift monitors up to 20 circular conditions (150 m) and, on entry, schedules the nudge 120 s later and cancels it on exit, so "still there after 2 minutes" works with the app suspended. The approved mockup components (artifact v6) are ported, not rewritten.

**Tech Stack:** Rust 2021, rusqlite, chrono, chrono-tz, sqlx/Postgres, axum; Swift (CoreLocation `CLMonitor`, `CLLocationManager` visits, MapKit `MKLocalSearch` and `MKMapSnapshotter`, UserNotifications); Svelte 5 runes, TypeScript, vitest; Kiri; Xcode simulator tools (`xcrun simctl location`).

## Global Constraints

- Settled decisions: `docs/agent-decisions.md` Q39, Q41, Q43, Q53, Q56 (with the 2026-11-22 approved defaults).
- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. TOML, SQL, YAML, shell, plist carry no comments. No em dashes.
- `assert!` for internal invariants; input validation returns errors. One copy of every helper.
- Mockup code is ported (Q39): `components/location/*`, `PlacesScreen`, `RoutinesScreen`, `LocationSettingsScreen`, `LocationLogScreen`, the optional props on `GoalScreen`, `InboxScreen`, `SettingsScreen`. Visual rule Q41. No new hue.
- Q53: no location code moves an action. Nudge actions change status (done, skipped) or post a later local notification; times never change.
- No provider keys in the app. No third-party map or key: MapKit only. `troupekit/` stays free of "aroh" and is not edited.
- Tests first for every task. Only small tests for critical logic (CLAUDE.md).
- Tests in Kiri, always with `export HOME=/Users/chaoticqubit` first: workspace `kiri build -f Dockerfile.test -t aroh-test-p23 .` then `kiri run --rm aroh-test-p23`; RLS `sh supabase/tests/run.sh`; server `sh crates/aroh-server/tests/run.sh` (with a `p23` project or name if the script allows it); frontend `pnpm check` and `pnpm vitest run` in `node:24-slim`. Remove only your own images and volumes after each run. Never touch `pod-a-*` or other pods' Kiri resources.
- Simulator device `aroh-p23` (iPhone 15 Pro), dev port 1430 read from `AROH_DEV_PORT` at run time and never committed as a default. Screenshots to `docs/screenshots/pod23/`. Intended edits to `gen/apple` (`Info.plist`, `project.yml`) are committed first; after the run `git checkout -- apps/aroh/src-tauri/gen/apple` drops the build churn.
- Pods 20 (edit sheet, locked actions) and 22 (time zones) run in parallel. Shared files take additive edits only: root `Cargo.toml`, `Dockerfile.test`, `aroh-domain` `rows.rs`, `schema.rs`, `seed.rs`, `aroh-store` `db.rs` (append migrations, never edit old ones), `commands/mod.rs`, `lib.rs` handler list, `Info.plist`, `project.yml`, the server engine input, `supabase/tests/run.sh`.
- Branch `feat/location-awareness`, conventional commits, one commit per task, commit and never push, `git merge master` before DONE (never rebase), no `Co-Authored-By` trailer.

## iOS Research

| Topic | Finding | Source |
|---|---|---|
| Watched places | `CLMonitor` (Swift actor, iOS 17+) with `CLMonitor.CircularGeographicCondition`; events arrive through `for try await event in monitor.events` and relaunch the app in the background. Core Location lets one app monitor at most 20 conditions of any type at once, so Aroh picks 20 by relevance. | https://developer.apple.com/documentation/corelocation/clmonitor-2r51v, https://developer.apple.com/documentation/corelocation/clmonitor-2r51v/circulargeographiccondition, https://developer.apple.com/documentation/corelocation/monitoring-the-user-s-proximity-to-geographic-regions |
| Visits | `CLLocationManager.startMonitoringVisits()` delivers `CLVisit` (coordinate, horizontal accuracy, arrival and departure dates) with low power use; needs Always authorization and also relaunches the app. Visits do not fire in the simulator, so a debug-only seeder writes visits for tests and screenshots. | https://developer.apple.com/documentation/corelocation/clvisit, https://developer.apple.com/documentation/corelocation/cllocationmanager/startmonitoringvisits() |
| Always flow | Ask When In Use first, then `requestAlwaysAuthorization()`; iOS shows the upgrade prompt (possibly later, provisional Always). The approved explainer says iOS asks twice. Status changes arrive in `locationManagerDidChangeAuthorization`. On iOS 18 `CLServiceSession` can declare the requirement; not needed with iOS 17 APIs. | https://developer.apple.com/documentation/corelocation/requesting-authorization-to-use-location-services, https://developer.apple.com/documentation/corelocation/cllocationmanager/requestalwaysauthorization(), https://developer.apple.com/documentation/corelocation/clservicesession-pt7n |
| Info.plist | `NSLocationWhenInUseUsageDescription` and `NSLocationAlwaysAndWhenInUseUsageDescription` are required, with plain strings matching the explainer. | https://developer.apple.com/documentation/bundleresources/information-property-list/nslocationwheninuseusagedescription, https://developer.apple.com/documentation/bundleresources/information-property-list/nslocationalwaysandwheninuseusagedescription |
| Background modes | The `location` background mode is for continuous location updates. Monitoring and visits relaunch the app without it, so Aroh does not add it (no blue indicator, no continuous GPS). Verify on the simulator that condition events arrive with the app terminated; add the mode only if they do not, and record why. | https://developer.apple.com/documentation/bundleresources/information-property-list/uibackgroundmodes, https://developer.apple.com/documentation/corelocation/handling-location-updates-in-the-background |
| Battery | No `startUpdatingLocation`. Only visits and condition monitoring, both system-managed low-power services. MapKit search and snapshots run only on screen. | https://developer.apple.com/documentation/corelocation/configuring-your-app-to-use-location-services |
| Deployment target | The app targets iOS 16.2; `CLMonitor` needs iOS 17. Location features run under `if #available(iOS 17, *)`; on 16.x `location_status` answers `unsupported` and the UI hides the Location row. No target bump. | `apps/aroh/src-tauri/gen/apple/project.yml` |
| Map | `MKLocalSearch` for search and reverse place lookup (POI name and category for the prompt's guess), `MKMapSnapshotter` for a static image of places returned as a PNG data URL. No key, no third party. Off iOS the neutral SVG `PlaceMap` stays. | https://developer.apple.com/documentation/mapkit/mklocalsearch, https://developer.apple.com/documentation/mapkit/mkmapsnapshotter |
| Nudge actions | A `UNNotificationCategory` `aroh.nudge` with three `UNNotificationAction`s: Mark done, Remind me in 1 hour, Skip today. Responses arrive in the existing `ReminderDelegate` `didReceive`. | https://developer.apple.com/documentation/usernotifications/unnotificationcategory, https://developer.apple.com/documentation/usernotifications/unnotificationaction |
| Simulator | `xcrun simctl location aroh-p23 set <lat>,<lon>` moves the device; `simctl location clear` resets it. Condition entry fires after the move. | `xcrun simctl help location` |

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Local-only tables | New SQLite migration: `visits` (id, lat, lon, accuracy_m, arrived_at, left_at, place_id null), `place_candidates` (id, lat, lon, poi_name, poi_kind, address, state `pending`/`ignored`, visit_count, prompted_at), `pattern_candidates` (id, place_id, title, gap_days, weekday null, local_time, evidence, state `pending`/`declined`/`approved`, declined_until), `nudge_sent` (place_id, local_date). None is a `TableDef` and none is in `SYNCED`. | Q56: raw visits stay on the phone; patterns stay on the phone until approved (owner 2026-11-22). |
| Synced tables | `places` (id, user_id, name, address, lat, lon, radius_m default 150, source `prompt`/`search`, updated_at, deleted) and `place_links` (id, user_id, place_id, goal_id, series_id null, updated_at, deleted). `TableDef`s in `rows.rs`, appended to `SYNCED` after `actions`. Supabase migration with `server_seq`, RLS by `user_id` like `preferences`. | Named places and approved routines sync so other devices and agents can use them. |
| Ignored places | A `place_candidates` row with state `ignored`; never synced, never prompted again, listed in Settings > Location with Stop ignoring. | An ignored place is a private "no", not a place. |
| Retention | Raw visits older than 90 days are deleted on start, on every drain and after clear. Clear history deletes all `visits` and pending `place_candidates`; places, links and routines stay. | Owner 2026-11-22. |
| Place prompt | A cluster of visits within 150 m on at least 3 distinct local days in the last 21 days, not near a named or ignored place, becomes one `pending` candidate; Swift's reverse lookup fills POI name and kind. The guess word comes from the POI kind (fitness center: "gym", grocery: "grocery store", otherwise "Is this {name}?"). Shown in the Inbox only, at most one new prompt per day. | Owner answer 1; Q56 "after repeated visits". |
| Near | 150 m circular condition, nudge fires 120 s after entry unless an exit came first. | Q56. |
| Nudge rule | `aroh_location::nudge_plan(places, links, actions, series, now, zone, sent)`: for each watched place and each of the next 7 local days, a nudge exists when a linked action (by `series_id`, else any action of the linked goal) is planned that local day and not done, or the newest linked action in the last 7 days is missed and not done. Window 06:00 to 22:00 local; none if the place already has a `nudge_sent` row for that day; none while paused. Text: title "{action title}, due {HH:MM}" or "{action title}, missed {day}", body "Near your {place}. Quick workout to stay on track?" for a fitness goal, otherwise "Near {place}. Good time for {action title}?" | Q56 rules; owner approved the nudge text. |
| Nudge delivery | Swift keeps the plan; on entry it schedules the matching day's nudge with a 120 s trigger if the fire time is inside the window and the place was not nudged that day (Swift records the day and reports it on the next drain into `nudge_sent`). Exit before firing removes it. | Works while suspended; rules stay in Rust. |
| Nudge actions | Mark done: `app_set_status(done)`; Skip today: status skipped; Remind me in 1 hour: one local notification 60 min later with the same text. No time change (Q53). | Owner answer 2. |
| Watched set | `choose_watched(places, links, actions, now)`: places with a linked action in the next 7 days first (soonest), then by link count and recent visits; at most 20; ignored never. Recomputed on start, resume and every write that touches places or actions; Swift diffs its conditions. | iOS limit of 20, "chosen by relevance". |
| Pattern detection | `detect_patterns(visits, actions, places, now, zone)`: per named place and normalized action title, actions done while a visit at that place was open (done_at within the visit plus or minus 30 min). At least 4 occurrences in 90 days, gaps within 25 percent (at least plus or minus 1 day) of their median, median 1 to 35 days. Weekday set when at least 75 percent share one; local time is the median rounded to 15 min. Skipped when a series with that title already exists or the candidate is declined and `declined_until` is in the future (60 days). | Q56: same title, same place, regular gap; "not again for a while". |
| Pattern proposal | Shown in the Inbox through the existing `ProposalCard` (approved mockup: persona "Planner", one `create` change). Apply creates an `action_series` (weekly RRULE when a weekday exists, else every N days) in the place's linked goal, else the Routines goal, plus a `place_links` row with `series_id`; Reject sets `declined`. Nothing is written to synced tables before Apply. | Owner answer 5. |
| Routines goal | Built-in goal seeded with `derived_id(user, "goal:routines", 0)`, title "Routines", category Habit, autonomy Ask, `SEED_STAMP`. The goal screen shows `RoutinesScreen` for this id; it cannot be deleted. | Q56; owner answer 4. |
| Streak unit | `streak(series, actions, now)` counts consecutive done occurrences back from the newest past one and labels the unit from the cadence: daily "12 days", weekly "4 weeks", every N days "3 times". | Owner answer 7. |
| Server | `Engine` reads `places` and `place_links` (names, linked goals and series titles, never coordinates) into plan input key `places` beside `profile`, for both `new-goal` and `replan`. No new trigger: approved routines arrive as `action_series`, which planning already reads. | Owner answer 5 moved detection and proposals onto the device. |
| Explainer copy | "Only the places you name and the routines you approve sync." Shown when the owner turns location on in Settings > Location, before the first iOS prompt. | Owner answer 5. |
| Rejected | Raw visits or candidates in Supabase; server-side pattern detection; `location` background mode; a web map library; a deployment target bump; a separate `routines` table. | Privacy, owner answers, battery, and an approved routine is already an `action_series`. |

## File Structure

| Path | Change |
|---|---|
| `crates/aroh-location/` | new pure crate: `geo.rs` (distance, clustering), `prompt.rs`, `nudge.rs`, `watch.rs`, `pattern.rs`, `streak.rs`, tests |
| `crates/aroh-domain/src/{rows.rs,schema.rs,seed.rs}` | `Place`, `PlaceLink`, `PLACES`, `PLACE_LINKS`, `SYNCED` append, Routines goal seed |
| `crates/aroh-store/src/db.rs`, `crates/aroh-store/tests/sync.rs` | migration for synced and local tables; sync exclusion test |
| `crates/aroh-app/src/location.rs`, `crates/aroh-app/tests/location.rs` | drain, prune, detect, decide prompt and pattern, approve, nudge plan and watched set |
| `crates/tauri-plugin-aroh-ios/ios/Sources/LocationBridge.swift`, `ArohPlugin.swift`, `ReminderDelegate.swift`, `src/lib.rs` | Core Location, MapKit, nudge category; additive |
| `apps/aroh/src-tauri/src/commands/location.rs`, `commands/mod.rs`, `lib.rs` | `app_location_*` commands, debug `app_dev_location_seed` |
| `apps/aroh/src-tauri/gen/apple/{aroh_iOS/Info.plist,project.yml}` | two usage strings |
| `supabase/migrations/20261126010000_places.sql`, `supabase/tests/rls_places.sql`, `supabase/tests/run.sh` | tables, RLS, test; asserts no `visits` table exists |
| `crates/aroh-server/src/{rows.rs,engine.rs}`, `crates/aroh-server/tests/` | plan input `places`, test |
| `apps/aroh/src/lib/app/{client.ts,views.ts,views.test.ts}` | location client and view mapping |
| `apps/aroh/src/routes/{inbox,places,settings/location,settings/location/log,goals/[id]}` | wire ported screens |
| `e2e/pod23.sh`, `e2e/pod23.js` | simulator flow (copied from pod 18's recipe) |

## Tasks

### Task 1: Data model and the sync boundary

- [x] Failing tests: `aroh-store/tests/sync.rs` `raw_visits_never_reach_sync`: insert visits, candidates and pattern candidates, run `pending` for every `SYNCED` table and assert no row or value from them appears, and no `SYNCED` name is a local table; `visits_older_than_90_days_are_deleted`; `places` and `place_links` round-trip and LWW like other tables; Routines goal seed is stable per user and valid.
- [x] Implement rows, schema, seed, SQLite migration. Kiri green. Commit `feat(aroh-store): places sync and local-only visit tables`.

### Task 2: Pure rules crate

- [x] Failing tests in `aroh-location`: prompt after 3 distinct days within 150 m and not before; ignored and named places never prompt; passing by (no visit) ignored; nudge due today, missed yesterday, done means none, quiet hours none, second nudge same day none, paused none; watched set caps at 20 and orders by soonest linked action; pattern found for 5 Saturday grocery visits with done actions, none for irregular gaps, none for 3 occurrences, none while declined, none when a series exists; streak units "4 weeks" and "12 days".
- [x] Implement. Kiri green. Commit `feat(aroh-location): place, nudge, watch and pattern rules`.

### Task 3: App layer

- [x] Failing tests in `aroh-app/tests/location.rs`: drain stores visits, prunes, creates one candidate; deciding a prompt with a link writes one place and one link; ignore writes no synced row; approving a pattern writes one series and one link, declining writes nothing synced; clear history keeps places; nudge plan and watched set come from stored rows.
- [x] Implement. Commit `feat(aroh-app): location drain, prompts, patterns and nudge plan`.

### Task 4: iOS bridge and commands

- [x] Swift `LocationBridge`: authorization in two steps, visits, `CLMonitor` conditions synced to the watched set, event buffer file in Application Support drained by `location_drain`, nudge plan storage and 120 s scheduling, `aroh.nudge` category, `MKLocalSearch` search and reverse lookup, `MKMapSnapshotter`. Rust `commands/location.rs`; responses to nudge actions call the existing status commands. Info.plist strings.
- [x] `cargo check -p aroh` and an iOS simulator build pass. Commit `feat(ios): location bridge, watched places and nudges`.

### Task 5: Server and Supabase

- [x] Failing tests: `supabase/tests/rls_places.sql` (owner reads and writes own rows, another user sees none, no `visits` table); server test that plan input carries `places` with names and links and no coordinates.
- [x] Implement migration, rows and engine input. Commit `feat(aroh-server): places in plan input with RLS`.

### Task 6: App screens

- [x] Failing vitest: view mapping for places, prompts, patterns, routines with streak labels, log grouping.
- [x] Wire ported screens: Inbox places group and pattern cards, Places with search and snapshot, Settings Location row and screen with explainer, location log, Routines goal dashboard. Explainer copy per Decisions. `pnpm check` 0 errors. Commit `feat(app): location screens`.

### Task 7: Simulator acceptance

- [x] `e2e/pod23.sh` on `aroh-p23`, port from `AROH_DEV_PORT`. Screenshots: 01 explainer, 02 Settings Location, 03 Inbox place prompt (seeded visits), 04 Places with snapshot and search, 05 nudge banner after `simctl location set` to a linked place with a due action and a 2 min wait, 06 nudge long press actions, 07 Inbox learned routine, 08 Routines goal, 09 location log, 10 cleared history. Restore `gen/apple` afterwards.
- [x] `git merge master`, rerun the location tests. Commit `test(e2e): pod 23 location flow`.

## Open Items

- Visit detection itself cannot run in the simulator; real-device check of `CLVisit` and background relaunch stays for the owner's phone.
- No `location` background mode: in the simulator, CLMonitor fired the place condition and the nudge showed 120 s later without it (screenshot 05).
- Screenshot 06 (nudge long-press actions) is not taken: the simulator has no long-press tool. The three actions are registered in the `aroh.nudge` category; check them on a device.
- Places added by search can only be linked from a prompt or the debug driver; a link editor on the Places screen needs a mockup first.
- Settings > Location shows Always when location is on; the real iOS access level (While using) is not read back yet.
