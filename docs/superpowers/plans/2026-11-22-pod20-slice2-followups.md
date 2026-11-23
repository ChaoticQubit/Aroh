# Pod 20: Slice 2 Follow-ups

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal.** Q52: the five slice 2 follow-ups. (1) All-day events are asked only through the day-kind choice, never in the keep/ignore busy-time cards; events from the device's holiday and birthday calendars default to Reminder with no question (Q51). (2) Busy days apply to every action, repeating occurrences and newly planned ones included, from one shared place. (3) Actions with no free day within 14 days are shown to the owner. (4) The triage "Suggested" kind stays until the owner confirms a kind. (5) The goal screen shows one coaches card with the same data as "Coaches on this goal".

**Branch.** `fix/slice2-followups` off master `ef1d40c`.

## Global Constraints

- Settled decisions: `docs/agent-decisions.md` Q19, Q39, Q41, Q42, Q47, Q48, Q51, Q52. Pod 18 and pod 19 plans for the code they built.
- No mockup (Q39): existing components only (InsightCard, AllDayChip, AllDayChoice, Persona, Card, Button). Q41: no new hue.
- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. TOML, SQL, YAML, shell carry no comments. No em dashes.
- `assert!` for internal invariants; input validation returns errors. One copy of every helper (Q42).
- No provider keys in the app. `troupekit/` stays free of "aroh" and is not edited.
- Tests first for every task, run red before code. Only small tests for critical logic.
- Tests in Kiri, always with `export HOME=/Users/chaoticqubit` first (or `DOCKER_HOST=unix:///Users/chaoticqubit/.kiri/run/docker.sock`): workspace `kiri build -f Dockerfile.test -t aroh-test-p20 .` then `kiri run --rm aroh-test-p20`; server `sh crates/aroh-server/tests/run.sh` with `p20` names where the script allows; frontend `pnpm check` and `pnpm vitest run` in `node:24-slim`. Remove own images and volumes after each run. If the disk fills, `kiri builder prune -f` and remove only dangling 64-hex volumes. Never touch `pod-a-*` or other pods' resources.
- Simulator device `aroh-p20` (iPhone 15 Pro), dev port 1428, never committed. Screenshots to `docs/screenshots/pod20/`. `git checkout -- apps/aroh/src-tauri/gen/apple` before every commit. Reuse `e2e/drive-lib.js` helpers; no duplicate helpers.
- Conventional commits, one per task, commit and never push, no `Co-Authored-By` trailer. Stage by path; `graphify-out` is an untracked symlink and never staged. `git merge master` before DONE, never rebase.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| All-day out of the groups | The server's `sort_groups` tool groups only timed event keys; all-day keys never enter a group or `Other`. The persona instructions say all-day events get a day pick, not a group. | `sort_groups` puts every ungrouped key in `Other`, so a prompt change or a TS filter alone does not hold. |
| Already-triaged marker | `refs` carry `all_day`. `write_rules` writes, for every all-day ref, a day rule (group "All day", decision Keep) that keeps an existing rule's `day_kind` and leaves it null otherwise. Import then finds a rule, the chip shows "Choose", and the event is no longer unmatched. | Without a rule every connect would send the same all-day events to triage again. |
| Run with no groups | When the waiting `decide` node has zero groups (all-day only), the app answers it with `[]` right after saving the suggestions; the run ends and writes the all-day rules. | Otherwise the run waits forever with no card, and `connect` starts no new triage while one is open. |
| Suggestion storage | New local setting `day_suggestions`: JSON object, device event id to `reminder`, `busy` or `away`, at most 300 entries, ids 1 to 255 characters. One function in `calendar.ts` reads the waiting `decide` node, merges its picks into the stored ones, prunes ids that no longer have an unconfirmed all-day block, and saves; Today, Calendar and Inbox all use it (the two `onMount(openTriage)` copies become one). `allDayViews` reads the stored map. A confirmed kind wins, so the mark ends at confirmation. | `openTriage` only finds `waiting_human` runs, so the mark vanished when the run ended. Settings are local and need no migration; the suggestion is a hint, not synced data. Rejected: a `suggested_kind` column on two tables plus Supabase (three migrations for a hint); reading finished runs (needs the server, fails offline). |
| Holiday default | `DeviceEvent` gains `holiday: bool` (`#[serde(default)]`), set by the EventKit plugin when `EKCalendar.type` is `.subscription` or `.birthday`. Import: an all-day holiday event with no rule becomes a block with `day_kind` Reminder and is not unmatched. A rule (set by a chip tap) wins. Timed holiday events are unchanged. | Q51. iOS serves its holiday calendar as a subscribed calendar and birthdays as the birthday calendar; Q47 names birthdays as Reminder. Risk: an owner-subscribed ICS feed also defaults to Reminder; one chip tap changes it. |
| Busy-day sweep | One function in `aroh-app::write`: `keep_off_busy_days(conn, clock, user_id, today, zone)` moves every planned, unlocked, live action whose local day is a Busy day and is `today` or later to the same local time on the next day within 14 that is no Busy day and has no overlap (the current `move_off` rule). Called by `set_day_kind` (replacing `move_off`), by `tick` after materialize and before marking missed, and by the calendar import command after `import`. `tick` gains a `zone` argument; all callers pass `crate::zone()`. | Root fix: repeating occurrences come from `tick`'s materialize, server plans arrive through sync, which ticks, and a block arriving with a Busy rule never passes `set_day_kind`. |
| Series on a Busy day | The sweep counts each series' coming occurrences (from the `recur` helper, up to 14 days past the latest Busy-day action) as taken. A daily occurrence then finds no free day, stays and shows in the stuck insight; a weekly one moves to the next free day. | Moving a daily occurrence collides with the next day's own occurrence or double-books later, and the owner decides through the insight. |
| Locked actions | The sweep never moves a locked action. | Q19: hand-edited or locked actions are never moved without asking; a sweep on every tick would undo the owner's drag. |
| Stuck actions | Derived, not stored: `busyDayInsights(s, zone, now)` in `views.ts`, appended after `insightItems` by `inboxView`, lists each Busy-day block with planned actions, locked ones included (Q53), still on its local days from today on as an Inbox insight (existing InsightCard, persona "Calendar"), id = the block id, title "No free day for <n> actions", body naming the day, the event and up to three titles and saying to move them by hand or change the day's kind. Dismiss uses the existing `dismissed_insights` setting. Neutral card, no status hue. `DaySet.stuck` stays as the command's return. | After the sweep, anything left on a Busy day found no slot or is fixed, so state is the truth; no duplicates per tick, survives restart. Locked actions were excluded before Q53. |
| Coaches card | `GoalHeader.personas` and the "Your personas" card go. The goal screen card is titled "Coaches on this goal" and lists `coachesOn(...)` by `display_name` with `Persona`, keeps "Chat with your personas" and links "All coaches" to `/personas?goal=<id>`. One loader in `personas.ts` (`loadCoaches(snapshot, goalId)`: engine personas, goal runs, `coachesOn`) is used by the goal route and the personas route. Empty or offline: "No coach has worked on this goal yet." | Q42: one copy of the logic; persona rows with `goal_id` are written only by plugin bundles. |
| Fixed actions on the server (Q53) | A move, resize or delete aimed at a locked action never reaches a proposal. The adaptation analyst gets a `keeps_fixed(changes)` eval over the run's calendar (locked ids read from the run checkpoint like `busy`), so the persona's repair round (Q33) asks it again; `record_proposal` refuses such a change with `InvalidArgs` as the backstop. `touches_locked` goes from the record, the workflow state and the autonomy gate (no other reader). The engine's `busy` input lists live planned locked actions as fixed blocks, so `no_overlap` keeps new or moved actions off them. | A tool-node error fails the run in troupekit-graph, it has no repair round; only a persona eval gets one. `crates/aroh-replan` is not changed: no app or server crate depends on it (legacy `aroh-core` model). |
| Fixed clashes (Q53) | A planned locked action overlapping a timed busy block becomes an Inbox note in `busyDayInsights` (id = the action id, persona "Calendar"); a locked action on a Busy day is already in the stuck note. | Reuses the one derived-notes path; nothing stored. |
| Fixed time switch (Q53) | `write::set_locked(conn, clock, id, locked)` plus command `app_set_locked`, following `app_edit_action`. The edit sheet (ActionEditor) gets a `switch` "Fixed time" bound to `locked`; a hand edit still locks. On the first-plan approval each one-off item gets the same switch; after the approved run ends and syncs, the app finds each fixed item's action (goal, no series, title, local start) and calls `app_set_locked`. Repeating items get no switch. | One write path both ways; the `approve` answer stays a bool, so the workflow and server are unchanged. A fixed repeating item needs `locked` on `action_series` (a schema change), left open. |
| Rejected | Filtering triage cards in TS; moving actions on the server; storing stuck titles; a new status value for actions. | Each fixes one path and leaves siblings broken, or adds schema for derived state. |

