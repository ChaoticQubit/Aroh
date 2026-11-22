# Pod 18: All-Day Events and the Calendar Edit Sheet

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Device all-day events show as a chip row on Today and Calendar (day, week, month, week strip). Each event has a kind, proposed by the triage persona and confirmed once by the owner on the choice card: Reminder keeps the day as planned, Busy day blocks the day and moves its actions to other days, Away has the replanner adapt each goal by category and send it to the Inbox as a proposal. Tapping a chip reopens the choice. Tapping an action on Today or Calendar opens the edit sheet (move, resize, delete); a long press drags a block on the phone. Every change is written through the app engine (HLC, LWW sync), so the server's edit learner sees it.

**Architecture:** The approved mockup components (pod 18 Phase A, artifact v5) are ported, not rewritten: `AllDayChip`, `AllDayChoice`, `ActionEditor`, `ActionSheet`, `CalendarGrid`, `CalendarScreen`, `TodayScreen`, `edits.ts`. The two-option `mode` becomes a three-option `kind`. Data: a new text enum `DayKind` (`reminder`, `busy`, `away`) in `aroh-domain`, a nullable `day_kind` on `calendar_rules` and `busy_blocks`. `aroh-app::calendar::import` stops skipping all-day events: a rule with a `day_kind` decides it, an all-day event with no rule is stored as a busy block with `day_kind` null (chip shows "Choose") and also goes to triage. The triage persona proposes a kind per all-day event in its output; the app shows it preselected on the choice card. Confirming writes a rule (series rule for a recurring event, title rule otherwise) and sets the kind on the event's blocks in one write. Busy day: the same write moves every planned action of that local day to the same time on the next day that is not a busy all-day day and has no overlap, in the app, offline, no model. Away: the server engine sees away days in the plan input; a new trigger source `calendar` claims an adaptation per goal with actions on the away days and runs the replan workflow with an Away reason, always as an Inbox proposal regardless of autonomy. The edit sheet calls the existing `app_edit_action` command (pod 15), which locks the action; the server already records `move`, `resize`, `delete` edits on sync push.

**Tech Stack:** Rust 2021, rusqlite, sqlx/Postgres, axum, chrono-tz; Svelte 5 runes, TypeScript, bits-ui, vitest; Kiri; Xcode simulator tools.

## Global Constraints

