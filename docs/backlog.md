# Backlog

Open and closed findings and leftovers. Closed items stay, marked with the commit or verdict that closed it.

| # | Source | Where | Finding | Severity | State |
|---|---|---|---|---|---|
| B1 | PR #1 CodeRabbit | `apps/aroh/src-tauri/src/lib.rs:63` | Zone change shifts only stored actions; a server-held occurrence pulled after the shift keeps its old-zone instant, so two live occurrences of one slot can exist | high | closed, 27ebe3f |
| B2 | PR #1 CodeRabbit | `crates/aroh-domain/src/travel.rs:117` | A series whose start falls in the destination zone's DST gap fails validation and aborts the whole device zone change | high | closed, 27ebe3f |
| B3 | PR #1 review (P2) | `crates/aroh-app/src/travel.rs:48` | Deleted future occurrences are not carried into the new zone, so a deleted occurrence can come back | medium | open |
| B4 | PR #1 review (P2) | `crates/aroh-domain/src/travel.rs:83` | Re-keying actions on a zone change leaves open proposals pointing at the old action id | medium | open |
| B5 | PR #1 review (P2) | `crates/aroh-domain/src/travel.rs:125` | Shifted occurrence keys do not use recurrence DST gap semantics | medium | closed, 27ebe3f (with B2) |
| B6 | PR #1 CodeRabbit | `apps/aroh/ios-release.sh:27` | `AROH_ASC_KEY_PATH` is not resolved to an absolute path before `cd`, so a relative key path breaks upload | low | open |
| B7 | PR #1 CodeRabbit | `apps/aroh/src-tauri/src/commands/engine.rs:88` | Concurrent `send_zone` calls can deliver an older zone after a newer one | medium | open |
| B8 | PR #1 CodeRabbit | `apps/aroh/src/routes/inbox/+page.svelte:62` | Dismissed cards are not retained for the combined Inbox card limit | low | open |
| B9 | PR #1 CodeRabbit | `crates/aroh-server/src/triggers.rs:738` | `scan_reviews` can overwrite a `set_timezone` change with a value from the old zone | medium | open |
| B10 | Pod 20 DONE | `e2e/pod20.sh` | Simulator steps 7 to 10 (fixed clash note, edit-sheet switch, coaches card) and server checks never ran; steps 1 to 6 passed once, screenshots lost | medium | open, check on build 2 |
| B11 | Pod 20 DONE | `e2e/up.sh` | 20-minute engine wait too short for a cold build on a loaded Mac | low | open |
| B12 | Pod 20 DONE | action series | Repeating plan items get no Fixed time switch at approval; needs a locked column on action series | medium | open |
| B13 | Pod 25 DONE | chat composer | Keyboard keeps composer visible: not verified, needs a manual tap on simulator or device | medium | open, check on build 2 |
| B14 | Pod 25 DONE | `docs/decisions.md` | Research before questions not yet recorded in decisions.md (foundation) | low | open |
| B15 | PR #2 review (P2) | `apps/aroh/src/lib/app/chat.svelte.ts:179` | Stopped runs not restored when reopening a goal chat, so Retry is missing after reopen | medium | open |
| B16 | Pod 27 (B1 fix) | `crates/aroh-domain/src/travel.rs` `settle` | An old-zone occurrence edited elsewhere (locked, done or re-stamped) before the device pulls it after a zone change is left as is, so it can sit beside the new-zone occurrence of the same slot | medium | open |
| B17 | Owner, build 2 | provider key status | Claude provider key missing after updating build 1 to build 2 | critical | closed, not a bug: owner confirmed the key was kept (2026-11-22) |
| B18 | Owner, build 2 | `apps/aroh/src/lib/app/questions.ts` | Number slider capped at 100 (fallback bounds); typing 315 clamps to 100 | high | closed, 9f402c1 |
| B19 | Owner, build 2 | chat composer | Opening the keyboard breaks the whole layout | high | open, pod 26 |
