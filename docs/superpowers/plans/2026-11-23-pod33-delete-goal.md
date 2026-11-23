# Pod 33: delete goal (Q68)

Decisions: `docs/agent-decisions.md`, section "Delete goal (pod 33)". Trunk-based: one short branch `feat/delete-goal`, merged by the leader. No feature flag (the entry point is small and complete). No reviews in this repo (pod rule). TDD per task: write the test, see it fail, implement.

## Tasks

1. **Domain write** (`crates/aroh-app/src/write.rs`, test `crates/aroh-app/tests/delete_goal.rs`)
   - `delete_goal(conn, clock, goal_id, keep_actions) -> Result<Vec<String>>` returns the storage paths to remove.
   - Tests: delete all tombstones every tied row and keeps preferences and usage; keep actions rehomes actions, series and series place links to Routines; Routines refuses; a missing goal is NotFound.
2. **Pending removals** (`crates/aroh-app/src/local.rs`): `queue_removals`, `pending_removals`, `clear_removals`; unit test in the same test file.
3. **Late rows in the tick** (`crates/aroh-app/src/tick.rs`): settle live actions and series of tombstoned goals. Test: a server-derived occurrence pulled after the delete is tombstoned (delete all) or rehomed (keep).
4. **Sync convergence** (`crates/aroh-store/tests/sync.rs` style, in `crates/aroh-app/tests/delete_goal.rs`): device A deletes, device B with its own occurrences syncs through a fake remote and matches A.
5. **Storage remove** (`crates/aroh-sync/src/storage.rs`): `remove(config, token, user_id, paths)`; test against the local one-shot server.
6. **Tauri command** (`apps/aroh/src-tauri/src/commands/goals.rs`, `lib.rs`): `delete_goal(goal_id, keep_actions)`: write, queue paths, `after_write`, sync; `run_sync` drains the queue.
7. **Server** (`crates/aroh-server/src/{engine,triggers,tools}.rs`, test in `crates/aroh-server/tests/triggers.rs`): stop runs of tombstoned goals in the scan and in `process`; tools refuse; `start_goal` and `retry` refuse. pg test via `crates/aroh-server/tests/run.sh` in Kiri.
8. **UI** (`apps/aroh/src/lib/ui/components/goal/DeleteGoalSheet.svelte`, route `goals/[id]/+page.svelte`, `client.ts`): "Delete goal" entry at the bottom of the goal screen, hidden on Routines; sheet with the two red choices and Cancel; `goto('/goals')` after. vitest for the client call shape if logic exists; `pnpm check`.
9. **Verify**: host `cargo test` for touched crates, `pnpm check`, vitest, Kiri pg; simulator `aroh-p33` port 1438 screenshots to `docs/screenshots/pod33/`; restore `gen/apple`; `git merge master`; `graphify update .`.
