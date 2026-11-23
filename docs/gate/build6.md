# Build 6 gate (Q67)

Date: 2026-11-23. Branch `chore/gate-build6` off master `bd048eb` (pod 40 `6f1c4d4`, pod 37 `3834a98`). Simulator `aroh-g6` (iPhone 15 Pro, iOS 26.1), debug build, frontend from a fresh vite on port 1447, run headless. Time-boxed to 90 minutes.

Verdict: **GATE FAIL**. No critical or high defect was found. The gate still fails, for two reasons:

- The gate did not finish in the time box. The keyboard pass, the tap pass and the guards did not run on build 6.
- The screenshot review found one medium defect (G1) and three low ones on the new action page and Run again screens.

## How the gate ran

| Step | What ran | State |
|---|---|---|
| Build | `AROH_E2E_DEVICE=aroh-g6 AROH_DEV_PORT=1447 sh e2e/pod37.sh build` | done, installed |
| Gate run 1 | `AROH_GATE_SHOTS=docs/screenshots/gate6 sh e2e/gate4.sh`, full | Killed at 14-settings during the light pass. Another agent's gate run wrote to the same scratchpad log file name, and this run exited with code 144. The 40 results it recorded all pass. |
| Gate run 2 | The same command with `AROH_GATE_ONLY` set to the 8 new screens | The light pass finished. The dark pass was stopped at the time box, at `23-action-workout-3`. |
| Keyboard pass (incl. the new action card fields and Run again answers) | added to `e2e/gate4.js` | **not run** (time box) |
| Tap pass (incl. Retry, Run again, Keep my answers, Edit answers first, card buttons) | added to `e2e/gate4.js` | **not run** (time box) |
| `e2e/pod37.sh` (mockups) and `AROH_FLOW=pod37-app.js` | | **not run** (time box) |
| `e2e/pod31.sh`, `e2e/pod32.sh`, `e2e/pod33.sh` | | **not run** (time box) |

Results: run 1 in `docs/gate/build6-results.txt`, run 2 in `docs/gate/build6-run2-results.txt`.

## Results per check

| Check | Result | Evidence |
|---|---|---|
| First-launch setup, light and dark | PASS. Run 2 logged `theme-live fail webview kept light` once during setup: the simulator's appearance switch was slow. Run 1 did not log it. | results |
| Overflow at 393 and 375, and inputs at least 16 px, screens 01 to 14 (run 1, light) | PASS, 20 of 20 | `build6-results.txt` |
| Overflow and 16 px on the new screens: failed run with Retry (08b), Run again sheet (09b), Run again with answers to edit (09c), Run again sheet with no answers (09d), workout card (23), list card (24), session in progress (25), card chat (26) | PASS, 16 of 16 | `build6-run2-results.txt`, `docs/screenshots/gate6/light/` |
| Screenshots of the new screens, light | Captured and reviewed | `docs/screenshots/gate6/light/08b-*` to `26-*` |
| Screenshots of the new screens, dark | `08b`, `09b`, `09c`, `09d` and `23` were captured. `09b` and `23` were reviewed and show no theme defect; the others were not reviewed. `24` to `26` were not captured. | `docs/screenshots/gate6/dark/` |
| Keyboard and spaces on every field | not run | |
| Taps on every button | not run | |
| Guards pod37, pod31, pod32, pod33 | not run | |

## Defects

| ID | Severity | What | Screenshot | Where |
|---|---|---|---|---|
| G1 | medium | On a steps or list card, the step text is cut off in the middle of a word, with no ellipsis and no wrap: "Start 400 g rice in the cook", "Season 1 kg chicken thigh", "Portion into 5 boxes, 200 g". The owner cannot read the step without tapping into it. | `light/24-action-list.png` | `EditBlock.svelte:58`: each step is a one-line `<input>` |
| G2 | low | Each block's title is shown twice: as the section heading and again as a heading inside the card. Examples: "Bring" / "Bring", and "Chicken rice bowls, 5 lunches" twice. | `light/23-action-workout-2.png`, `light/24-action-list.png` | `ActionCard.svelte:132` (section heading) and `EditBlock.svelte:31` (title input) |
| G3 | low | An empty card chat is taller than the screen. The first screen is blank, and the "Ask your coach to change this" composer shows only after scrolling. | `light/26-action-chat.png`, `light/26-action-chat-2.png` | `routes/actions/[id]/chat/+page.svelte`: `min-h-[calc(100dvh-8rem)]` is taller than the space between the header and the tab bar |
| G4 | low | After "Edit answers first", the goal shows "Run again with these answers". It has no Cancel, so the only way out of edit mode is to leave the page. | `light/09c-run-again-edit-3.png` | `GoalCard.svelte` `rerun` branch |

## Observations, not defects

- **Retry on a failed run looks right.** A transient failure shows the "Failed" chip, "The run stopped: ...", the cost and Retry, with no "Change time limit" (`light/08b-goal-failed-retry.png`). Run again is at the bottom of the goal (`-3`).
- **Run again sheet.** It shows "Keep my answers" and "Edit answers first" when answers exist (`09b`), and a single "Run again" when there are none (`09d`). Long goal titles wrap. Cancel is separate.
- **Run again is offered while a plan waits for approval** (`09b`, the Save $10k goal). Starting a fresh run there leaves the pending plan behind. This is a product question for the owner.
- **The session in progress looks right**: the set ticks, "0 of 14 done", and the "Say 'last set 7 reps'" composer above the tab bar (`25-action-logging.png`).
- **The back label is "Inbox" on every screen above.** The harness reaches each screen through `/inbox` (Q66 history rule).

## Harness changes on this branch

- **`e2e/gate4.js`:**
  - New screens: `08b` (a retryable failed run), `09b`, `09c` and `09d` (the Run again sheet with and without answers, and the edit mode), and `23` to `26` (the workout card, the list card, a session in progress, and the card chat).
  - The fake engine answers `POST /v1/goals/{id}/run-again` with a new `run-again` run.
  - The keyboard pass also covers:
    - the Run again answers
    - every text field on the two cards, the session and the card chat, up to 30 new fields per page
  - After a card edit the keyboard pass answers the scope sheet with "This session only".
- **`e2e/gate4_seed.py`:** the gate's one-off actions were cloned from a sample row and kept its card. A piano session then showed the leg workout and its "Why" (`light/23-action-workout.png`). The seed now clears `card` on cloned actions. **This fix was not rerun.**
- **Scratchpad clash.** Parallel gate runs must not share a log file name in the shared scratchpad. Run 2 used its own folder.

## Still to run before upload

Run 2 command for the new screens, then the full command:

```sh
AROH_E2E_DEVICE=aroh-g6 AROH_DEV_PORT=1447 AROH_GATE_SHOTS=$PWD/docs/screenshots/gate6 sh e2e/gate4.sh
```

Then, light and dark:

- `e2e/pod37.sh`
- `AROH_FLOW=pod37-app.js sh e2e/pod37.sh`
- `e2e/pod31.sh`
- `e2e/pod32.sh` (ask and deny)
- `e2e/pod33.sh`

## Only a real phone can confirm

The same as build 4:

- mic and speech, including the voice set logging on the session composer
- attachments
- notifications
- location
- the Calendar and Apple Health permission sheets
- real engine runs: Retry against a real transient failure, and Run again producing a new plan
- the back gesture
