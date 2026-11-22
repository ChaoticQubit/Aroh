//! Device calendar import: the occurrences the EventKit plugin read are checked at the trust
//! boundary, matched against the synced rules under the core lock, and the unmatched ones
//! go back to the UI for triage. Nothing is written to the device calendar.

use super::with_core;
use crate::{notify, AppState};
use aroh_app::calendar::Import;
use aroh_domain::calendar::DeviceEvent;
use aroh_domain::recur::DEFAULT_WINDOW_DAYS;
use chrono::{Duration, Utc};
use tauri::{AppHandle, State};

const MAX_EVENTS: usize = 5000;
const PAST_DAYS: i64 = 1;

/// Imports the device occurrences of the last day and the next 14 days: kept events become
/// busy blocks, stale blocks are removed, and the unmatched events (one per series) are
/// returned. Takes at most 5000 occurrences; reloads the screens after a change.
#[tauri::command]
pub fn app_calendar_import(app: AppHandle, state: State<'_, AppState>, events: Vec<DeviceEvent>) -> Result<Import, String> {
    if events.len() > MAX_EVENTS {
        return Err(format!("at most {MAX_EVENTS} calendar events"));
    }
    let now = Utc::now();
    let (from, to) = (now - Duration::days(PAST_DAYS), now + Duration::days(i64::from(DEFAULT_WINDOW_DAYS)));
    let report = with_core(&state, |c| aroh_app::calendar::import(&c.conn, &mut c.clock, &c.user_id, &events, from, to))?;
    if report.kept > 0 || report.removed > 0 {
        if let Err(e) = notify::sync(&app) {
            eprintln!("aroh: reminders not scheduled: {e}");
        }
    }
    Ok(report)
}
