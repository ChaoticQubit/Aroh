# Pod calendar: screen and data plan

Owner of: `/today`, `/calendar`, reminders (local notifications), `commands/calendar.rs`, and the new pure crate `crates/aroh-reminders`.
Built on top of foundation (`aroh-core` structs, SQLite, app shell, `<Slot>` from pod-plugins). Build starts after foundation lands and `git merge crazyagent/main`.

## 1. Data

No new tables. Calendar reads and writes `actions` and `goals` from the shared model, and uses `settings` keys:

| Key | Type | Default | Meaning |
|---|---|---|---|
| `reminders.enabled` | bool | `true` | Master switch for local notifications |
| `reminders.lead_min` | int 0..=120 | `10` | Minutes before `start_at` for the "starts soon" reminder. `0` means only the at-start reminder |
| `reminders.missed_grace_min` | int 0..=240 | `15` | Minutes after `end_at` before the "not marked done" nudge fires |

All action times stay UTC ISO 8601 in the DB. The device timezone (IANA name from the OS, read in the webview with `Intl.DateTimeFormat().resolvedOptions().timeZone`) is passed into every command that needs local day boundaries. No timezone is stored.

### Writes

| Command | Change | Rules |
|---|---|---|
| `calendar_set_status(id, status)` | `status`, `done_at`, `updated_at` | `status` in `done`, `skipped`, `planned` (`planned` is the undo). `done_at` set only for `done`, cleared otherwise |
| `calendar_move_action(id, start_at, end_at)` | `start_at`, `end_at`, `updated_at` | `end_at > start_at`, duration at most 24 h, action not `deleted`. A move of a `missed` action sets it back to `planned`. `est_minutes` unchanged |

Every write bumps `updated_at` through the foundation HLC helper so sync sees it, then calls `sync_reminders`.

## 2. Commands (`apps/aroh/src-tauri/src/commands/calendar.rs`)

```rust
calendar_list_actions(from: String, to: String) -> Vec<ActionView>
calendar_set_status(id: String, status: String) -> ActionView
calendar_move_action(id: String, start_at: String, end_at: String) -> ActionView
calendar_get_reminder_settings() -> ReminderSettings
calendar_set_reminder_settings(s: ReminderSettings) -> ReminderSettings
```

- `ActionView` = the `actions` row plus `goal_title` and `goal_color_index` (0..8, stable hash of `goal_id`), so the UI makes one call per range.
- `calendar_list_actions` returns actions whose `[start_at, end_at)` overlaps `[from, to)`, not deleted, ordered by `start_at, seq`. The UI computes `from`/`to` from local midnight in the device timezone, so a DST day is 23 or 25 hours and still correct.
- Inputs are validated at the command boundary: ids are uuids, timestamps parse as RFC 3339, status is from the allowed list. Bad input returns an error string, never a panic.
- `pub fn sync_reminders(app: &AppHandle)` is public in the crate. Any pod that changes actions (goals plan approve, replan apply/undo, sync pull) calls it once after its write. That is the one rule other pods need.

## 3. Reminders

### Pure core: `crates/aroh-reminders`

No Tauri, no DB. Depends on `chrono` only, so its tests build fast in a small Kiri container.

```rust
pub fn plan_reminders(actions: &[ActionSlot], settings: &ReminderSettings, now: DateTime<Utc>, horizon: Duration, cap: usize) -> Vec<Reminder>
pub fn reminder_id(action_id: &str, kind: ReminderKind) -> i32
```

- `ReminderKind`: `Soon` (start minus lead), `Start` (at `start_at`), `Nudge` (end plus grace, "Did you finish X?").
- Only `planned` actions get reminders. Fire times at or before `now` are dropped. Lead `0` drops `Soon`. If `Soon` and `Start` coincide only `Start` is kept.
- Window: fire times within `now + horizon` (48 h), sorted, cut at `cap` (60, under the iOS limit of 64 pending notifications). The window rolls forward each time `sync_reminders` runs (app start, resume, every write, and a once-an-hour tick while running).
- `reminder_id` is a stable 31-bit hash of action id and kind, so a reschedule replaces rather than duplicates.

Critical tests (the only tests this pod writes, TDD, in Docker):

1. Lead time: action at 09:00Z, lead 10 gives `Soon` 08:50Z and `Start` 09:00Z. Lead 0 gives only `Start`.
2. Past times: `now` between `Soon` and `Start` keeps only `Start` and `Nudge`.
3. Timezone: an action at 09:00 local `Europe/London` on the DST change day, converted to UTC by the caller, fires at the right UTC instant. `local_day_range("2026-03-29", tz)` is 23 h long (test helper lives in the crate as `local_day_bounds`).
4. Reschedule after move: plan, move the action 2 h later, plan again. Same ids, new times, no reminder at the old times.
5. Status: `done`, `skipped`, `missed` actions get no reminders. Moving a `missed` action back to `planned` brings them back.
6. Cap and horizon: 100 actions give 60 reminders, all inside 48 h, earliest first.

### Delivery: `sync_reminders`

1. Load planned actions with `end_at + grace` in `[now, now + 48 h]` and the settings.
2. `plan_reminders(...)`.
3. Cancel all pending Aroh notifications, then schedule the new list.

Per platform:

