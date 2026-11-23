# Pod 37: action cards as notes (Q71)

Decisions and data design: `docs/agent-decisions.md`, section "Action cards as notes (pod 37)". Trunk-based: one short branch `feat/action-cards`, merged by the leader. No reviews in this repo (pod rule). TDD per task.

## Phase A: mockup (this branch, stop after it)

1. **Card logic** (`apps/aroh/src/lib/ui/action/card.ts`, test `card.test.ts` first): card and log types; `setsLine` (sets as `3 x 8 @ 60 kg`, or `8, 8, 7 @ 60 kg` when they differ); `setState` (planned, done, edited); `progress` (done and total sets and checklist items); `cardDiff` (added, removed and changed lines between two cards plus minutes).
2. **Components** (`apps/aroh/src/lib/ui/components/action/`): `ActionCard.svelte` (header, why, blocks, notes, session log with easy / ok / hard, chat entry; full, logging and outline modes), `WorkoutBlock.svelte`, `StepsBlock.svelte`, `ChecklistBlock.svelte`, `CardChat.svelte` (messages, diff with Undo, composer with mic), `ScopeSheet.svelte` (this session only, this and all future).
3. **Mock route** (`apps/aroh/src/routes/mockups/action-cards/`): `ActionCards.svelte` with the six frames in `PhoneShell` with a back bar, seed data in `data.ts`; a new `action-cards` view in `routes/mockups/+page.svelte`, solo frames by hash `#action-cards/<i>/<theme>`.
4. **Screenshots** (`e2e/pod37.sh`, `e2e/pod37.js`, copied from pod 30): simulator `aroh-p37`, vite on 1443, light and dark, a scrolled shot where a frame is longer than the screen, overflow check per frame, into `docs/screenshots/pod37/{light,dark}`.
5. **Verify**: vitest, `pnpm check`; restore `src-tauri/gen/apple`; shut down `aroh-p37` (keep it); `git merge master`; `graphify update .`.

## Phase B: build (after owner approval, not on this branch yet)

1. **Schema**: `card` column on `actions` and `action_series`, table `action_logs` (SQLite migration in `crates/aroh-store/src/db.rs`, row structs and `SYNCED` in `crates/aroh-domain`, Supabase migration with the LWW trigger, RLS and `server_seq` like the other synced tables). Tests: round trip, sync convergence of a log written on two devices.
2. **Materialize**: occurrences copy the series `card` as an outline. Test in `recur.rs`.
3. **Tools and personas**: `write_cards`, `edit_card`, `card` proposal op with undo; `card-writer` and `card-coach` personas; `cards_filled` eval; planner `outline` per week.
4. **Fill**: `fill:<date>` and `fill:backfill` window keys in `scan_reviews`; pg test in Kiri that a fill runs once per week and the backfill once per goal.
5. **App**: `/actions/[id]` route on the Phase A components, Tauri commands for log writes, voice logging through the speech plugin, Today and Calendar taps open the card.
6. **Verify**: Kiri tests, simulator e2e for the Q71 acceptance (open Leg day, log by tap and voice, next week reflects the log, chat edit with Undo).
