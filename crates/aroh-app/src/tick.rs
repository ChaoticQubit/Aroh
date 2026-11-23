//! The start-up tick: runs at app start, on resume and when the local date changes.

use crate::Result;
use aroh_domain::hlc::Hlc;
use aroh_domain::recur::DEFAULT_WINDOW_DAYS;
use aroh_domain::rows::{Action, ActionSeries, ActionStatus};
use aroh_store::repo;
use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;

pub use aroh_domain::time::MISSED_GRACE_MIN;

/// What one tick changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Tick {
    pub seeded: usize,
    pub materialized: usize,
    pub missed: usize,
    pub moved: usize,
}

/// Seeds the categories of `user_id` (insert-or-ignore), settles occurrences pulled in an
/// old zone ([`crate::travel::settle`]), materializes every live series for `[now, now +
/// 14 days)`, keeps actions off Busy days from today on (local in `zone`), and marks planned
/// actions that ended more than [`MISSED_GRACE_MIN`] minutes before `now` as missed.
/// Running it twice changes nothing. Output: the counts changed.
pub fn tick(conn: &Connection, clock: &mut Hlc, user_id: &str, now: DateTime<Utc>, zone: chrono_tz::Tz) -> Result<Tick> {
    let mut report = Tick::default();
    for category in aroh_domain::seed::categories(user_id) {
        if repo::insert_derived(conn, &category)? {
            report.seeded += 1;
        }
    }
    crate::travel::settle(conn, clock, user_id, now)?;
    for series in repo::list::<ActionSeries>(conn, &[("user_id", user_id)])? {
        report.materialized += repo::materialize(conn, &series.id, now, DEFAULT_WINDOW_DAYS)?.len();
    }
    let today = now.with_timezone(&zone).date_naive();
    report.moved = crate::write::keep_off_busy_days(conn, clock, user_id, today, zone)?.0.len();
    let cutoff = now - Duration::minutes(MISSED_GRACE_MIN);
    for mut action in repo::list::<Action>(conn, &[("user_id", user_id), ("status", "planned")])? {
        if action.end_at < cutoff {
            action.status = ActionStatus::Missed;
            repo::put(conn, clock, &mut action)?;
            report.missed += 1;
        }
    }
    Ok(report)
}