| Platform | How it fires while the app is closed |
|---|---|
| iOS (first target) | `tauri-plugin-notification` with `Schedule::At { date, allow_while_idle: true }`. The OS delivers it with the app closed. |
| Android | Same as iOS. |
| macOS desktop (secondary) | The plugin's desktop backend has no `schedule` support (checked in `plugins-workspace/v2/plugins/notification/src/desktop.rs`). Step 1: an in-process tokio timer fires the next reminder with the plugin's `show()` while the app runs, and closing the window hides the app instead of quitting it. Step 2: in a bundled, signed build, schedule through `UNUserNotificationCenter` with a calendar trigger (`objc2-user-notifications`), so the OS fires it with the app quit. `tauri dev` is unbundled, so it stays on step 1. |

Permission is asked the first time an action is created or the user opens Today with actions, not on first launch.

Notification text: `Soon`: "In 10 min: {title}". `Start`: "Now: {title}". `Nudge`: "Did you finish {title}?". Body is the goal title. Max 120 chars. Tapping opens `/today?action={id}`.

### Missed hook (agreed with pod-replan, msg 1896)

- Calendar never sets `status = 'missed'`. Replan owns missed detection.
- Shared threshold: `reminders.missed_grace_min` (default 15). An action is missed when `status = 'planned'` and `end_at + grace < now`. The `Nudge` fires at that same instant.
- Pure side: `aroh_replan::detect_missed(actions: &[Action], now, grace_min) -> Vec<String>`. DB side: `commands::replan::check_missed(app: &AppHandle) -> Result<Vec<String>, String>`. It marks actions missed, runs the replan, then calls `commands::calendar::sync_reminders(&app)` once.
- Calendar calls `check_missed` on app start, on resume, and when a nudge fires in-process. No `aroh://action-overdue` event.
- A hand-moved missed action goes back to `planned`; replan needs no signal for it.

## 4. Screens

Visual direction: "calm instrument" from `context/research/front_end_design.md`. Warm off-white and graphite themes from tokens, one accent for the primary action and the now line, 8 goal hues, system font with tabular numerals, state shown by fill and border, never colour alone. 44 px touch targets. 150 to 200 ms transitions, none with `prefers-reduced-motion`. Built with the `design-taste-frontend` app-screen rules.

Phone first (owner, msg 1949): the iPhone day view is the main design target; the desktop week view comes second.

### `/today`

```
 Thursday 20 November                      3 of 5 done
 [=========-----------]
 <Slot name="today.top" />
 NOW  09:00 to 09:45      Course: Rust ownership       [ Done ] [ Skip ]
 ---- now line ----
 10:30 to 11:00  Read chapter 4            Goal B      [ Done ] [ Skip ]
 18:00 to 18:30  Run 3 km                  Goal C      [ Done ] [ Skip ]
 Done (2)  collapsed list, tap to undo
```

- One list in time order across all goals, from `calendar_list_actions(local midnight, next local midnight)`.
- The top card is the current action, or the next one if none is running.
- Progress = done over (all minus skipped) for the day, shown as a count and a thin bar.
- Done and Skip are one tap, optimistic, with a 5 s undo toast that calls `calendar_set_status(id, 'planned')`.
- `today.top` slot renders from pod-plugins' `<Slot>`. Until it lands, the slot is an empty element.
- Empty day: one line, "Nothing planned today", and a link to New Goal.
- Refreshes on focus, on the `aroh://actions-changed` event, and each minute for the now line.

### `/calendar`

- Phone (under 600 px, main view): day view with a horizontal swipe to the previous or next day (pointer events, 60 px threshold, no library), plus a date strip of the week at the top. Safe-area insets respected.
- Desktop (840 px and up, second): week view by default, a Day/Week toggle, prev/next/today buttons, keys `D`, `W`, `T`, arrow keys.
- Medium widths use the day view with the date strip.
- One `TimeGrid.svelte` component draws N day columns (1 or 7), hours 00 to 24, scrolled to 07:00 or the now line, with absolute-positioned `ActionBlock`s. Overlapping blocks split the column width. No calendar library: the grid is small and the replan ghost overlay comes later, which libraries handle badly.
- Tap a block to open the action sheet (bottom sheet on phone, right panel on desktop): title, goal, time, notes, links from notes, Done, Skip, and Move (date input plus start and end time inputs in local time, saved through `calendar_move_action`). No drag in the MVP.

### Shared front-end files

`apps/aroh/src/lib/calendar/`: `api.ts` (typed `invoke` wrappers), `time.ts` (local day bounds, formatting), `TimeGrid.svelte`, `ActionBlock.svelte`, `ActionSheet.svelte`, `ActionRow.svelte`.

## 5. Out of scope for this pod

Month view, drag to move, notification action buttons (Done/Snooze from the banner), quiet hours, ordinary calendar events, OS calendar sync. Each is a later slice.

## 6. Acceptance

- `crates/aroh-reminders` tests pass in a small container run with Kiri (`/usr/local/bin/kiri build`, `kiri run --rm`), using foundation's `Dockerfile.test`.
- In the iOS simulator with `tauri ios dev`: create a test action starting in 2 minutes, see it on Today and Calendar, mark it done, undo, and get a notification at its start.

## 7. Needs from other pods

- foundation: `aroh-core` structs, DB access in commands, HLC helper, placeholder `/today` and `/calendar` routes (this pod overwrites them), and `aroh-reminders` joins the workspace through `crates/*`. This pod adds `pub mod calendar;` and its handlers to `lib.rs` per `AGENTS.md`.
- This pod owns the `tauri-plugin-notification` setup (Cargo, `tauri.conf`, capability) and close-to-hide on macOS; asks @pod-foundation only if the shell needs another change.
- plugins: `<Slot name="today.top" />`.
- replan: `commands::replan::check_missed` and the missed hook above.
