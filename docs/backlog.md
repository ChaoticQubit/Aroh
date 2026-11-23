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
| B13 | Pod 25 DONE | chat composer | Keyboard keeps composer visible: not verified, needs a manual tap on simulator or device | medium | closed, f5e5a1c (same fix as B19, keyboard shown on simulator) |
| B14 | Pod 25 DONE | `docs/decisions.md` | Research before questions not yet recorded in decisions.md (foundation) | low | open |
| B15 | PR #2 review (P2) | `apps/aroh/src/lib/app/chat.svelte.ts:179` | Stopped runs not restored when reopening a goal chat, so Retry is missing after reopen | medium | open |
| B16 | Pod 27 (B1 fix) | `crates/aroh-domain/src/travel.rs` `settle` | An old-zone occurrence edited elsewhere (locked, done or re-stamped) before the device pulls it after a zone change is left as is, so it can sit beside the new-zone occurrence of the same slot | medium | open |
| B17 | Owner, build 2 | provider key status | Claude provider key missing after updating build 1 to build 2 | critical | closed, not a bug: owner confirmed the key was kept (2026-11-22) |
| B18 | Owner, build 2 | `apps/aroh/src/lib/app/questions.ts` | Number slider capped at 100 (fallback bounds); typing 315 clamps to 100 | high | closed, 9f402c1 |
| B19 | Owner, build 2 | chat composer | Opening the keyboard breaks the whole layout | high | closed, f5e5a1c (simulator; confirm on phone) |
| B20 | Pod 26 DONE | attachments | Photo and File upload unverified until the prod storage migration is applied; check on build 3 | high | open |
| B21 | Pod 26 DONE | personas | A copied intake coach misses the lasting/known instruction lines, so no profile facts for that owner | medium | open |
| B22 | Pod 26 DONE | replan | Files added later on the goal card reach only future new-goal runs; replan does not read files | medium | open |
| B23 | Pod 26 DONE | attachments | Removing an attachment does not delete its storage object | medium | open |
| B24 | Pod 26 DONE | mockups | Point the mockup GoalCard at the real card | low | open |
| B25 | Pod 26 DONE | goal card | Top bar says "Goals" instead of the goal name | low | open |
| B26 | Pod 26 DONE | chat composer | Placeholder says "Message your personas" instead of the lead coach | low | open |
| B27 | PR #3 review (P2) | `apps/aroh/src-tauri/src/commands/goals.rs:112` | Snapshot not reloaded after profile edits, so the profile screen can show stale facts until the next refresh | medium | open |
| B28 | Owner, build 3 | plan node request | New goal run stopped at node "plan": HTTP 400 "messages: text content blocks must be non-empty" | critical | closed, a8eed02 + f978f84 (prod PR #4) |
| B29 | Owner, build 3 | Today action rows | Action rows overflow the screen width; checkbox only visible after scrolling right | high | closed, c829697 (pod 31) |
| B30 | Owner, build 3 | Inbox run cards | Stopped goal-planner cards still show a spinning blue circle | medium | closed, ac51d57 (pod 31) |
| B31 | Owner, build 3 | composer mic, attach Photo | Mic, camera and every attachment (Link, Photo, File) in New Goal do nothing on iPhone; Info.plist lacks NSCameraUsageDescription and NSPhotoLibraryUsageDescription | high | closed, 023168f (simulator up to probe and prompt; phone confirms on build 4) |
| B32 | Owner, build 3 | Goals tab | Tapping Goals from inside a goal shows a blank screen until another tab is visited | high | open, pod 31: not reproduced on the simulator (dev build, sample goals, no engine run, goal screen scrolled or not); e2e/pod31.sh checks it. Pod 30: not reproduced on a production-asset build either (e2e/pod30-b32.js); the layout now resets the main scroller on every navigation and restores it on back, a likely cause if the blank screen was a stale scroll offset. Needs the owner's phone check on build 5 |
| B33 | Owner, build 3 | write_plan tool | First goal's later run stopped: write_plan invalid arguments "bad cadence for quizzes score" after the plan was applied | high | closed, b339d13 + b638eb1 (prod PR #5) |
| B34 | Owner, build 3 | Calendar status | Action checked done on Today still shows blue (planned) on the Calendar, not marked done | high | closed, 9e9056f (pod 31) |
| B35 | Owner, build 3 | Calendar | No swipe left or right to change day or week | medium | closed, e068703 (pod 31) |
| B36 | Owner, build 3 | intake, planner | Intake questions shallow for every goal; workout plan had no session content (exercises, sets, reps) and no nutrition guidance | high | closed, a23d3dd |
| B37 | Pod 33 DONE | delete goal | Kept actions take the Routines reminder setting (on) even if the deleted goal had reminders off | low | open |
| B38 | Pod 33 DONE | storage bulk delete | DELETE /storage/v1/object/goal-files with prefixes tested only against a stub; confirm on prod | medium | open, check on build 4 |
| B39 | Pod 31 DONE | Calendar phone week view | Swipe moves 7 days while the phone grid shows one day; next page not drawn during the drag | low | open |
| B40 | Pod 29 DONE | troupekit http | `http::tests::times_out` fails in the full parallel run, passes alone (flaky) | low | open |
| B41 | Pod 29 DONE | limit_choice | Choosing "Move the date" does not update the goal's target date | medium | fixed on feat/plan-view (pod 30), server test `moving_the_date_on_the_limits_card_sets_the_deadline`; ships with build 5 |
| B42 | Pod 29 DONE | replan, record_proposal | No daily-cap check on replans and proposals, only on new plans | medium | open |
| B43 | Pod 29 DONE | web search errors | "web search not enabled" 400 from the API surfaces raw, not as a clear message | medium | open |
| B44 | Pod 29 DONE | coach_questions eval | Matches English keywords only | low | open |
| B45 | Pod 30 dataviz check | `apps/aroh/src/lib/ui/tokens/tokens.css` dark `--cat-*`, `--series-*` | Dark category and series colours fail the dataviz validator's lightness band; light order cat-1, cat-5, cat-4 passes | medium | open, foundation |
| B46 | Pod 30 | `e2e/pod30-app.js` | One of four runs timed out waiting for the plan card on the first goal (20 s); passed on retry | low | open |
| B47 | Gate 4 D4 | goal chat composer | With the keyboard open, the chat composer covers the focused intake answer and the can't-fit number answer | high | closed, 3d4c7fb (re-gate PASS) |
| B48 | Gate 4 D5 | Settings | Search providers "Add" and Location "Open iOS Settings" have no click handler | medium | closed, ff9888c (re-gate PASS; Search providers section removed, Q63) |
| B49 | Gate 4 D1 | Calendar day and week | Overlapping blocks draw on top of each other | medium | closed, 413aac3 (re-gate PASS) |
| B50 | Gate 4 D6 | plan card summary | "1 days a week"; finish date has no year when it is in another year | low | closed, 9ec0916 (re-gate PASS) |
| B51 | Gate 4 D2 | goal Next sessions | Times shown without the day, so sessions look duplicated | low | closed, f1e96d8 (re-gate PASS) and 67e7596 (Today label, unit test only) |
| B52 | PR #6 review | server limits | When the workload exceeds the 366-day horizon, finish is None and "Move the date" returns fits true with no move_to | medium | open |
| B53 | PR #6 review | plan card | Open-ended recurrences (FREQ=DAILY) show the 366-day preview end as "finishes" with a finish flag | medium | open |
| B54 | PR #6 review | back bar | Cold start at a deep route: replaceState Back pushes the parent on the synthetic stack, so the next Back is mislabelled and calls history.back() | medium | open |
| B55 | Owner, build 4 | question answers | Spaces could not be typed in answer boxes: each keystroke was trimmed and bound back | high | closed, see fix(app): keep spaces while typing an answer |
| B56 | Owner, build 4 | server DB pool | write_plan failed: EMAXCONNSESSION max clients 15; pool max 10 per Vercel instance | critical | closed, fix(server): cap database connections per instance |
| B57 | Owner, build 4 | profile facts | A workout time fact was offered as known for reading and office-prep questions | high | closed, fix(agents): profile facts answer only questions about the same subject |
| B58 | Pod 37 DONE | Run again | A Run again plan has no cards until the next weekly fill; owner expects cards right away | high | open |
