//! Canonical text form of UTC instants, shared by SQLite storage and the sync wire.

use crate::{invalid, Result};
use chrono::{DateTime, SecondsFormat, Utc};

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
}
