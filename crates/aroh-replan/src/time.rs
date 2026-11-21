use crate::model::Availability;
use crate::parsed::PGoal;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

/// A half-open UTC interval [start, end).
pub(crate) type Span = (DateTime<Utc>, DateTime<Utc>);

/// Local calendar date of a UTC instant under the availability offset.
pub(crate) fn local_date(at: DateTime<Utc>, availability: &Availability) -> NaiveDate {
    (at + Duration::minutes(availability.utc_offset_minutes.into())).date_naive()
}

/// UTC instant of local midnight starting `date`.
fn local_midnight(date: NaiveDate, availability: &Availability) -> DateTime<Utc> {
    let midnight = date.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc();
    midnight - Duration::minutes(availability.utc_offset_minutes.into())
}

/// First instant after the goal's target date ends, in UTC.
pub(crate) fn deadline(goal: &PGoal, availability: &Availability) -> DateTime<Utc> {
    local_midnight(goal.target_date + Duration::days(1), availability)
}

/// Free windows of one local date as sorted, merged UTC spans.
pub(crate) fn day_windows(date: NaiveDate, availability: &Availability) -> Vec<Span> {
    let midnight = local_midnight(date, availability);
    let weekday = date.weekday().num_days_from_sunday();
    let mut spans: Vec<Span> = availability
        .windows
        .iter()
        .filter(|w| u32::from(w.weekday) == weekday && w.start < w.end)
        .map(|w| {
            (
                midnight + Duration::minutes(w.start.0.into()),
                midnight + Duration::minutes(w.end.0.into()),
            )
        })
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
