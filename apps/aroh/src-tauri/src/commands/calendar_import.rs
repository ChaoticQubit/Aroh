//! Device calendar import: the occurrences the EventKit plugin read are checked at the trust
//! boundary, matched against the synced rules under the core lock, and the unmatched ones
//! go back to the UI for triage. Nothing is written to the device calendar.

use super::app::after_write;
use super::with_core;
use crate::{notify, AppState};
use aroh_app::calendar::Import;
use aroh_app::write::DaySet;
use aroh_domain::calendar::DeviceEvent;
use aroh_domain::rows::DayKind;
use aroh_domain::recur::DEFAULT_WINDOW_DAYS;
use chrono::{Duration, Utc};
use tauri::{AppHandle, State};

const MAX_EVENTS: usize = 5000;
const PAST_DAYS: i64 = 1;

/// Imports the device occurrences of the last day and the next 14 days: kept events become
/// busy blocks, stale blocks are removed, actions are kept off Busy days, and the unmatched
/// events (one per series) are returned. Takes at most 5000 occurrences; reschedules
/// reminders after a change.
#[tauri::command(async)]
pub fn app_calendar_import(app: AppHandle, state: State<'_, AppState>, events: Vec<DeviceEvent>) -> Result<Import, String> {
    if events.len() > MAX_EVENTS {
        return Err(format!("at most {MAX_EVENTS} calendar events"));
    }
    let now = Utc::now();
    let (from, to) = (now - Duration::days(PAST_DAYS), now + Duration::days(i64::from(DEFAULT_WINDOW_DAYS)));
    let today = now.with_timezone(&crate::zone()).date_naive();
    let (report, moved) = with_core(&state, |c| {
        let report = aroh_app::calendar::import(&c.conn, &mut c.clock, &c.user_id, &events, from, to)?;
        let (moved, _) = aroh_app::write::keep_off_busy_days(&c.conn, &mut c.clock, &c.user_id, today, crate::zone())?;
        Ok((report, moved.len()))
    })?;
    if report.kept > 0 || report.removed > 0 || moved > 0 {
        if let Err(e) = notify::sync(&app) {
            eprintln!("aroh: reminders not scheduled: {e}");
        }
    }
    Ok(report)
}

/// Confirms the kind of the all-day event behind busy block `id`: `reminder`, `busy` or
/// `away`. Writes the event's rule and block kinds and moves planned actions off every Busy
/// day from today on to the next free day. Reschedules reminders and reloads the screens.
#[tauri::command(async)]
pub fn app_set_day_kind(app: AppHandle, state: State<'_, AppState>, id: String, kind: String) -> Result<DaySet, String> {
    let kind: DayKind = serde_json::from_value(serde_json::Value::String(kind)).map_err(|_| "invalid kind".to_string())?;
    let set = with_core(&state, |c| aroh_app::write::set_day_kind(&c.conn, &mut c.clock, &c.user_id, &id, kind, Utc::now().with_timezone(&crate::zone()).date_naive(), crate::zone()))?;
    after_write(&app);
    Ok(set)
}
