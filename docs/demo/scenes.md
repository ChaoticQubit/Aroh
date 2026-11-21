# Aroh MVP demo video

File: `docs/demo/aroh-mvp-demo.mp4` (5:10, 720x1712 H.264, 10 MB). Recorded on 2026-11-20 on a dedicated iOS 26.1 simulator, "aroh-demo iPhone 15 Pro". It is a debug build of the app at `3412db0` with the frontend built in (`custom-protocol`) and ad-hoc signing. `AROH_DEV_SCRIPT` drove the real UI at human pace, and `AROH_DEV_CLAUDE_KEY` loaded the owner's key into the keychain. Every agent step is a live Claude call. Long agent waits are cut, and the caption says so.

## Scenes

| Time | Scene | What you see |
|---|---|---|
| 0:00 | Title | Title card |
| 0:04 | 1. Fresh app | The "Set up Aroh" card on Today (key, sync, reminders already allowed). "Add key" opens Settings with "No key saved." |
| 0:20 | 1. Key set | Settings shows "Saved, ending in ••••". The setup card's Sync step is skipped, so the card hides itself. |
| 0:31 | 2. New goal | "Finish CS50 Python", https://cs50.harvard.edu/python/, finish by Jan 15, 2027. Tap "Read the course". |
| 0:50 | 2. Agent reads the course | Progress steps (wait cut) |
| 0:53 | 2. Questions | Claude asks 8 questions (minutes per day, time of day, days off, session length, time zone, blackout dates, buffer, start). The answers go in, then "Make my plan". |
| 1:17 | 2. Planning | "Plan every session up to 2027-01-15" (about 80 s of waiting cut) |
| 1:21 | 2. Proposed plan | 106 sessions, 106 h, with Claude's summary. Approve plan. |
| 1:31 | 3. Goal detail | Sessions done 0/106, next session, the plan summary, and the course-progress plugin (Course done bar, Sessions left 106, Up next checklist) |
| 1:42 | 3. Today and Calendar | A session on Today, then the Calendar day view for Fri, Sat and Sun with the evening blocks |
| 2:15 | 4. Reminder | The banner "In 10 min: Week 0: Functions, Variables (1/8)" appears while the app is open |
| 2:28 | 5. Mark done | Done on the session, the row strikes through, and a "Done: ..." toast offers Undo |
| 2:37 | 6. Small replan | Two sessions are overdue on reopen. Replan auto-applies one resize, with the "Calendar adjusted: 1 resized. Undo" toast. Then the Calendar shows the missed blocks. |
| 2:52 | 6. Big replan | Six sessions missed. /replan shows the diff (resize Sat catch-up 120 to 180 min, new 300 min catch-up on Sun). Apply, then "Nothing to review". |
| 3:18 | 7. Targets | Settings, Trackers: water 2500 ml, workout 45 min, food 2200 kcal |
| 3:33 | 7. Log | Typed "2 glasses of water and a 30 min run" is parsed by Claude into water 500 ml and workout 30 min. Then the "Glass of water" and "Workout 30 min" chips, and totals against targets. |
| 4:00 | 7. Check-in | `/log?checkin=1`: Claude's daily check-in question |
| 4:08 | 7. Voice | The mic button. iOS asks for Speech Recognition permission. |
| 4:22 | 8. Plugin YAML | Contents of `apps/aroh/plugins/water-tracker.yaml` |
| 4:30 | 8. Plugin rendering | The water tracker in `today.sidebar` (Water today, +250 ml logs 1,250 to 1,500) and in `log.trackers` (Water vs target bar, +500 ml) |
| 4:56 | 9. Sync | Settings, Sync: Supabase URL and publishable key form (no live sign-in) |

## Found and fixed during recording

- **Typed Log entries always failed.** Claude returned 400 `For 'array' type, property 'maxItems' is not supported` for the intake output schema. Fixed in `fd3cbaf`: the schema no longer uses `maxItems` or `minimum`, and `parse_model_output` already enforces both limits. A new test (`output_schema_avoids_keywords_claude_structured_output_rejects`) failed red, then passed green in a Kiri container.
- **The water tracker's quick-chips on Today did nothing.** `PluginSlot` rendered `Slot` without `onlog`, so the chips in `today.sidebar` were disabled. Fixed in `3412db0`: they log through `intake_log_chip` like the Log page. Verified in the simulator (1,250 to 1,500 ml).

## Did not work, or differs from the brief

- **A single missed session never makes a small replan by itself.** The replanner turns missed minutes into a resize of a same-goal session on the target day. When that day has none, it creates a "Catch up" session instead, and a create is always "big". The first miss therefore became a big replan. The small, auto-applied replan in the video works because the earlier big replan had already put a catch-up session on Saturday.
- **Replans pile missed work into large catch-up blocks.** Six missed hours became +1 h on Saturday and a 5 h block on Sunday. The brief describes spreading missed work over the following days.
- **Voice: the first prompt is Speech Recognition, not the microphone.** The microphone prompt comes after Allow and is not in the video.
- **The Calendar opens in the morning hours.** Evening sessions need a scroll.
- **Content scrolls under the status bar.** For example the check-in card and Settings headings sit under the clock when scrolled.
- **On Today, the water chips peek out below the tab bar.**
- **Two scenes are simulated.** The overdue sessions were made by editing `start_at` in the app's SQLite while the app was closed. The reminder session was moved to 12 minutes ahead so the 10-minute reminder fires about 1 minute later.
- **The key's last 4 characters are masked by the drive script.** Settings normally shows them.
- **The question for blackout dates was left empty.** The wizard still made the plan (7 of 8 answered).

## Claude calls

About 13 live calls in total:

- Goal flow: 3 (analyze, questions, plan)
- Replans: 4 (one of them for a seed mistake that was then rejected)
- Intake parses: 3 (the first hit the 400 above)
- Check-in questions: 3
