//! Device-only location tables (Q56): raw visits, unnamed place candidates, learned
//! patterns before approval and the nudge log. None is in `SYNCED`, so sync never reads them.

use crate::Result;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::Connection;

/// Tables that exist only on this device and never sync.
pub const LOCAL_TABLES: [&str; 5] = ["visits", "place_candidates", "pattern_candidates", "nudge_sent", "place_kinds"];

/// Days a raw visit is kept on the phone before it is deleted (owner 2026-11-22).
pub const VISIT_DAYS: i64 = 90;

/// Deletes raw visits that arrived more than [`VISIT_DAYS`] days before `now`.
/// Output: how many visits were deleted.
pub fn prune_visits(conn: &Connection, now: DateTime<Utc>) -> Result<usize> {
    let cutoff = (now - Duration::days(VISIT_DAYS)).to_rfc3339_opts(SecondsFormat::Secs, true);
    Ok(conn.execute("DELETE FROM visits WHERE arrived_at < ?1", [cutoff])?)
}
