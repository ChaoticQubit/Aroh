//! Calendar commands: list actions by range, set status, move, reminder settings, and local reminder delivery.

use super::with_core;
use crate::{AppState, Core};
use aroh_core::models::{Action, ActionStatus, Goal};
use aroh_core::{repo, Error};
use aroh_reminders::{plan_reminders, ActionSlot, Reminder, ReminderKind, ReminderSettings};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
#[cfg(mobile)]
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_notification::NotificationExt;

const HORIZON_HOURS: i64 = 48;
const REMINDER_CAP: usize = 60;
const MAX_ACTION_HOURS: i64 = 24;
const TEXT_MAX_CHARS: usize = 120;
const LEAD_MAX_MIN: u32 = 120;
const GRACE_MAX_MIN: u32 = 240;
const KEY_ENABLED: &str = "reminders.enabled";
const KEY_LEAD: &str = "reminders.lead_min";
const KEY_GRACE: &str = "reminders.missed_grace_min";
const DEFAULT_SETTINGS: ReminderSettingsDto = ReminderSettingsDto { enabled: true, lead_min: 10, missed_grace_min: 15 };

/// One action row plus its goal title and a stable colour index in 0..8.
#[derive(Debug, Clone, Serialize)]
pub struct ActionView {
    #[serde(flatten)]
    pub action: Action,
    pub goal_title: Option<String>,
    pub goal_color_index: u8,
}

/// Reminder settings as the UI reads and writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderSettingsDto {
    pub enabled: bool,
    pub lead_min: u32,
    pub missed_grace_min: u32,
}

/// Serialises reminder delivery: holds the latest generation so stale schedules and desktop timers stop.
#[derive(Default)]
pub struct ReminderState {
    generation: Arc<AtomicU64>,
    #[cfg(mobile)]
    deliver: Mutex<()>,
}

fn invalid(what: &str) -> String {
    format!("invalid {what}")
}

fn parse_time(text: &str, what: &str) -> Result<DateTime<Utc>, String> {
    if text.len() > 64 {
        return Err(invalid(what));
    }
    DateTime::parse_from_rfc3339(text).map(|t| t.with_timezone(&Utc)).map_err(|_| invalid(what))
}

fn format_time(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn check_id(id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(id).map(|_| ()).map_err(|_| invalid("id"))
}

/// Stable colour slot for a goal: FNV-1a 32 of the goal id, modulo 8.
fn goal_color_index(goal_id: &str) -> u8 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in goal_id.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    let index = (hash % 8) as u8;
    assert!(index < 8);
    index
}

fn goal_titles(core: &Core) -> aroh_core::Result<HashMap<String, String>> {
    Ok(repo::list_goals(&core.conn)?.into_iter().map(|g: Goal| (g.id, g.title)).collect())
}

fn view(action: Action, titles: &HashMap<String, String>) -> ActionView {
    let goal_color_index = goal_color_index(&action.goal_id);
    let goal_title = titles.get(&action.goal_id).cloned();
    ActionView { action, goal_title, goal_color_index }
}

fn load_action(core: &Core, id: &str) -> aroh_core::Result<Action> {
    repo::list_actions(&core.conn, None)?
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| Error::NotFound(id.to_string()))
}

fn overlaps(action: &Action, from: DateTime<Utc>, to: DateTime<Utc>) -> bool {
    match (parse_time(&action.start_at, "start_at"), parse_time(&action.end_at, "end_at")) {
        (Ok(start), Ok(end)) => start < to && end > from,
        _ => false,
    }
}

/// Lists live actions overlapping `[from, to)` (RFC 3339), ordered by start then seq.
#[tauri::command]
pub fn calendar_list_actions(state: State<'_, AppState>, from: String, to: String) -> Result<Vec<ActionView>, String> {
    let from = parse_time(&from, "from")?;
    let to = parse_time(&to, "to")?;
    if to <= from || to - from > Duration::days(62) {
        return Err(invalid("range"));
    }
    with_core(&state, |c| {
        let titles = goal_titles(c)?;
        let actions = repo::list_actions(&c.conn, None)?;
        Ok(actions.into_iter().filter(|a| overlaps(a, from, to)).map(|a| view(a, &titles)).collect())
    })
}