## File Structure

| Path | Change |
|---|---|
| `crates/aroh-domain/src/calendar.rs` | `DeviceEvent.holiday`, `busy_block` gives Reminder to a ruleless all-day holiday event |
| `crates/aroh-app/src/calendar.rs` | holiday events not unmatched |
| `crates/aroh-app/src/write.rs` | `keep_off_busy_days`, `set_day_kind` uses it, no locked moves |
| `crates/aroh-app/src/tick.rs`, `local.rs` | `zone` argument and sweep; `day_suggestions` setting |
| `apps/aroh/src-tauri/src/commands/*.rs`, `lib.rs` | pass `crate::zone()`; sweep after import |
| `crates/tauri-plugin-aroh-calendar/ios/Sources/CalendarPlugin.swift` | `holiday` from the calendar type |
| `crates/aroh-server/src/triage.rs`, `workflows/calendar-triage.toml`, `personas/calendar-triage.toml` | timed-only groups, `refs.all_day`, all-day rules |
| `apps/aroh/src/lib/app/calendar.ts`, `views.ts`, `personas.ts`, `client.ts` | suggestions, stuck insights, coaches loader, types |
| `apps/aroh/src/routes/today`, `calendar`, `inbox`, `goals/[id]`, `personas` | one suggestion reader, coaches |
| `apps/aroh/src/lib/ui/screens/GoalScreen.svelte`, `types.ts`, `mockdata/*` | coaches card |
| `e2e/pod20.js` | simulator flow |