- Settled decisions: `docs/agent-decisions.md` Q19, Q37, Q39, Q41, Q47, Q48.
- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. TOML, SQL, YAML, shell carry no comments. No em dashes.
- `assert!` for internal invariants; input validation returns errors. One copy of every helper.
- Mockup code is ported (Q39). Visual rule Q41: neutral chrome; Busy day and Away chips filled, Reminder outlined, pending dashed. No new hue.
- No provider keys anywhere in the app. `troupekit/` stays free of "aroh" and is not edited.
- Tests first for every task. Only small tests for critical logic (CLAUDE.md).
- Tests in Kiri, always with `export HOME=/Users/chaoticqubit` first: workspace `kiri build -f Dockerfile.test -t aroh-test-p18 .` then `kiri run --rm aroh-test-p18`; server `sh crates/aroh-server/tests/run.sh` (use a `p18` project or name if the script allows it); frontend `pnpm check` and `pnpm vitest run` in `node:24-slim`. Remove only your own images and volumes after each run. Never touch `pod-a-*` or other pods' Kiri resources.
- Simulator device `aroh-p18` (iPhone 15 Pro), dev port 1426, never committed. Screenshots to `docs/screenshots/pod18/`. `git checkout -- apps/aroh/src-tauri/gen/apple` afterwards.
- Branch `feat/calendar-editing`, conventional commits, one commit per task, commit and never push, bring in master with `git merge master` (never rebase), no `Co-Authored-By` trailer.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Kinds | `DayKind { Reminder, Busy, Away }`, text `reminder`, `busy`, `away`, labels "Reminder", "Busy day", "Away". | Owner answer 2026-11-22, Q47. |
| Storage | Nullable `day_kind` on `calendar_rules` and `busy_blocks`; new SQLite migration in `aroh-store/src/db.rs`, new Supabase migration, server row structs follow `aroh-domain`. | Rules make the choice stick for every future occurrence; the block carries it for views and planning without a join. |
| Unruled all-day events | Kept as busy blocks with `day_kind` null and sent to triage with `all_day: true` (within the existing `MAX_TRIAGE` cap). | The chip must show before triage answers; timed events keep today's behaviour. |
| Proposal | The triage persona outputs `kind` for each all-day event it sees; the app reads it from the run like the triage groups and preselects it on the choice card ("Suggested"). Never applied without the owner's tap. | "Proposed by the triage persona, confirmed once by the owner." |
| Reopen | Tapping a chip opens the choice card for that event with its current kind marked. | Owner answer 2. |
| Busy day effect | In `write::set_day_kind`: rule upsert, block kinds, then each planned (not done, not deleted) action on that local day moves to the same local time on the next day (up to 14 days) that is not a Busy day and where it does not overlap another action or busy block. Moves use `repo::put` with the HLC; moved actions stay unlocked. A day with no slot found leaves the action and reports it. | Deterministic, offline, testable, syncs through LWW. Not dropped (owner). |
| Away effect | Engine `busy()` stays timed-only plus Busy days as full-day busy; plan input gets `away_days` (local dates). Trigger source `calendar`: for each goal with planned actions on away days, claim one adaptation per goal and window, reason names the dates and asks to adapt by category (examples in the reason: no-equipment workout, pause and move practice, habits continue); the proposal always waits in the Inbox. | Owner answer 1; reuses the adaptation and proposal flow pods 14 and 15 built. |
| Reminder effect | None on planning. | Owner. |
| Edit write path | `client.editAction(id, start, end)` and `client.deleteAction(id)` over `app_edit_action`; routes pass `onedit`/`ondelete` and call them through `data.run`, then the snapshot reloads. | Command, lock and learning already exist (pod 15). |
| Sheet | `ActionSheet` moves onto the `sheet` primitive (bits-ui Dialog, side bottom, no `shadow-lg`, hairline top border) for focus trap and Escape. A prop renders it inline for `/mockups` frames (portal disabled, no scroll lock, no focus trap). | Leftover from Phase A. |
| Phone drag | A long press (350 ms) on any block starts a drag: after activation, a non-passive `touchmove` listener calls `preventDefault` so the page does not scroll. Selected blocks keep dragging at once. | Leftover from Phase A. |
| Month view and week strip | Month cells and week strip days show a small neutral bar per all-day event (filled for Busy day and Away, outlined for Reminder) with the title in the label. | Leftover from Phase A. |
| Rejected | Dropping actions on a busy day; storing kinds in `group_name`; asking the model to move Busy-day actions. | Owner said move, not drop; overloading a field breaks triage groups; a model is not needed for a same-time shift. |

## File Structure

| Path | Change |
|---|---|
| `crates/aroh-domain/src/rows.rs` | `DayKind`, `day_kind` on `CalendarRule` and `BusyBlock` |
| `crates/aroh-domain/src/calendar.rs` | `busy_block` copies `day_kind`; rule builder for one event with a kind |
| `crates/aroh-store/src/db.rs` | migration adding both columns |
| `supabase/migrations/2026112218xxxx_day_kind.sql`, `supabase/tests/*` | columns, RLS unchanged |
| `crates/aroh-app/src/calendar.rs` | import all-day events |
| `crates/aroh-app/src/write.rs` | `set_day_kind` with the Busy-day move |
| `apps/aroh/src-tauri/src/commands/calendar.rs` (or the module that owns calendar commands) | `app_set_day_kind`, registered in `lib.rs` |
| `crates/aroh-server/src/triage.rs`, triage persona file | all-day input flag and `kind` output |
| `crates/aroh-server/src/engine.rs`, `triggers.rs` | Busy days in `busy()`, `away_days`, trigger source `calendar` |
| `apps/aroh/src/lib/app/client.ts`, `calendar.ts`, `views.ts` | types, `editAction`, `deleteAction`, `setDayKind`, `allDayViews`, triage proposal read |
| `apps/aroh/src/lib/ui/...` | kind instead of mode, chip reopen, month and strip markers, sheet primitive, long-press drag |
| `apps/aroh/src/routes/today`, `calendar`, `mockups` | wire callbacks and data; mockup shows the three kinds |
| `e2e/pod18*` | simulator flow |