/// Sets an action to `done`, `skipped` or back to `planned`. `done_at` is set only for `done`.
#[tauri::command]
pub fn calendar_set_status(app: AppHandle, state: State<'_, AppState>, id: String, status: String) -> Result<ActionView, String> {
    check_id(&id)?;
    let status = match status.as_str() {
        "done" => ActionStatus::Done,
        "skipped" => ActionStatus::Skipped,
        "planned" => ActionStatus::Planned,
        _ => return Err(invalid("status")),
    };
    let row = with_core(&state, |c| {
        let mut action = load_action(c, &id)?;
        action.status = status;
        action.done_at = (status == ActionStatus::Done).then(|| format_time(Utc::now()));
        repo::update_action(&c.conn, &mut c.clock, &mut action)?;
        let titles = goal_titles(c)?;
        Ok(view(action, &titles))
    })?;
    after_write(&app);
    Ok(row)
}

/// Moves an action to new RFC 3339 times. End must follow start by at most 24 h; a missed action becomes planned.
#[tauri::command]
pub fn calendar_move_action(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    start_at: String,
    end_at: String,
) -> Result<ActionView, String> {
    check_id(&id)?;
    let start = parse_time(&start_at, "start_at")?;
    let end = parse_time(&end_at, "end_at")?;
    if end <= start || end - start > Duration::hours(MAX_ACTION_HOURS) {
        return Err(invalid("time range"));
    }
    let row = with_core(&state, |c| {
        let mut action = load_action(c, &id)?;
        action.start_at = format_time(start);
        action.end_at = format_time(end);
        if action.status == ActionStatus::Missed {
            action.status = ActionStatus::Planned;
        }
        repo::update_action(&c.conn, &mut c.clock, &mut action)?;
        let titles = goal_titles(c)?;
        Ok(view(action, &titles))
    })?;
    after_write(&app);
    Ok(row)
}

fn read_setting(core: &Core, key: &str) -> aroh_core::Result<Option<String>> {
    Ok(core
        .conn
        .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0))
        .optional()?)
}

fn write_setting(core: &mut Core, key: &str, value: &str) -> aroh_core::Result<()> {
    let stamp = core.clock.tick();
    core.conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, stamp],
    )?;
    Ok(())
}

fn load_settings(core: &Core) -> aroh_core::Result<ReminderSettingsDto> {
    let d = DEFAULT_SETTINGS;
    let enabled = read_setting(core, KEY_ENABLED)?.map_or(d.enabled, |v| v != "false");
    let number = |key: &str, default: u32, max: u32| -> aroh_core::Result<u32> {
        Ok(read_setting(core, key)?.and_then(|v| v.parse::<u32>().ok()).filter(|n| *n <= max).unwrap_or(default))
    };
    Ok(ReminderSettingsDto {
        enabled,
        lead_min: number(KEY_LEAD, d.lead_min, LEAD_MAX_MIN)?,
        missed_grace_min: number(KEY_GRACE, d.missed_grace_min, GRACE_MAX_MIN)?,
    })
}

/// Reads the reminder settings, falling back to defaults for missing or out-of-range values.
#[tauri::command]
pub fn calendar_get_reminder_settings(state: State<'_, AppState>) -> Result<ReminderSettingsDto, String> {
    with_core(&state, |c| load_settings(c))
}

