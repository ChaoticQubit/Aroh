//! Local notification planning (Q22 offline fallback): loads the owner's rows and runs the
//! shared planner in `aroh_domain::remind`, so the server pushes the same ids.

use crate::Result;
pub use aroh_domain::remind::{scheduled_until, Planned, PlannedKind, CAP, HORIZON_HOURS};
use aroh_domain::rows::{Action, Goal, Tracker};
use aroh_reminders::{reminder_id, ReminderKind};
use aroh_store::repo;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use rusqlite::Connection;

/// Plans the notifications of `user_id` after `now` from the local store (local times in
/// `zone`). Output: at most [`CAP`] reminders within [`HORIZON_HOURS`], earliest first.
pub fn plan(conn: &Connection, user_id: &str, now: DateTime<Utc>, zone: Tz) -> Result<Vec<Planned>> {
    let goals = repo::list::<Goal>(conn, &[("user_id", user_id)])?;
    let actions = repo::list::<Action>(conn, &[("user_id", user_id), ("status", "planned")])?;
    let trackers = repo::list::<Tracker>(conn, &[("user_id", user_id)])?;
    Ok(aroh_domain::remind::plan(&goals, &actions, &trackers, now, zone))
}

/// The lowest pull cursor of `actions` and `trackers` for `user_id`: every remote row of
/// either table at or below it is local and so in the local schedule. Reported to the
/// engine with the push token so it pushes only reminders this device never saw.
pub fn synced_seq(conn: &Connection, user_id: &str) -> Result<i64> {
    let actions = aroh_store::sync::pulled_seq(conn, user_id, &aroh_domain::rows::ACTIONS)?;
    let trackers = aroh_store::sync::pulled_seq(conn, user_id, &aroh_domain::rows::TRACKERS)?;
    Ok(actions.min(trackers))
}

/// Every notification id any action or tracker row (live or deleted) could own, so a
/// reschedule cancels stale ones without reading the OS pending list.
pub fn all_ids(conn: &Connection) -> Result<Vec<i32>> {
    let mut ids = Vec::new();
    let kinds = [ReminderKind::Soon, ReminderKind::Start, ReminderKind::Nudge];
    for table in ["actions", "trackers"] {
        let mut stmt = conn.prepare(&format!("SELECT id FROM {table}"))?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        ids.extend(rows.iter().flat_map(|id| kinds.map(|k| reminder_id(id, k))));
    }
    Ok(ids)
}
