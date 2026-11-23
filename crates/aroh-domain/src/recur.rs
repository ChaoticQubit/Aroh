//! Recurrence (Q34): RRULE parsing with bounds, rolling-window materialization of
//! action series into actions, and the "this and following" split.

use crate::rows::{Action, ActionSeries, ActionStatus};
use crate::{derived_id, Error, Result};
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use rrule::{Frequency, RRuleSet, Tz};
use std::collections::BTreeSet;

/// Days ahead the app and server engines keep occurrences materialized.
pub const DEFAULT_WINDOW_DAYS: u32 = 14;

/// Most occurrences one window may produce; a rule that yields more is refused.
pub const MAX_PER_WINDOW: usize = 500;

/// Largest COUNT a series rule may carry.
pub const MAX_COUNT: u32 = 10_000;

pub(crate) const LOCAL_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
const ICAL_LOCAL: &str = "%Y%m%dT%H%M%S";
const ICAL_UTC: &str = "%Y%m%dT%H%M%SZ";

fn recurrence(reason: impl Into<String>) -> Error {
    Error::Recurrence(reason.into().chars().take(200).collect())
}

fn timezone(series: &ActionSeries) -> Result<chrono_tz::Tz> {
    series.timezone.parse().map_err(|_| recurrence(format!("unknown timezone {:?}", series.timezone)))
}

/// Parses a series into a validated rule set anchored at its local start in its timezone.
/// Refuses unknown timezones, a malformed local start, a rule line carrying other
/// iCalendar properties, sub-daily frequencies, several rules and COUNT above [`MAX_COUNT`].
/// Errors with `Recurrence` naming the problem.
pub fn rule_set(series: &ActionSeries) -> Result<RRuleSet> {
    let tz = timezone(series)?;
    let local = NaiveDateTime::parse_from_str(&series.dtstart_local, LOCAL_FORMAT)
        .map_err(|_| recurrence("dtstart_local must be YYYY-MM-DDTHH:MM:SS"))?;
    let line = series.rrule.trim();
    let upper = line.to_ascii_uppercase();
    if line.is_empty() || line.len() > 500 || line.contains(['\n', '\r', ':']) || upper.contains("DTSTART") {
        return Err(recurrence("rrule must be one RRULE value such as FREQ=DAILY"));
    }
    let text = format!("DTSTART;TZID={}:{}\nRRULE:{}", tz.name(), local.format(ICAL_LOCAL), line);
    let set: RRuleSet = text.parse().map_err(|e: rrule::RRuleError| recurrence(e.to_string()))?;
    let [rule] = set.get_rrule().as_slice() else {
        return Err(recurrence("exactly one rule"));
    };
    if matches!(rule.get_freq(), Frequency::Secondly | Frequency::Minutely | Frequency::Hourly) {
        return Err(recurrence("sub-daily frequencies are not allowed"));
    }
    if rule.get_count().is_some_and(|c| c > MAX_COUNT) {
        return Err(recurrence(format!("COUNT above {MAX_COUNT}")));
    }
    Ok(set)
}

/// Returns the deterministic id of the occurrence of `series_id` at `at`, a v7-shaped
/// uuid, so every engine that materializes the same occurrence writes the same row.
pub fn occurrence_id(series_id: &str, at: &DateTime<Utc>) -> String {
    let millis = u64::try_from(at.timestamp_millis()).unwrap_or(0);
    derived_id(series_id, &at.timestamp_millis().to_string(), millis)
}

