# Pod 43: replan status (Q77)

Branch `feat/replan-status`. Owner report: an action was moved, nothing showed, an overlap stayed, no replan ran.

## Path trace before this pod

| Hop | Where | What happened | How the app can see it |
|---|---|---|---|
| 1. Device edit | `apps/aroh/src/routes/{today,calendar}/+page.svelte` `onedit`, `commands/learning.rs:14` `app_edit_action`, `crates/aroh-app/src/write.rs:298` `edit_action` (locks the action) | Stored on device only | `aroh://changed` reload |
| 2. Sync push | `commands/engine.rs:67` `sync_if_signed_in`, only at start and resume (`lib.rs:73`) | **Gap 1:** an edit never pushed until the next start or resume | `app_sync` result |
| 3. DB trigger | `supabase/migrations/20261123150000_triggers_learning.sql:54-93` `aroh_action_events` | Inserts `action_events` kind `move`/`resize`/`delete` for a locked action; no job | service role only (RLS) |
| 4. Cron | same file `:102` `aroh-tick` every minute to `/v1/tick`; `20261125000000_step_mode_wake.sql:33` `jobs_wake` on job insert | Leader confirmed 200 every minute in prod | none |
| 5. Engine tick | `crates/aroh-server/src/api.rs:353` `tick`, `engine.rs:893` `tick`, `triggers.rs:388` `scan_if_due` (30 s per process), `triggers.rs:399` `scan` | Runs `scan_events` (missed/skipped/done only) and `scan_edits` | none |
| 6. Replan decision | `triggers.rs:631` `scan_edits` feeds `learning::similar_groups` (needs 3 similar edits, `learning.rs:19`) | **Gap 2 (root cause):** a single move is never replanned | none |
| 7. Replan run | `triggers.rs:440` `trigger_replan`, `engine.rs:577` `replan`, `engine.rs:431` `launch` (`runs` row, checkpoint, `jobs` row via `queue.rs:77`) | Never reached for a move | `GET /v1/runs/{id}`, `runs`/`run_events` select RLS, `run_events` in Realtime |
| 8. Apply | `tools.rs:484` `apply_proposal` writes actions and `proposals.undo` | | proposals sync (`run_id`) |
| 9. Sync back | `aroh_store::sync::sync_all` pull | | snapshot reload |
| 10. App status | none | **Gap 3:** no status line, no overlap mark, no Undo | |

## Built

- Server: `crates/aroh-server/src/conflicts.rs` `check` (overlap with other planned actions of any goal, busy time, Q64 cap from the `daily_minutes` profile fact). `triggers.rs` `scan_edit_conflicts` runs before `scan_edits`. It starts one replan per affected goal with window `edit:<event id>` and Source `edits`. A goal with a replan in flight is retried on a later scan. Learning still reads `handled`; this path marks `replan_checked_at` and `replan_run_id` (migration `20261201000000_edit_replan.sql`).
- API: `GET /v1/actions/{action_id}/replan` returns `event_at`, `checked`, `run_id`, `waiting_on`.
- Device: `app_edit_action` syncs at once. `crates/aroh-app/src/undo.rs` (`run_proposals`, `undo_run`, both undo shapes) behind `app_run_proposals` and `app_undo_run`.
- App: `lib/app/replan.ts` `phaseOf` (pure, tested), `lib/app/replan.svelte.ts` `editStatus`, `ReplanStatus.svelte` on Today and Calendar. `CalendarGrid.svelte` marks overlapping actions "Overlap".

## Tests

- `cargo test -p aroh-server --lib conflicts`
- `cargo test -p aroh-app --test undo`
- `vitest src/lib/app/replan.test.ts`
- PG: `tests/triggers.rs` `one_owner_move_into_an_overlap_replans_the_other_goal` (Kiri queue)
