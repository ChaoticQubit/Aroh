//! Reads the live rows of the local user and builds the Dashboard figures (Q69).

use crate::Result;
use aroh_domain::dashboard::{self, Dashboard, Rows};
use aroh_domain::rows::{Action, ActionSeries, Goal, Tracker, TrackerEntry};
use aroh_store::repo;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use rusqlite::Connection;
use std::collections::BTreeMap;

/// The dashboard of `user_id` at `now` in `zone`, over every live goal, action, series,
/// tracker and entry, so figures cover the whole history, not the snapshot window.
/// `targets` maps tracker id to field targets from the active plugin versions.
pub fn read(conn: &Connection, user_id: &str, targets: &BTreeMap<String, BTreeMap<String, f64>>, now: DateTime<Utc>, zone: Tz) -> Result<Dashboard> {
    let mine = [("user_id", user_id)];
    let goals: Vec<Goal> = repo::list(conn, &mine)?;
    let actions: Vec<Action> = repo::list(conn, &mine)?;
    let series: Vec<ActionSeries> = repo::list(conn, &mine)?;
    let trackers: Vec<Tracker> = repo::list(conn, &mine)?;
    let entries: Vec<TrackerEntry> = repo::list(conn, &mine)?;
    let rows = Rows { goals: &goals, actions: &actions, series: &series, trackers: &trackers, entries: &entries, targets };
    Ok(dashboard::dashboard(&rows, now, zone))
}