fn occurrences(set: RRuleSet, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<DateTime<Utc>>> {
    assert!(from < to, "occurrence range must not be empty");
    let found = set.after(from.with_timezone(&Tz::UTC)).before(to.with_timezone(&Tz::UTC)).all(MAX_PER_WINDOW as u16 + 1);
    let dates: Vec<DateTime<Utc>> =
        found.dates.iter().map(|d| d.with_timezone(&Utc)).filter(|d| *d >= from && *d < to).collect();
    if dates.len() > MAX_PER_WINDOW {
        return Err(recurrence(format!("more than {MAX_PER_WINDOW} occurrences in one window")));
    }
    Ok(dates)
}

/// Builds the actions of `series` that start in `[from, from + days)` and are not in
/// `existing` (occurrence instants already stored, tombstoned and locked ones included).
/// Each action has a deterministic id and the lowest stamp ([`crate::seed::SEED_STAMP`]),
/// so engines derive identical rows, no device pushes an untouched one, and any real
/// edit or tombstone wins under LWW, even after the series is re-stamped.
pub fn materialize(series: &ActionSeries, from: DateTime<Utc>, days: u32, existing: &BTreeSet<DateTime<Utc>>) -> Result<Vec<Action>> {
    assert!((1..=366).contains(&days), "window must be 1 to 366 days");
    if series.deleted {
        return Ok(Vec::new());
    }
    let set = rule_set(series)?;
    let to = from + Duration::days(i64::from(days));
    let actions: Vec<Action> =
        occurrences(set, from, to)?.into_iter().filter(|at| !existing.contains(at)).map(|at| occurrence(series, at)).collect();
    assert!(actions.len() <= MAX_PER_WINDOW);
    Ok(actions)
}

/// The derived action of `series` at occurrence `at`: deterministic id, planned,
/// unlocked, the series' length and the lowest stamp, exactly as [`materialize`]
/// writes it. Input: a series and one of its occurrence instants. Output: the action.
pub fn occurrence(series: &ActionSeries, at: DateTime<Utc>) -> Action {
    Action {
        id: occurrence_id(&series.id, &at),
        user_id: series.user_id.clone(),
        goal_id: series.goal_id.clone(),
        series_id: Some(series.id.clone()),
        occurrence: Some(at),
        title: series.title.clone(),
        notes: series.notes.clone(),
        start_at: at,
        end_at: at + Duration::minutes(series.duration_minutes),
        status: ActionStatus::Planned,
        locked: false,
        done_at: None,
        updated_at: crate::seed::SEED_STAMP.to_string(),
        deleted: false,
    }
}

/// Whether `at` is an occurrence of the current rule of `series`.
/// Input: a series and an instant. Output: true when the rule yields exactly `at`.
/// Errors with `Recurrence` when the series rule does not validate.
pub fn is_occurrence(series: &ActionSeries, at: DateTime<Utc>) -> Result<bool> {
    Ok(occurrences(rule_set(series)?, at, at + Duration::seconds(1))? == [at])
}

fn rule_parts(line: &str) -> Vec<String> {
    line.split(';')
        .filter(|p| !p.is_empty())
        .filter(|p| {
            let key = p.split('=').next().unwrap_or("").to_ascii_uppercase();
            key != "COUNT" && key != "UNTIL"
        })
        .map(str::to_string)
        .collect()
}

/// Splits `series` for a "this and following" edit at occurrence `at`.
/// Output: the old series bounded by UNTIL one second before `at` (tombstoned when
/// `at` is its first occurrence) and a new series with id `new_id` starting at `at`,
/// keeping the old UNTIL or the remaining COUNT. Errors when `at` is not an occurrence.
pub fn split(series: &ActionSeries, at: DateTime<Utc>, new_id: String) -> Result<(ActionSeries, ActionSeries)> {
    let set = rule_set(series)?;
    let tz = timezone(series)?;
    let start = *set.get_dt_start();
    let rule = set.get_rrule()[0].clone();
    if !is_occurrence(series, at)? {
        return Err(recurrence("split point is not an occurrence of the series"));
    }
    let start_utc = start.with_timezone(&Utc);
    let before = if at > start_utc {
        set.before((at - Duration::seconds(1)).with_timezone(&Tz::UTC)).all(u16::MAX).dates.len()
    } else {
        0
    };
    let kept = rule_parts(&series.rrule);

    let mut old = series.clone();
    if before == 0 {
        old.deleted = true;
    } else {
        let until = (at - Duration::seconds(1)).format(ICAL_UTC);
        old.rrule = [kept.clone(), vec![format!("UNTIL={until}")]].concat().join(";");
    }

    let mut tail = kept;
    if let Some(count) = rule.get_count() {
        let remaining = count as usize - before;
        assert!(remaining >= 1, "split occurrence must be within COUNT");
        tail.push(format!("COUNT={remaining}"));
    } else if let Some(until) = rule.get_until() {
        tail.push(format!("UNTIL={}", until.with_timezone(&Utc).format(ICAL_UTC)));
    }
    let mut new = series.clone();
    new.id = new_id;
    new.rrule = tail.join(";");
    new.dtstart_local = at.with_timezone(&tz).naive_local().format(LOCAL_FORMAT).to_string();
    new.deleted = false;

    if !old.deleted {
        rule_set(&old)?;
    }
    rule_set(&new)?;
    Ok((old, new))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rows::samples;
    use crate::rows::Record;
    use chrono::{Offset, TimeZone, Timelike};

    fn utc(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
    }

    fn berlin_hour(t: &DateTime<Utc>) -> u32 {
        t.with_timezone(&chrono_tz::Europe::Berlin).hour()
    }

    fn offset_hours(t: &DateTime<Utc>) -> i32 {
        t.with_timezone(&chrono_tz::Europe::Berlin).offset().fix().local_minus_utc() / 3600
    }

    #[test]
    fn window_keeps_local_time_across_autumn_dst() {
        let series = samples::series();
        let actions = materialize(&series, utc(2026, 10, 22, 0), DEFAULT_WINDOW_DAYS, &BTreeSet::new()).unwrap();
        assert_eq!(actions.len(), 14);
        assert!(actions.iter().all(|a| berlin_hour(&a.start_at) == 7));
        let before = actions.iter().find(|a| a.start_at.date_naive() == utc(2026, 10, 24, 0).date_naive()).unwrap();
        let after = actions.iter().find(|a| a.start_at.date_naive() == utc(2026, 10, 26, 0).date_naive()).unwrap();
        assert_eq!((before.start_at.hour(), offset_hours(&before.start_at)), (5, 2));
        assert_eq!((after.start_at.hour(), offset_hours(&after.start_at)), (6, 1));
        assert!(actions.iter().all(|a| a.end_at - a.start_at == Duration::minutes(45)));
        assert!(actions.iter().all(|a| a.updated_at == crate::seed::SEED_STAMP && a.validate().is_ok()));
    }

    #[test]
    fn window_keeps_local_time_across_spring_dst() {
        let series = samples::series();
        let actions = materialize(&series, utc(2027, 3, 21, 0), 14, &BTreeSet::new()).unwrap();
        assert_eq!(actions.len(), 14);
        let before = actions.iter().find(|a| a.start_at.date_naive() == utc(2027, 3, 27, 0).date_naive()).unwrap();
        let after = actions.iter().find(|a| a.start_at.date_naive() == utc(2027, 3, 29, 0).date_naive()).unwrap();
        assert_eq!((before.start_at.hour(), offset_hours(&before.start_at)), (6, 1));
        assert_eq!((after.start_at.hour(), offset_hours(&after.start_at)), (5, 2));
        assert!(actions.iter().all(|a| berlin_hour(&a.start_at) == 7));
    }

    #[test]
    fn window_end_is_exclusive_and_existing_occurrences_are_skipped() {
        let series = samples::series();
        let from = utc(2026, 11, 23, 6);
        let first = materialize(&series, from, 2, &BTreeSet::new()).unwrap();
        assert_eq!(first.len(), 2);
        let mut existing: BTreeSet<_> = first.iter().map(|a| a.occurrence.unwrap()).collect();
        assert!(materialize(&series, from, 2, &existing).unwrap().is_empty());
        existing.remove(&first[1].occurrence.unwrap());
        let again = materialize(&series, from, 2, &existing).unwrap();
        assert_eq!(again, vec![first[1].clone()]);
        let mut gone = series.clone();
        gone.deleted = true;
        assert!(materialize(&gone, from, 2, &BTreeSet::new()).unwrap().is_empty());
    }

    #[test]
    fn occurrence_ids_are_deterministic_v7() {
        let at = utc(2026, 11, 23, 6);
        let id = occurrence_id(&samples::series().id, &at);
        assert_eq!(id, occurrence_id(&samples::series().id, &at));
        assert_ne!(id, occurrence_id(&samples::series().id, &(at + Duration::days(1))));
        assert_ne!(id, occurrence_id(&samples::goal_id(), &at));
        let parsed = uuid::Uuid::parse_str(&id).unwrap();
        assert_eq!(parsed.get_version_num(), 7);
        assert_eq!(parsed.get_timestamp().unwrap().to_unix().0, at.timestamp() as u64);
    }

    #[test]
    fn split_divides_count_and_starts_new_series_at_the_occurrence() {
        let mut series = samples::series();
        series.rrule = "FREQ=DAILY;COUNT=10".into();
        let all = materialize(&series, utc(2026, 10, 1, 0), 30, &BTreeSet::new()).unwrap();
        assert_eq!(all.len(), 10);
        let at = all[3].start_at;
        let (old, new) = split(&series, at, crate::new_id()).unwrap();
        assert!(!old.deleted);
        assert!(old.rrule.contains("UNTIL=") && !old.rrule.contains("COUNT"));
        assert_eq!(new.rrule, "FREQ=DAILY;COUNT=7");
        assert_eq!(new.dtstart_local, "2026-10-23T07:00:00");
        let old_dates = materialize(&old, utc(2026, 10, 1, 0), 30, &BTreeSet::new()).unwrap();
        let new_dates = materialize(&new, utc(2026, 10, 1, 0), 30, &BTreeSet::new()).unwrap();
        assert_eq!(old_dates.len(), 3);
        assert_eq!(new_dates.len(), 7);
        let joined: Vec<_> = old_dates.iter().chain(&new_dates).map(|a| a.start_at).collect();
        assert_eq!(joined, all.iter().map(|a| a.start_at).collect::<Vec<_>>());
        assert!(new_dates.iter().all(|a| a.series_id.as_deref() == Some(new.id.as_str())));
    }

    #[test]
    fn split_keeps_until_and_tombstones_when_split_at_first() {
        let mut series = samples::series();
        series.rrule = "FREQ=WEEKLY;BYDAY=MO,WE;UNTIL=20261231T000000Z".into();
        let all = materialize(&series, utc(2026, 10, 1, 0), 90, &BTreeSet::new()).unwrap();
        let (old, new) = split(&series, all[2].start_at, crate::new_id()).unwrap();
        assert!(new.rrule.ends_with("UNTIL=20261231T000000Z"));
        assert_eq!(materialize(&old, utc(2026, 10, 1, 0), 90, &BTreeSet::new()).unwrap().len(), 2);
        assert_eq!(materialize(&new, utc(2026, 10, 1, 0), 90, &BTreeSet::new()).unwrap().len(), all.len() - 2);
        let (first_old, _) = split(&series, all[0].start_at, crate::new_id()).unwrap();
        assert!(first_old.deleted);
        assert!(split(&series, all[0].start_at + Duration::hours(1), crate::new_id()).is_err());
    }

    #[test]
    fn bad_rules_are_refused() {
        for (rrule, tz, start) in [
            ("FREQ=HOURLY", "Europe/Berlin", "2026-10-20T07:00:00"),
            ("FREQ=MINUTELY", "Europe/Berlin", "2026-10-20T07:00:00"),
            ("FREQ=DAILY", "Mars/Olympus", "2026-10-20T07:00:00"),
            ("FREQ=DAILY", "Europe/Berlin", "2026-10-20 07:00"),
            ("FREQ=DAILY;COUNT=10001", "Europe/Berlin", "2026-10-20T07:00:00"),
            ("FREQ=DAILY\nRRULE:FREQ=WEEKLY", "Europe/Berlin", "2026-10-20T07:00:00"),
            ("DTSTART=20261020T070000;FREQ=DAILY", "Europe/Berlin", "2026-10-20T07:00:00"),
            ("FREQ=SOMETIMES", "Europe/Berlin", "2026-10-20T07:00:00"),
        ] {
            let mut s = samples::series();
            s.rrule = rrule.into();
            s.timezone = tz.into();
            s.dtstart_local = start.into();
            assert!(rule_set(&s).is_err(), "{rrule} {tz} {start}");
            assert!(crate::rows::Record::validate(&s).is_err());
        }
        let mut dense = samples::series();
        dense.rrule = "FREQ=DAILY;BYHOUR=0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23;BYMINUTE=0,30".into();
        assert!(materialize(&dense, utc(2026, 11, 1, 0), 14, &BTreeSet::new()).is_err());
    }

    #[test]
    fn nonexistent_local_times_shift_forward_or_are_refused_at_start() {
        let mut s = samples::series();
        s.dtstart_local = "2027-03-20T02:30:00".into();
        let starts: Vec<_> = materialize(&s, utc(2027, 3, 27, 0), 3, &BTreeSet::new()).unwrap().into_iter().map(|a| a.start_at).collect();
        let half = |d, h| Utc.with_ymd_and_hms(2027, 3, d, h, 30, 0).unwrap();
        assert_eq!(starts, vec![half(27, 1), half(28, 1), half(29, 0)]);
        assert_eq!(berlin_hour(&starts[1]), 3);
        s.dtstart_local = "2027-03-28T02:30:00".into();
        assert!(matches!(rule_set(&s), Err(Error::Recurrence(_))));
    }
}
