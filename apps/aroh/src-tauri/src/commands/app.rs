//! App screen commands: thin wrappers over `aroh_app` that parse input at the trust boundary,
//! run one engine call under the core lock, then reschedule reminders after every write.

use super::with_core;
use crate::{notify, AppState};
use aroh_app::snapshot::Snapshot;
use aroh_app::tick::Tick;
use aroh_app::write::{self, Logged};
use aroh_domain::rows::{Action, ActionStatus, Autonomy, Goal, Proposal, ProviderSetting, TrackerEntry};
use chrono::{DateTime, Duration, Utc};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager, State};

const MAX_RANGE_DAYS: i64 = 120;
const ENTRY_DAYS: i64 = 90;

pub(super) fn parse_time(text: &str, what: &str) -> Result<DateTime<Utc>, String> {
    if text.len() > 64 {
        return Err(format!("invalid {what}"));
    }
    DateTime::parse_from_rfc3339(text).map(|t| t.with_timezone(&Utc)).map_err(|_| format!("invalid {what}"))
}

fn parse_enum<T: DeserializeOwned>(text: &str, what: &str) -> Result<T, String> {
    serde_json::from_value(Value::String(text.to_string())).map_err(|_| format!("invalid {what}"))
}

pub(crate) fn after_write(app: &AppHandle) {
    if let Err(e) = notify::sync(app) {
        eprintln!("aroh: reminders not scheduled: {e}");
    }
}

/// Reads every row the screens show: actions overlapping `[from, to)` (RFC 3339, at most
/// 120 days apart) and tracker entries of the last 90 days.
#[tauri::command]
pub fn app_snapshot(state: State<'_, AppState>, from: String, to: String) -> Result<Snapshot, String> {
    let from = parse_time(&from, "from")?;
    let to = parse_time(&to, "to")?;
    if to <= from || to - from > Duration::days(MAX_RANGE_DAYS) {
        return Err("invalid range".into());
    }
    let since = Utc::now() - Duration::days(ENTRY_DAYS);
    with_core(&state, |c| aroh_app::snapshot::snapshot(&c.conn, &c.user_id, from, to, since))
}

/// Runs the start-up tick now, for a changed local date, and reschedules reminders.
#[tauri::command]
pub fn app_tick(app: AppHandle, state: State<'_, AppState>) -> Result<Tick, String> {
    let report = with_core(&state, |c| aroh_app::tick::tick(&c.conn, &mut c.clock, &c.user_id, Utc::now()))?;
    after_write(&app);
    Ok(report)
}

/// Sets an action to `planned`, `done` or `skipped`.
#[tauri::command]
pub fn app_set_action_status(app: AppHandle, state: State<'_, AppState>, id: String, status: String) -> Result<Action, String> {
    let status: ActionStatus = parse_enum(&status, "status")?;
    let action = with_core(&state, |c| write::set_action_status(&c.conn, &mut c.clock, &id, status, Utc::now()))?;
    after_write(&app);
    Ok(action)
}

/// Writes a check-in answer: `data` maps tracker field keys to values.
#[tauri::command]
pub fn app_add_entry(app: AppHandle, state: State<'_, AppState>, tracker_id: String, data: Map<String, Value>) -> Result<TrackerEntry, String> {
    let entry = with_core(&state, |c| write::add_entry(&c.conn, &mut c.clock, &c.user_id, &tracker_id, data, Utc::now()))?;
    after_write(&app);
    Ok(entry)
}

/// Logs free text as a tracker entry, or queues it for the engine.
#[tauri::command]
pub fn app_log_text(app: AppHandle, state: State<'_, AppState>, text: String) -> Result<Logged, String> {
    let logged = with_core(&state, |c| write::log_text(&c.conn, &mut c.clock, &c.user_id, &text, Utc::now()))?;
    after_write(&app);
    Ok(logged)
}

/// Applies (`apply` true) or rejects an open proposal.
#[tauri::command]
pub fn app_decide_proposal(app: AppHandle, state: State<'_, AppState>, id: String, apply: bool) -> Result<Proposal, String> {
    let proposal = with_core(&state, |c| write::decide_proposal(&c.conn, &mut c.clock, &id, apply))?;
    after_write(&app);
    Ok(proposal)
}