## Tasks

### Task 1: Busy days everywhere (items 2 and 3, Rust)

- [ ] Tests first in `crates/aroh-app/tests/calendar.rs` and `app.rs`: a daily series with a Busy day 5 days ahead: `tick` moves that occurrence to the next free day and a second `tick` changes nothing (no re-materialized copy on the Busy day); a planned action synced in on a Busy day moves on `tick`; a locked action stays; a done action stays; an action on a Busy day with 14 following Busy days stays and is reported by `set_day_kind`; import of an all-day event matching a Busy rule then the import command's sweep moves that day's actions. Existing pod 18 `set_day_kind` tests stay green.
- [ ] Implement. Commit `fix(app): busy days apply to every action`.

### Task 2: Holiday default and all-day out of the groups (item 1, Rust)

- [ ] Tests first: domain `busy_block` gives Reminder to a ruleless all-day holiday event and keeps a rule's kind; import puts a holiday all-day event in no `unmatched`; a missing `holiday` field deserializes as false. Server: `sort_groups` over timed and all-day keys puts no all-day key in any group; `triage_input` refs carry `all_day`; `write_rules` writes an "All day" Keep rule per all-day ref keeping an old `day_kind`, and works with zero groups.
- [ ] Implement, Swift mapping included; update the triage persona text. `sh crates/aroh-server/tests/run.sh` green. Commit `fix(calendar): all-day events skip the busy-time cards, holidays default to Reminder`.

