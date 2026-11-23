# Build 4 gate (Q67)

Date: 2026-11-23. Branch `chore/gate-build4`. It was first run on master `bab418e`, then master `052fd24` was merged in (pod 30: plan strip, can't-fit card, back bar, Dashboard tab and Inbox bell; pod 34: Dashboard screen) and the whole gate was run again on the merged tip. Simulator `aroh-gate4` (iPhone 15 Pro, iOS 26), debug build, frontend from vite on port 1439, run headless. Everything below is from the merged tip.

Verdict: **GATE FAIL**. Every guard passes and every scripted overflow and font check passes. But the keyboard check and the screenshot review found six defects. D4 hides what the owner types into every intake answer and into the can't-fit card, which is a core flow.

## How the gate ran

| Step | What ran |
|---|---|
| Build | `AROH_E2E_DEVICE=aroh-gate4 AROH_DEV_PORT=1439 sh e2e/pod33.sh build` |
| Pod 30 and 34 | `AROH_FLOW=pod30-app.js AROH_SHOTS=gate4 AROH_THEME=light\|dark sh e2e/pod30.sh`, and `sh e2e/pod34.sh`. Both use the dev seed (the pod 29 server is not live). |
| Guards | `e2e/pod31.sh gate4`, `e2e/pod32.sh` with `AROH_P32_MIC=ask` and `deny`, `e2e/pod33.sh` |
| Gate | `sh e2e/gate4.sh` (full run). A harness bug stopped its tap pass at the Dashboard bar chart (see Guard and harness fixes). The screens from 12b to 22 were then rerun with `AROH_GATE_NOKB=true AROH_GATE_ONLY=[...]`. |
| Can't-fit keyboard | the `pod30-app.js` fakes up to the can't-fit card, then the gate's keyboard check on its number answer |

Raw results are in `docs/gate/build4-results.txt`, and every tap is in `docs/gate/build4-taps.txt`.

What the screenshots show, and where it comes from:

- **Local data is real.** `e2e/gate4-setup.js` loads the six-pack sample through `app_dev_seed`. `e2e/gate4_seed.py` then edits the app's own SQLite store:
  - It makes goal, series and action titles long.
  - It adds a fourth goal ("Read The Joy of X ...").
  - It adds 10 one-off actions today and 3 on each of the next six days. Today has done, skipped and missed actions.
  - pod 34 uses `app_dev_dashboard_seed`: 120 days of history and one finance plugin target.
- **Engine data is faked inside the webview.** `e2e/gate4.js` replaces `engine.call` and intercepts the Tauri IPC fetch for `app_account`, `app_sync` and `app_engine`. That gives four runs:
  - Six pack: waiting, with 6 intake questions.
  - Save $10k: waiting for plan approval. This now renders the pod 30 plan card with its week strip.
  - Learn piano: stopped at its time limit.
  - Read The Joy of X: working.
  - The account shows as signed in, and a coach library of 5 personas is faked.
  - `e2e/pod30-app.js` fakes a plan approval run and a `limit_choice` run, which give the plan card and the can't-fit card.
- **What this cannot prove.** Nothing here runs the real engine, sync or Supabase. Retry and Stop do not change a faked run.

## Results per check

| Check | Result | Evidence |
|---|---|---|
| pod 30: Dashboard tab, Inbox bell, plan card week strip with a session tapped and the next week, can't-fit card with Raise picked, back bar labels | PASS, light and dark:<br>- back label "Back to Today" from Today<br>- "Goal" from a goal<br>- back from Coaches restores the goal's scroll (400 of 400) | `docs/screenshots/gate4/guards/pod30/` |
| pod 34: Dashboard empty and seeded:<br>- minutes by goal and by category, with a week picked<br>- trends and the plugin target<br>- the Behind, Ahead and Learning filters | PASS, light and dark. Overflow 0, plugin target shown | `docs/screenshots/gate4/guards/pod34/` |
| pod31 guard:<br>- no overflow at 393 and 375 on 13 routes, now including `/dashboard`<br>- done style on Calendar<br>- Goals tab from a goal<br>- week swipe | PASS, light and dark | `docs/screenshots/gate4/guards/pod31/` |
| pod32 guard: mic prompt (ask) and mic denied | PASS | `docs/screenshots/gate4/guards/pod32/` |
| pod33 guard: delete goal both ways, Routines has no delete | PASS | `docs/screenshots/gate4/guards/pod33/` |
| First-launch setup, 3 steps, light and dark | PASS | `light/00-setup-*.png`, `dark/00-setup-*.png` |
| Every screen in light and dark: 24 screens, scrolled to the end, now including the Dashboard tab (12b) | Captured and reviewed | `docs/screenshots/gate4/light/`, `dark/` |
| Horizontal overflow at 393 and 375 on all 24 screens | PASS, 0 screens | results |
| Inputs at least 16 px on every screen | PASS on all 24 | results |
| No tab bar on the keyboard, composer above it, nothing off screen | PASS on every field. The tab bar is hidden and the composer stays above the keyboard. | `docs/screenshots/gate4/keyboard/` |
| Real software keyboard on 30 fields:<br>- goal chat, New Goal title, description and new category<br>- attach link, Log and Today quick-log<br>- places search, coach rule<br>- 5 intake answers, the can't-fit number answer<br>- 7 Settings fields signed in and 8 signed out | 24 PASS: the keyboard opens and stays, the field is above the keyboard and not covered, the top bar is on screen, the font is 16 px or more.<br>6 FAIL: every intake answer and the can't-fit answer is covered by the composer (D4) | `keyboard/04`, `11`-`14`, `30` |
| Every visible control tapped | 280 taps:<br>- 248 changed the screen<br>- 4 called native code<br>- 18 did nothing (two of them are defects D5)<br>- 6 phone-only, 4 external links | Tap list below |

## Defects

| ID | Severity | What | Screenshot | Where |
|---|---|---|---|---|
| D4 | high | When the keyboard opens on any intake question answer, or on the can't-fit card's "Your answer" number, the field sits behind the goal chat composer ("Message your personas"). The hit test at the field's centre lands on the composer textarea. The owner cannot see what they type. Fields outside the goal card scroll clear. | `keyboard/04-number-question.png`, `keyboard/11-question-1.png` to `14-question-4.png`, `keyboard/30-cant-fit-number.png` | Goal card questions (`QuestionGroup`, `NumberInput`, `CantFitCard`) inside the goal card chat, whose composer is pinned at the bottom |
| D5 | medium | Two buttons do nothing because they have no click handler:<br>- Settings > Search providers > "Add"<br>- Settings > Location > "Open iOS Settings" | `light/14-settings-3.png`, `light/17b-settings-location-on.png` | `SettingsScreen.svelte:202`, `LocationSettingsScreen.svelte:60` |
| D1 | medium | Overlapping actions in the Day and Week hour grid are drawn on top of each other, so the earlier one is hidden and hard to tap. Example: "Strength session ..." at 18:00-19:00 sits under "Strength session B" at 18:15-19:00. | `light/02-calendar-day-2.png`, `dark/02-calendar-day-2.png` | `CalendarGrid.svelte`: every block is `absolute inset-x-1`, with no columns |
| D6 | low | The plan card summary says "1 days a week". It also says "finishes 26 Nov" with no year, although that date is 26 Nov 2027 ("149 days after your date", "Week 1 of 53"). | `light/07-goal-plan-4.png` | `apps/aroh/src/lib/ui/plan/strip.ts:194` and `:202` |
| D2 | low | The goal's "Next sessions" list shows times with no day. Sessions on different days look like duplicates or out of order: two "Weekly budget review 12:00-12:30", and two piano "16:30-16:50". | `light/07-goal-plan-3.png`, `light/08-goal-stopped-2.png` | `nextPlanned` / action list on the goal screen |

**D3 is gone on the merged tip.** The raw ISO time and RRULE in the plan card were replaced by the pod 30 plan card.

**Not fixed here.** None of these is a one-line obvious fix, so none is fixed on this branch.

## Observations, not defects

- **Back bar labels follow history.** The label is the screen you came from: "Back to Inbox" on a goal opened from the Inbox, "Back to Settings" on a goal reached after Settings. On a cold start it is the parent. This is the Q66 rule.
- **A goal with nothing due shows "Ahead".** On the Dashboard, "Read The Joy of X" shows "0 of 0 done to date, Finish 23 Nov, 70 days early" with Ahead. That follows `pace()`, because its only session is today. The owner may want "Not started" there.
- **Thin demo data.** "What Aroh knows about you" shows its empty state, and the Location log shows "No visits yet".

## Tap list

Each control on each screen is tapped once. Labels with digits are collapsed, so one month cell stands for all 42.

**Did nothing (18):**

- **16 expected.** These controls are already selected, or have nothing to add yet:
  - the Today tab while on Today, and the Dashboard tab while on Dashboard
  - "Today" in the Calendar while today is shown (Day, Week and Month)
  - Day, Week or Month while that view is selected, and the selected week day "Mon 23"
  - "Balanced" while Balanced is selected
  - "All" and "Goal" on the Dashboard while selected
  - the coach "Add" with an empty rule
- **2 defects (D5).** Settings "Add" (Search providers) was tapped on all three Settings screens. "Open iOS Settings" was tapped on both Location screens.

**Called native code (4):**
- Retry (`app_sync`)
- the New Goal category guess (`guess_category`)
- Sign out (`app_sign_out`) on two Settings screens

**Not tapped:**
- phone-only: "Speak a log entry" and "Speak a message"
- external links: the research sources

**Covered by the pod 30 guard instead:** the header back arrow and the Inbox bell.

**Out of scope:** controls inside sheets.

## Only a real phone can confirm

- **Mic:** speech to text in Today quick-log, Log and goal chat. The simulator shows only the permission prompt and the denied state.
- **Attachments:** the camera, photo picker, file picker and upload.
- **Notifications:** the first-launch prompt and reminders arriving on time.
- **Location:** the permission sheets, visits, place nudges and the MapKit map.
- **Calendar and Apple Health:** the permission sheets and their data.
- **Real engine runs:** research, questions, the plan, the can't-fit card from a real pod 29 server, Stop, Retry, cost, and sync with Supabase.
- **Back gesture:** the iOS edge-swipe back gesture against the new back bar.

## Coverage

- **Covered on the merged tip:**
  - all guards and pods 30 and 34, light and dark
  - all 24 screens for overflow and 16 px inputs
  - 30 keyboard fields
  - taps on all 24 screens
  - the light screenshot set
- **Dark set: lag risk.** Some dark file names point one step late (see Guard and harness fixes). Every dark screen state was captured and reviewed.
- **Not covered:**
  - controls inside sheets
  - the header back arrow and the Inbox bell in the tap pass (pod 30 covers both)
  - anything that needs the live pod 29 server

## Guard and harness fixes on this branch

- **`e2e/pod32.js`:** the first-launch skip loop now waits for five steady checks. The merge kept master's switch from "Inbox" to "Dashboard" as the ready text.
- **`e2e/pod32.sh`, `e2e/pod33.sh`:** the failure case is now matched before the generic screenshot case, so a failing run stops at once.
- **`e2e/gate4.js`:**
  - The ready text is now "Dashboard".
  - A Dashboard screen (12b) was added.
  - SVG chart bars are tapped with a dispatched click, because SVG elements have no `click()`.
- **`e2e/gate4.sh` opens (time-boxed).**
  - **Problem:** the host loop re-reads and re-splits the whole mark log for every line. On long runs it falls behind the driver. Some dark screenshots are then taken one step late and carry the name of the step before, for example `dark/03-calendar-week.png` (Day view), `dark/09-goal-working.png`, `dark/14-settings.png` and `dark/22-settings-signed-out.png` (scrolled).
  - **Fix that failed:** a fix that read only new lines stalled the run, so it was reverted.
  - **Fix to try next:** make the driver wait for a host acknowledgement after each shot.
  - **Effect on the light set:** none. The light shots reviewed match their names.
  - **Effect on the dark set:** a dark file can show a neighbouring step. Every dark screen was still captured and reviewed for theme defects, and none was found.
- **New `AROH_GATE_NOKB` and `AROH_GATE_NOTAPS`** skip the keyboard and tap passes on a targeted rerun.
- **Lost hour on the first merged run.** A vite server from before the merge was still running. It served hot-reloaded modules (`?t=`), so the drivers' imports patched a second copy of the data and engine modules, and pod30 and pod31 failed. A fresh vite fixed it. Restart vite after every merge.

## Re-gate (pod 35)

Date: 2026-11-23. Branch `fix/gate4-defects`, simulator `aroh-p35` (iPhone 15 Pro), vite on port 1441, restarted before each run. Only the checks that the defects touch were rerun. Shots are in `docs/screenshots/gate4/regate/`. Results are in `docs/gate/build4-regate-results.txt` and taps in `docs/gate/build4-regate-taps.txt`.

| What ran | Result |
|---|---|
| `AROH_GATE_SHOTS=.../regate AROH_GATE_ONLY=[02, 03, 06, 07, 08, 14, 17b, 21, 22] sh e2e/gate4.sh` | 51 of 51 results pass: overflow 0 and inputs at least 16 px on 9 screens in light and dark, all 29 keyboard fields |
| `AROH_FLOW=pod30-app.js AROH_P30_KB=true sh e2e/pod30.sh`, light and dark | can't-fit number field passes. Back label "Back to Today", goal scroll restored 400 of 400 |
| `sh e2e/pod31.sh` overflow guard, light and dark | 0 routes overflow. Goals tab, done style and week swipe ok |
| Taps on the 9 screens | 134 taps. The 7 that did nothing are all expected (already-selected Today, Day, Week, Calendar and Mon controls; coach Add with an empty rule) |

| Defect | Verdict | Commit | Evidence |
|---|---|---|---|
| D4 | PASS | 3d4c7fb | Keyboard fields 30 of 30. The intake answers (`question-1` to `question-4`, `number-question`) and the can't-fit number are clear of the composer and above the keyboard. While another field is focused, the composer is hidden through `[data-composer]:not(:focus-within)`, and it comes back on blur. |
| D5 | PASS | ff9888c | "Open iOS Settings" calls `plugin:aroh-speech\|open_settings` (the gate stubs it, so the simulator stays in the app). The Search providers section was removed rather than wired: research uses Claude's hosted web search (Q63), and the app has no way to add a search setting. Search setting rows, sync and the server fallback for other vendors are unchanged. |
| D1 | PASS | 413aac3 | `regate/light/02-calendar-day-2.png`: "Strength session" 18:00 and "Strength session B" 18:15 sit side by side. |
| D6 | PASS | 9ec0916 | `regate/light/07-goal-plan-4.png`: "15-30 min, 1 day a week · 12:00 · finishes 26 Nov 2027". The plan card's session and day counts are pluralized too. |
| D2 | PASS | f1e96d8, 67e7596 | `regate/light/08-goal-stopped-2.png` shows "Tue 24" on the session tomorrow. 67e7596 then labels every row of a list that spans days, using "Today" for today's rows. Today's own list is unchanged. 67e7596 came after the harness run and is covered by its unit test only. |

**Not rechecked:** screens other than the 9 above, controls inside sheets, and the pod 32 and pod 33 guards (no code they cover changed).