/// Saves the reminder settings after bounds checks (lead 0..=120, grace 0..=240) and returns them.
#[tauri::command]
pub fn calendar_set_reminder_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    s: ReminderSettingsDto,
) -> Result<ReminderSettingsDto, String> {
    if s.lead_min > LEAD_MAX_MIN {
        return Err(invalid("lead_min"));
    }
    if s.missed_grace_min > GRACE_MAX_MIN {
        return Err(invalid("missed_grace_min"));
    }
    let stored = with_core(&state, |c| {
        write_setting(c, KEY_ENABLED, if s.enabled { "true" } else { "false" })?;
        write_setting(c, KEY_LEAD, &s.lead_min.to_string())?;
        write_setting(c, KEY_GRACE, &s.missed_grace_min.to_string())?;
        load_settings(c)
    })?;
    after_write(&app);
    Ok(stored)
}

/// Asks the OS for notification permission if it has not been decided yet. Returns whether it is granted.
#[tauri::command]
pub async fn calendar_ensure_notification_permission(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_notification::PermissionState;
    let notifications = app.notification();
    let mut permission = notifications.permission_state().map_err(|e| e.to_string())?;
    if permission != PermissionState::Granted && permission != PermissionState::Denied {
        permission = notifications.request_permission().map_err(|e| e.to_string())?;
    }
    let granted = permission == PermissionState::Granted;
    if granted {
        sync_reminders(&app)?;
    }
    Ok(granted)
}

fn after_write(app: &AppHandle) {
    if let Err(e) = sync_reminders(app) {
        eprintln!("aroh: sync_reminders failed: {e}");
    }
}

struct Planned {
    reminder: Reminder,
    title: String,
    body: String,
}

fn truncate(text: String) -> String {
    if text.chars().count() <= TEXT_MAX_CHARS {
        return text;
    }
    let mut cut: String = text.chars().take(TEXT_MAX_CHARS - 1).collect();
    cut.push('…');
    cut
}

fn notification_text(kind: ReminderKind, title: &str, lead_min: u32) -> String {
    truncate(match kind {
        ReminderKind::Soon => format!("In {lead_min} min: {title}"),
        ReminderKind::Start => format!("Now: {title}"),
        ReminderKind::Nudge => format!("Did you finish {title}?"),
    })
}

fn plan_for(core: &Core, now: DateTime<Utc>) -> aroh_core::Result<Vec<Planned>> {
    let s = load_settings(core)?;
    let settings = ReminderSettings { enabled: s.enabled, lead_min: s.lead_min, missed_grace_min: s.missed_grace_min };
    let titles = goal_titles(core)?;
    let window_from = now - Duration::minutes(i64::from(s.missed_grace_min));
    let window_to = now + Duration::hours(HORIZON_HOURS) + Duration::minutes(i64::from(s.lead_min));
    let actions: Vec<Action> = repo::list_actions(&core.conn, None)?
        .into_iter()
        .filter(|a| a.status == ActionStatus::Planned && overlaps(a, window_from, window_to))
        .collect();
    let slots: Vec<ActionSlot> = actions
        .iter()
        .filter_map(|a| {
            let start_at = parse_time(&a.start_at, "start_at").ok()?;
            let end_at = parse_time(&a.end_at, "end_at").ok()?;
            Some(ActionSlot { id: a.id.clone(), start_at, end_at, status: aroh_reminders::ActionStatus::Planned })
        })
        .collect();
    let by_id: HashMap<&str, &Action> = actions.iter().map(|a| (a.id.as_str(), a)).collect();
    let reminders = plan_reminders(&slots, &settings, now, Duration::hours(HORIZON_HOURS), REMINDER_CAP);
    assert!(reminders.len() <= REMINDER_CAP);
    Ok(reminders
        .into_iter()
        .map(|reminder| {
            let action = by_id[reminder.action_id.as_str()];
            let title = notification_text(reminder.kind, &action.title, s.lead_min);
            let body = truncate(titles.get(&action.goal_id).cloned().unwrap_or_default());
            Planned { reminder, title, body }
        })
        .collect())
}