/// Confirms (`confirm` true) or dismisses a suggested preference.
#[tauri::command]
pub fn app_decide_preference(app: AppHandle, state: State<'_, AppState>, id: String, confirm: bool) -> Result<(), String> {
    with_core(&state, |c| write::decide_preference(&c.conn, &mut c.clock, &id, confirm))?;
    after_write(&app);
    Ok(())
}

/// Sets a goal's autonomy level: `ask`, `balanced` or `auto`.
#[tauri::command]
pub fn app_set_autonomy(app: AppHandle, state: State<'_, AppState>, goal_id: String, autonomy: String) -> Result<Goal, String> {
    let autonomy: Autonomy = parse_enum(&autonomy, "autonomy")?;
    let goal = with_core(&state, |c| write::set_autonomy(&c.conn, &mut c.clock, &goal_id, autonomy))?;
    after_write(&app);
    Ok(goal)
}

/// Makes a vendor and model the default provider. Takes no key: keys go only to the engine.
#[tauri::command]
pub fn app_save_provider(app: AppHandle, state: State<'_, AppState>, vendor: String, model: String) -> Result<ProviderSetting, String> {
    let provider = with_core(&state, |c| write::save_provider(&c.conn, &mut c.clock, &c.user_id, &vendor, &model))?;
    after_write(&app);
    Ok(provider)
}

/// Sets the monthly budget over all goals in US dollars; null or 0 removes it.
#[tauri::command]
pub fn app_save_budget(app: AppHandle, state: State<'_, AppState>, usd: Option<f64>) -> Result<(), String> {
    with_core(&state, |c| write::save_budget(&c.conn, &mut c.clock, &c.user_id, usd))?;
    after_write(&app);
    Ok(())
}

/// Stores one device setting from the whitelist in `aroh_app::local::SETTINGS`.
#[tauri::command]
pub fn app_set_setting(app: AppHandle, state: State<'_, AppState>, name: String, value: String) -> Result<(), String> {
    with_core(&state, |c| aroh_app::local::set_setting(&c.conn, &name, &value))?;
    after_write(&app);
    Ok(())
}

/// Asks the OS for notification permission if not decided yet, then reschedules reminders.
/// Returns whether notifications are allowed.
#[tauri::command]
pub async fn app_notification_permission(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_notification::{NotificationExt, PermissionState};
    let notifications = app.notification();
    let mut permission = notifications.permission_state().map_err(|e| e.to_string())?;
    if permission != PermissionState::Granted && permission != PermissionState::Denied {
        permission = notifications.request_permission().map_err(|e| e.to_string())?;
    }
    let granted = permission == PermissionState::Granted;
    if granted {
        after_write(&app);
    }
    Ok(granted)
}

/// Debug builds only: loads the six-pack sample when there is no goal, then ticks.
/// Returns whether the sample was loaded. Release builds refuse it.
#[tauri::command]
pub fn app_dev_seed(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    if !cfg!(debug_assertions) {
        return Err("debug builds only".into());
    }
    let loaded = with_core(&state, |c| {
        let loaded = aroh_app::sample::seed(&c.conn, &mut c.clock, &c.user_id, Utc::now(), crate::zone())?;
        aroh_app::tick::tick(&c.conn, &mut c.clock, &c.user_id, Utc::now())?;
        Ok(loaded)
    })?;
    after_write(&app);
    Ok(loaded)
}

/// Debug builds only: appends `aroh-dev: <mark>` to `dev-marks.log` in the app data folder so
/// a driver script can tell the host which step the screen shows (the e2e screenshots).
/// At most 80 visible ASCII characters.
#[tauri::command]
pub fn app_dev_mark(app: AppHandle, mark: String) -> Result<(), String> {
    use std::io::Write;
    if !cfg!(debug_assertions) {
        return Err("debug builds only".into());
    }
    if mark.len() > 80 || !mark.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        return Err("bad mark".into());
    }
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("dev-marks.log");
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| e.to_string())?;
    writeln!(file, "aroh-dev: {mark}").map_err(|e| e.to_string())
}
