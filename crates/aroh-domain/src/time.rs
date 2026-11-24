//! Canonical text form of UTC instants, shared by SQLite storage and the sync wire.

use crate::{invalid, Result};
use chrono::{DateTime, Duration, NaiveTime, SecondsFormat, TimeZone, Utc};
use chrono_tz::Tz;

/// Minutes after its end a planned action at the least waits before it
/// counts as missed; see [`missed_at`] for the full rule.
pub const MISSED_GRACE_MIN: i64 = 15;

/// The instant a still-planned action from `start` to `end` turns missed (Q78):
/// the end of its local day in `zone`, and never sooner than
/// [`MISSED_GRACE_MIN`] after `end`. Before it the action is "Not logged yet".
/// Output: a UTC instant after `end`.
pub fn missed_at(start: DateTime<Utc>, end: DateTime<Utc>, zone: Tz) -> DateTime<Utc> {
    let midnight = (start.with_timezone(&zone).date_naive() + Duration::days(1)).and_time(NaiveTime::MIN);
    let day_end = (0..=2)
        .find_map(|h| zone.from_local_datetime(&(midnight + Duration::hours(h))).earliest())
        .map_or(start + Duration::days(1), |t| t.with_timezone(&Utc));
    let at = day_end.max(end + Duration::minutes(MISSED_GRACE_MIN));
    assert!(at > end, "an action turns missed only after it ends");
    at
}

/// Formats `t` as RFC 3339 UTC with a `Z` suffix and only the fraction digits
/// it needs, the same text serde writes for `DateTime<Utc>`.
/// Output example: `2026-11-21T09:00:00Z`.
pub fn format(t: &DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// Parses any RFC 3339 instant (for example Postgres `+00:00` output) and
/// returns it in the canonical form of [`format`].
/// Errors with `Invalid` on text that is not RFC 3339.
pub fn normalize(text: &str) -> Result<String> {
    let parsed = DateTime::parse_from_rfc3339(text).map_err(|e| invalid("time", e.to_string()))?;
    Ok(format(&parsed.with_timezone(&Utc)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn format_matches_serde_and_normalize_is_stable() {
        let t = Utc.with_ymd_and_hms(2026, 11, 21, 9, 0, 0).unwrap();
        let serde_text = serde_json::to_value(t).unwrap();
        assert_eq!(serde_text, format(&t));
        assert_eq!(format(&t), "2026-11-21T09:00:00Z");
        assert_eq!(normalize("2026-11-21T09:00:00+00:00").unwrap(), "2026-11-21T09:00:00Z");
        assert_eq!(normalize("2026-11-21T10:00:00+01:00").unwrap(), "2026-11-21T09:00:00Z");
        assert_eq!(normalize("2026-11-21T09:00:00.250+00:00").unwrap(), "2026-11-21T09:00:00.250Z");
        let millis = Utc.timestamp_millis_opt(1_763_715_600_250).unwrap();
        assert_eq!(serde_json::to_value(millis).unwrap(), format(&millis));
        assert!(normalize("yesterday").is_err());
    }

    #[test]
    fn missed_at_is_the_end_of_the_local_day_never_inside_the_grace() {
        let berlin: Tz = "Europe/Berlin".parse().unwrap();
        let at = |d, h, m| berlin.with_ymd_and_hms(2026, 11, d, h, m, 0).unwrap().with_timezone(&Utc);
        assert_eq!(missed_at(at(24, 7, 0), at(24, 7, 30), berlin), at(25, 0, 0));
        assert_eq!(missed_at(at(24, 23, 30), at(25, 0, 0), berlin), at(25, 0, 15));
        assert_eq!(missed_at(at(24, 22, 0), at(25, 1, 0), berlin), at(25, 1, 15));
        let new_york: Tz = "America/New_York".parse().unwrap();
        let morning = Utc.with_ymd_and_hms(2026, 11, 24, 12, 0, 0).unwrap();
        assert_eq!(missed_at(morning, morning + Duration::minutes(30), new_york), Utc.with_ymd_and_hms(2026, 11, 25, 5, 0, 0).unwrap());
        let santiago: Tz = "America/Santiago".parse().unwrap();
        let before_gap = Utc.with_ymd_and_hms(2026, 9, 5, 14, 0, 0).unwrap();
        assert!(missed_at(before_gap, before_gap + Duration::hours(1), santiago) > before_gap + Duration::hours(1));
    }
}
