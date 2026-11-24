//! Local notification planning (Q22 offline fallback): loads the owner's rows and runs the
//! shared planner in `aroh_domain::remind`, so the server pushes the same ids.

use crate::Result;
pub use aroh_domain::remind::{scheduled_until, Planned, PlannedKind, CAP, HORIZON_HOURS};
use aroh_domain::rows::{Action, Goal, Tracker};
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

/// Whether this device may schedule local reminders: always for a store that never signed
/// in, and for a store adopted by an account only while `signed_in`, so a signed-out device
/// does not keep showing that account's rows. Errors only when the store read fails.
pub fn reminders_allowed(conn: &Connection, signed_in: bool) -> Result<bool> {
    Ok(signed_in || crate::local::account(conn)?.is_none())
}
