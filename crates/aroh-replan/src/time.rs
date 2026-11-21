use crate::model::Availability;
use crate::parsed::PGoal;
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};

/// A half-open UTC interval [start, end).
pub(crate) type Span = (DateTime<Utc>, DateTime<Utc>);

/// Local calendar date of a UTC instant in the availability time zone.
pub(crate) fn local_date(at: DateTime<Utc>, availability: &Availability) -> NaiveDate {
    at.with_timezone(&availability.timezone).date_naive()
}

/// Longest DST gap searched past a local time that does not exist.
const GAP_MAX_MIN: i64 = 180;

/// UTC instant `minutes` after local midnight of `date` on the wall clock.
/// A wall time inside a DST gap moves to the first valid minute after it;
/// a repeated wall time takes its earlier instant.
fn local_instant(date: NaiveDate, minutes: i64, availability: &Availability) -> DateTime<Utc> {
    let wall = date.and_hms_opt(0, 0, 0).expect("midnight exists") + Duration::minutes(minutes);
    for shift in 0..=GAP_MAX_MIN {
        let local = wall + Duration::minutes(shift);
        if let Some(t) = availability.timezone.from_local_datetime(&local).earliest() {
            return t.with_timezone(&Utc);
        }
    }
    unreachable!("no valid local time within {GAP_MAX_MIN} minutes of {wall} in {}", availability.timezone)
}

/// First instant after the goal's target date ends, in UTC.
pub(crate) fn deadline(goal: &PGoal, availability: &Availability) -> DateTime<Utc> {
    local_instant(goal.target_date + Duration::days(1), 0, availability)
}

/// Free windows of one local date as sorted, merged UTC spans.
pub(crate) fn day_windows(date: NaiveDate, availability: &Availability) -> Vec<Span> {
    let weekday = date.weekday().num_days_from_sunday();
    let mut spans: Vec<Span> = availability
        .windows
        .iter()
        .filter(|w| u32::from(w.weekday) == weekday && w.start < w.end)
        .map(|w| (local_instant(date, w.start.0.into(), availability), local_instant(date, w.end.0.into(), availability)))
        .filter(|span| span.0 < span.1)
        .collect();
    spans.sort();
    let mut merged: Vec<Span> = Vec::with_capacity(spans.len());
    for span in spans {
        match merged.last_mut() {
            Some(last) if span.0 <= last.1 => last.1 = last.1.max(span.1),
            _ => merged.push(span),
        }
    }
    merged
}

/// True when [start, end) lies inside one free window of its local date.
pub(crate) fn inside_window(start: DateTime<Utc>, end: DateTime<Utc>, availability: &Availability) -> bool {
    day_windows(local_date(start, availability), availability)
        .iter()
        .any(|w| w.0 <= start && end <= w.1)
}