## Tasks

### Task 1: Day kind in the data model

- [ ] Tests first: `DayKind` round-trips as text; `CalendarRule` and `BusyBlock` with and without `day_kind` round-trip through the store and the codec; migrating a database at the previous version adds both columns and keeps rows; the Supabase migration applies after the existing ones (server test harness).
- [ ] Implement rows, store migration, Supabase migration, server row mapping. Commit `feat(core): day kind on calendar rules and busy blocks`.

### Task 2: Import all-day events and set a kind

- [ ] Tests first in `crates/aroh-app/tests/calendar.rs`: an all-day event with no rule becomes a block with `all_day` and no kind and is in `unmatched`; with a kind rule it gets that kind; `set_day_kind(Busy)` writes a rule, sets the kind on all blocks of that event, moves each planned action of that day to the next free day at the same time, skips done actions, skips a following Busy day, avoids overlaps; `set_day_kind(Reminder)` and `Away` move nothing; changing Busy to Reminder later does not move actions back. Replace the old `ignored_and_all_day_events_make_no_blocks` expectation (stale test).
- [ ] Implement `import`, `write::set_day_kind`, Tauri command. Commit `feat(app): import all-day events and confirm their kind`.

### Task 3: Triage proposes, engine plans around, Away adapts

- [ ] Tests first (server): triage input carries `all_day`; a fake triage answer with `kind` per all-day event is stored and returned with the run; engine plan input lists Busy days as full-day busy and Away days under `away_days`, ignores Reminder; the `calendar` trigger claims one adaptation per goal for an Away window, the run reason names the dates, the result is a pending proposal even for an `Auto` goal; a second scan claims nothing.
- [ ] Implement; update the triage persona instructions for kinds. `sh crates/aroh-server/tests/run.sh` green. Commit `feat(server): all-day kinds in triage, planning and away adaptation`.

### Task 4: App views, edit sheet and chips

- [ ] vitest first: `allDayViews(snapshot, zone)` gives inclusive local dates for EventKit all-day spans (midnight to next midnight is one day, three-day span is three days); the triage proposal reader maps run output to `{ eventId: kind }`; `connect` sends all-day unmatched events to triage.
- [ ] Port: `AllDayView.kind` and `suggested`, three-option `AllDayChoice`, chip tap reopens, month cells and week strip markers, `ActionSheet` on the sheet primitive with an inline mode for mockups, long-press drag on phone. Remove the screens' local blocked-day filter (the store is the truth). Wire `/today` and `/calendar` to `editAction`, `deleteAction`, `setDayKind`.
- [ ] Update `/mockups` so it stays the reference: the three kinds on the choice card, a Busy day, an Away chip, a Reminder chip. Rebuild with `AROH_MOCKUP=1 pnpm build`.
- [ ] `pnpm check`, vitest. Commit `feat(app): all-day row and calendar edit sheet`.

### Task 5: Simulator acceptance

- [ ] `git merge master`.
- [ ] Write `e2e/pod18.js` and `e2e/pod18.sh` following `e2e/pod15.js` and `e2e/drive-calendar.js`: seed device all-day events (the dev calendar seed or EventKit sample), open Today and Calendar, confirm a Busy day and see its actions move, mark Mom's birthday Reminder, open the edit sheet on an action, move it 30 minutes, resize it, delete one with confirm, sync, and confirm the server recorded the edits.
- [ ] Create `aroh-p18`, dev server on 1426, run the flow, capture into `docs/screenshots/pod18/`, take the stack down and remove its volumes, `git checkout -- apps/aroh/src-tauri/gen/apple`. Commit `test(e2e): pod 18 all-day kinds and edit sheet`.

## Open Items

- Away proposals need the server and a provider; offline, the chip and kind are saved and the adaptation runs on the next sync.