/// Every reminder id any action row (live or deleted) could own, so stale schedules are cancelled
/// without relying on the plugin's pending list.
fn all_reminder_ids(core: &Core) -> aroh_core::Result<Vec<i32>> {
    let mut stmt = core.conn.prepare("SELECT id FROM actions")?;
    let ids = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let kinds = [ReminderKind::Soon, ReminderKind::Start, ReminderKind::Nudge];
    Ok(ids.iter().flat_map(|id| kinds.map(|k| aroh_reminders::reminder_id(id, k))).collect())
}

/// Replans local notifications for planned actions in the next 48 h (cap 60) and hands them to the OS,
/// then emits `aroh://actions-changed` so open screens reload.
/// Call once after any write to actions or reminder settings, and at app start or resume.
/// Returns an error only when the database read fails; delivery runs in the background.
pub fn sync_reminders(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Emitter;
    if let Err(e) = app.emit("aroh://actions-changed", ()) {
        eprintln!("aroh: actions-changed not emitted: {e}");
    }
    let state = app.state::<AppState>();
    let (planned, cancel_ids) = with_core(&state, |c| Ok((plan_for(c, Utc::now())?, all_reminder_ids(c)?)))?;
    let reminders = app.state::<ReminderState>();
    let generation = reminders.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || deliver(&app, generation, planned, cancel_ids));
    Ok(())
}

#[cfg(mobile)]
fn deliver(app: &AppHandle, generation: u64, planned: Vec<Planned>, cancel_ids: Vec<i32>) {
    use tauri_plugin_notification::Schedule;
    let state = app.state::<ReminderState>();
    let _guard = state.deliver.lock().unwrap_or_else(|e| e.into_inner());
    if state.generation.load(Ordering::SeqCst) != generation {
        return;
    }
    let notifications = app.notification();
    assert!(cancel_ids.iter().all(|id| (0..aroh_reminders::REMINDER_ID_LIMIT as i32).contains(id)));
    if !cancel_ids.is_empty() {
        if let Err(e) = notifications.cancel(cancel_ids) {
            eprintln!("aroh: cancel reminders failed: {e}");
        }
    }
    for p in planned {
        let Some(date) = plugin_fire_date(p.reminder.fire_at) else { continue };
        let result = notifications
            .builder()
            .id(p.reminder.id)
            .title(p.title)
            .body(p.body)
            .extra("action_id", &p.reminder.action_id)
            .schedule(Schedule::At { date, repeating: false, allow_while_idle: true })
            .show();
        if let Err(e) = result {
            eprintln!("aroh: schedule reminder failed: {e}");
        }
    }
}

/// The iOS plugin (2.7) parses the date as local wall-clock time and ignores the trailing `Z`,
/// so pass the instant shifted by the device's UTC offset at that instant.
/// Returns `None` if the shifted time is out of range.
#[cfg(mobile)]
fn plugin_fire_date(fire_at: DateTime<Utc>) -> Option<time::OffsetDateTime> {
    use chrono::Offset;
    let offset = fire_at.with_timezone(&chrono::Local).offset().fix().local_minus_utc();
    let date = time::OffsetDateTime::from_unix_timestamp(fire_at.timestamp() + i64::from(offset)).ok()?;
    assert_eq!(date.offset(), time::UtcOffset::UTC);
    Some(date)
}

#[cfg(desktop)]
fn deliver(app: &AppHandle, generation: u64, planned: Vec<Planned>, _cancel_ids: Vec<i32>) {
    let state = app.state::<ReminderState>();
    let current = Arc::clone(&state.generation);
    drop(state);
    for p in planned {
        loop {
            if current.load(Ordering::SeqCst) != generation {
                return;
            }
            let wait = p.reminder.fire_at - Utc::now();
            if wait <= Duration::zero() {
                break;
            }
            let step = wait.min(Duration::seconds(30)).to_std().unwrap_or_default();
            std::thread::sleep(step);
        }
        if let Err(e) = app.notification().builder().title(p.title).body(p.body).show() {
            eprintln!("aroh: show reminder failed: {e}");
        }
    }
}