### Task 3: App views (items 1, 3, 4, 5, TS)

- [ ] vitest first: the suggestion reader merges and prunes, answers `[]` for a zero-group `decide`, and `allDayViews` keeps a stored suggestion after the run ended and drops it once a kind is set; `day_suggestions` validation in Rust (`local.rs` test); stuck insights from a snapshot (one per Busy block, locked and done excluded, past days excluded, dismissed hidden); `loadCoaches` with fake engine calls (rows plus run slugs, offline empty).
- [ ] Implement; GoalScreen card; update mockdata so `/mockups` still builds. `pnpm check`, vitest. Commit `fix(app): keep day suggestions, show stuck actions, one coaches card`.

### Task 5: Fixed actions are never moved (Q53 item 6)

- [ ] Tests first: server `keeps_fixed` eval unit test; `autonomy_gate` without `touches_locked`; engine `day_inputs` lists locked planned actions as busy; `record_proposal` refuses a change on a locked action; api test `locked_actions_are_fixed_for_every_proposal` (replaces `locked_actions_force_approval_and_are_never_moved`): the analyst's move of a locked action is repaired into a move of a flexible one. vitest: a locked action overlapping a timed busy block gives a clash note.
- [ ] Implement. `sh crates/aroh-server/tests/run.sh` green. Commit `fix(server): locked actions are fixed for every agent`.

### Task 6: Fixed time switch (Q53 item 7)

- [ ] Tests first: Rust `set_locked` both ways and on a gone row; vitest for the approval switch's fixed items to action ids.
- [ ] Implement the write, the command, the ActionEditor switch and the plan approval switch. Commit `feat(app): fixed time switch`.

### Task 4: Simulator acceptance

- [ ] `git merge master`.
- [ ] `e2e/pod20.js` on `drive-lib.js`: seed device all-day events, connect calendars, see no all-day event in the Inbox busy-time cards and the "Suggested" mark on the chip after the cards are answered; confirm a Busy day with a repeating action and see later occurrences move after a resume tick; a Busy day with no free day shows the Inbox insight; a Fixed time action stays put on a Busy day and the edit sheet shows the switch; the goal screen shows "Coaches on this goal" with names. Holiday default is unit-tested only if the simulator has no holiday calendar.
- [ ] `aroh-p20`, port 1428, screenshots to `docs/screenshots/pod20/`, stack down and own volumes removed, restore `gen/apple`. Commit `test(e2e): pod 20 slice 2 follow-ups`.

## Open Items

- `graphify update .` after merge (the graph lives in the main checkout).
- Task 4 is NOT VERIFIED on the simulator. One `e2e/pod20.js` run passed steps 1 to 6: Fixed time switch on the one-off plan item only, Dentist locked after approval, triage card holds only the timed standup, kept Busy day suggestion, workout moved while the fixed Dentist and the daily plank stay, Inbox "No free day for 2 actions". Its screenshots were deleted before a rerun and are not committed. Later runs never reached the app: the e2e engine link outlasted the `up.sh` 20 minute wait, Kiri failed to start containers once, and two runs launched the app with no drive script marks. Time-boxed by the PM.
- Not run on the simulator: step 7 fixed clash Inbox card, steps 8 and 9 the edit sheet Fixed time switch off and on, step 10 the goal coaches card, the busy-day caption overlap check, and the `pod20.sh` server checks (Dentist locked, Conference busy). Holiday default is unit-tested only.
- `e2e/up.sh` waits at most 20 minutes for the engine; a cold engine build on a loaded machine needs longer.
- Fixed time for repeating plan items needs a series `locked` column.
