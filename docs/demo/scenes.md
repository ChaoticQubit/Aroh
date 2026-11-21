# Aroh MVP demo video

File: `docs/demo/aroh-mvp-demo.mp4` (5:24, 720x1712 H.264, 8 MB). Recorded on 2026-11-21 on a dedicated iOS 26.1 simulator, "aroh-demo2 iPhone 15 Pro". It is a debug build of the app at `8a0b653` (main `c4e8d34` plus the course-progress fix), with the frontend built in (`custom-protocol`) and ad-hoc signing. `AROH_DEV_SCRIPT` drove the real UI at human pace (a grey ring marks each tap), and `AROH_DEV_CLAUDE_KEY` loaded the owner's key into the keychain. Every agent step is a live Claude call. Long agent waits are cut, and the caption says so.

## Scenes

| Time | Scene | What you see |
|---|---|---|
| 0:00 | Title | Title card |
| 0:04 | 1. Fresh app | The "Set up Aroh" card on Today: Claude key, Sync (optional), Reminders already allowed. "Add key" opens Settings with "No key saved." |
| 0:13 | 1. Key set | Settings shows "Saved, ending in ••••". |
| 0:17 | 1. Setup card | The key step is ticked. Skip on Sync marks it "Skipped. Set it up later in Settings." |
| 0:24 | 2. New goal | "Finish CS50 Python", https://cs50.harvard.edu/python/, finish by Jan 31, 2027. Tap "Read the course". |
| 0:34 | 2. Agent reads the course | Progress steps (8 s wait cut) |
| 0:37 | 2. Questions | Claude asks 8 questions (minutes per day, time of day, days off, session length, start, blackout dates, buffer, longer weekend sessions). All 8 answered, then "Make my plan". |
| 1:03 | 2. Planning | "Plan every session up to 2027-01-31" (95 s wait cut) |
| 1:06 | 2. Proposed plan | 77 sessions, 76 h 30 min, with Claude's summary and the day-by-day list. Approve plan. |
| 1:16 | 3. Goal detail | Sessions done 0 / 77, the plan, and the course-progress plugin: Course done **0 / 77**, Sessions left 77, Up next |
| 1:25 | 3. Today and Calendar | The sessions on Today, then the Calendar day view opens scrolled to the current session (00:55) |
| 1:40 | 4. Reminder | The banner "In 10 min: Week 0: Functions, Variables (1/5)" appears while the app is open (1 min wait cut) |
| 1:52 | 5. Mark done | Done on the session, the "Done: ..." toast with Undo, then Goal detail reads 1 / 77 and Course done 1 / 77 |
| 2:09 | 6. Small replan | One session missed yesterday. On reopen the replan auto-applies one move, with the "Calendar adjusted: 1 moved. Undo" toast. The Calendar shows it at 08:00 today. |
| 2:35 | 6. Big replan | Five sessions missed yesterday (10 s Claude wait cut) |
| 2:38 | 6. Replan review | /replan lists 5 moves spread forward at 1 h a day: Sat 08:00, Sun 08:00 and 09:00, Mon 08:00 and 09:00. Apply, "Nothing to review", then the Calendar day by day. |
| 3:10 | 7. Targets | Settings, Trackers: water 2500 ml, workout 45 min, food 2200 kcal, Save |
| 3:30 | 7. Log | Typed "2 glasses of water and a 30 min run" is parsed by Claude into water 500 ml and workout 30 min. Then the "Glass of water" and "Workout 30 min" chips, totals against targets, and the entry list. |
| 4:00 | 7. Check-in | `/log?checkin=1`: Claude's check-in ("Great job beating your workout goal! ... any food logged yet"). Scrolled, the content stays below the status bar. |
| 4:19 | 7. Voice | The mic button. iOS asks for the Microphone first. |
| 4:28 | 7. Voice | After Allow (cut), iOS asks for Speech Recognition. |
| 4:41 | 8. Plugin YAML | Contents of `apps/aroh/plugins/water-tracker.yaml` |
| 4:49 | 8. Plugin rendering | The water tracker in `today.sidebar`: +250 ml taps take Water today from 750 to 1,000 to 1,250 ml. Then `log.trackers`: Water vs target, +250 ml to 1,500. |
| 5:12 | 9. Sync | Settings, Sync: Supabase URL and publishable key form (no live sign-in), the safe-area layout when scrolled |

## Fixed for this recording

- **Course done showed "0.01 / 1".** `goal.progress` returned the done fraction while the course-progress plugin set `max: 1`. Fixed in `8a0b653`: `goal.progress` is now the count of done actions, a new `goal.actions_total` query gives all actions, and the plugin uses `max: goal.actions_total`, so the bar reads "1 / 77". The test `goal_progress_counts_done_actions` failed red (`left: 0.25, right: 1.0`), then passed.

## Shown working (fixed on main since the last video)

- The first-run "Set up Aroh" card.
- A single missed session makes a small, auto-applied replan (1 move, Undo toast).
- The big replan spreads missed work over the next days under the goal's daily cap instead of one large catch-up block.
- The Calendar opens at the current session.
- Content stays below the status bar and the water chips sit above the tab bar.
- Voice asks for the Microphone, then Speech Recognition.
- Typed Log entries parse, and the water buttons on Today log.

## Still not working, or differs from the brief

- **Replans ignore the preferred time of day.** The owner answered "Evening", but the small replan moved the missed session to 08:00, and the big replan put all five sessions at 08:00 to 10:00.
- **Missed sessions vanish from the Calendar.** Yesterday's day view is empty after the replan, so the missed blocks are not visible anywhere.
- **Two time formats on Today.** The plugin's "Today's sessions" list shows "12:55 AM", while the session rows show "00:55".
- **A query change on the same route does nothing.** Going from `/log` to `/log?checkin=1` inside the app shows no check-in. It only appears when you arrive from another screen, as in the video.
- **The Speech Recognition prompt says data goes to Apple.** That is the iOS system text. Aroh's own line says audio never leaves the device.
- **Simulated parts.** Missed sessions were made by editing `start_at` in the app's SQLite while the app was closed. The reminder session was moved to 11.5 minutes ahead so the 10-minute reminder fires about 1.5 minutes later. The notification permission was pre-set in the simulator, because this Mac has no Simulator app to tap Allow. The "Allow" tap between the two voice prompts was done by granting the microphone with `simctl` and is cut.
- **The setup card's Reminders step differs between 0:04 and 0:17.** The app was reinstalled between the two takes, which reset the notification permission until it was set again. In normal use the step stays ticked.
- **The key's last 4 characters are masked by the drive script.** Settings normally shows them.

## Claude calls

About 10 live calls in total:

- Goal flow: 6 (analyze, questions, plan, done twice: the first take answered the blackout-dates question wrongly and was re-recorded)
- Replans: 2 (one small, one big)
- Intake parse: 1
- Check-in questions: 1 or 2 (the first, from the same-route navigation above, may have called Claude without showing)
