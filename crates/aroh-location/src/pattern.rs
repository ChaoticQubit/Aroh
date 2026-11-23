//! Learned patterns (Q56): the same action done at the same named place at a regular gap
//! becomes a proposal for a repeating action. Nothing is created here; the owner approves.

use crate::Visit;
use aroh_domain::rows::{Action, ActionSeries, ActionStatus};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Timelike, Utc, Weekday};
use chrono_tz::Tz;
use std::collections::BTreeMap;

/// Fewest occurrences a pattern needs.
pub const MIN_OCCURRENCES: usize = 4;

/// Days of done actions a pattern looks back on.
pub const LOOKBACK_DAYS: i64 = 90;

/// Minutes before arrival or after leaving that a done action still counts for the visit.
pub const SLACK_MIN: i64 = 30;

/// Days a declined pattern is not proposed again.
pub const DECLINE_DAYS: i64 = 60;

/// A regular routine at a place: normalized title `key` and the newest `title` as written,
/// median gap in days, the shared weekday when at least 75 percent agree, the median start
/// rounded to 15 minutes, the median length and how many occurrences back it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub place_id: String,
    pub key: String,
    pub title: String,
    pub gap_days: i64,
    pub weekday: Option<Weekday>,
    pub local_time: NaiveTime,
    pub duration_minutes: i64,
    pub evidence: usize,
}

/// A pattern the owner declined: not proposed again before `until`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declined {
    pub place_id: String,
    pub key: String,
    pub until: DateTime<Utc>,
}

/// An action title as patterns compare it: trimmed, lowercase, single spaces.
pub fn normalize(title: &str) -> String {
    title.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn median(values: &mut [i64]) -> i64 {
    assert!(!values.is_empty(), "a median needs values");
    values.sort_unstable();
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2
    }
}

fn place_of<'a>(visits: &'a [Visit], done: DateTime<Utc>, now: DateTime<Utc>) -> Option<&'a str> {
    let slack = Duration::minutes(SLACK_MIN);
    visits
        .iter()
        .filter(|v| v.arrived_at - slack <= done && done <= v.left_at.unwrap_or(now) + slack)
        .find_map(|v| v.place_id.as_deref())
}

fn pattern(place_id: &str, key: &str, mut group: Vec<&Action>, zone: Tz) -> Option<Pattern> {
    group.sort_by_key(|a| a.start_at);
    let mut by_day: BTreeMap<NaiveDate, &Action> = BTreeMap::new();
    for a in group {
        by_day.entry(a.start_at.with_timezone(&zone).date_naive()).or_insert(a);
    }
    if by_day.len() < MIN_OCCURRENCES {
        return None;
    }
    let days: Vec<NaiveDate> = by_day.keys().copied().collect();
    let gaps: Vec<i64> = days.windows(2).map(|w| (w[1] - w[0]).num_days()).collect();
    let gap = median(&mut gaps.clone());
    if !(1..=35).contains(&gap) {
        return None;
    }
    let tolerance = ((gap as f64) * 0.25).round().max(1.0) as i64;
    if gaps.iter().any(|g| (*g - gap).abs() > tolerance) {
        return None;
    }
    let actions: Vec<&Action> = by_day.values().copied().collect();
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    for a in &actions {
        *counts.entry(a.start_at.with_timezone(&zone).weekday().num_days_from_monday()).or_default() += 1;
    }
    let (top, top_count) = counts.iter().max_by_key(|(_, c)| **c).map(|(d, c)| (*d, *c)).expect("occurrences have weekdays");
    let weekday = (top_count * 4 >= actions.len() * 3).then(|| Weekday::try_from(top as u8).expect("0 to 6 is a weekday"));
    let mut minutes: Vec<i64> = actions.iter().map(|a| a.start_at.with_timezone(&zone)).map(|l| i64::from(l.hour() * 60 + l.minute())).collect();
    let rounded = ((median(&mut minutes) + 7) / 15 * 15) % 1440;
    let local_time = NaiveTime::from_hms_opt((rounded / 60) as u32, (rounded % 60) as u32, 0).expect("a minute of the day");
    let mut lengths: Vec<i64> = actions.iter().map(|a| (a.end_at - a.start_at).num_minutes().clamp(1, 1440)).collect();
    let title = actions.last().expect("at least four occurrences").title.trim().to_string();
    Some(Pattern {
        place_id: place_id.to_string(),
        key: key.to_string(),
        title,
        gap_days: gap,
        weekday,
        local_time,
        duration_minutes: median(&mut lengths),
        evidence: actions.len(),
    })
}

/// Finds routines in the last [`LOOKBACK_DAYS`] days: done actions whose `done_at` falls in a
/// visit to a named place (plus or minus [`SLACK_MIN`] minutes), grouped by place and
/// normalized title, with at least [`MIN_OCCURRENCES`] local days whose gaps stay within
/// 25 percent (at least one day) of a median of 1 to 35 days. Skips titles that already have
/// a live series and patterns declined until after `now`. Output: patterns by place and key.
pub fn detect_patterns(visits: &[Visit], actions: &[Action], series: &[ActionSeries], declined: &[Declined], now: DateTime<Utc>, zone: Tz) -> Vec<Pattern> {
    let since = now - Duration::days(LOOKBACK_DAYS);
    let mut groups: BTreeMap<(String, String), Vec<&Action>> = BTreeMap::new();
    for a in actions.iter().filter(|a| !a.deleted && a.status == ActionStatus::Done) {
        let Some(done) = a.done_at.filter(|d| *d >= since && *d <= now) else { continue };
        let Some(place) = place_of(visits, done, now) else { continue };
        groups.entry((place.to_string(), normalize(&a.title))).or_default().push(a);
    }
    groups
        .into_iter()
        .filter(|((_, key), _)| !series.iter().any(|s| !s.deleted && normalize(&s.title) == *key))
        .filter(|((place, key), _)| !declined.iter().any(|d| d.place_id == *place && d.key == *key && d.until > now))
        .filter_map(|((place, key), group)| pattern(&place, &key, group, zone))
        .collect()
}
