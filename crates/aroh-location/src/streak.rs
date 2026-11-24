//! Routine streaks and cadence labels for the Routines dashboard (owner answer 7).

use aroh_domain::rows::{Action, ActionSeries, ActionStatus};
use chrono::{DateTime, Utc};
use serde::Serialize;

/// Consecutive done occurrences back from the newest decided past one, with its unit label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Streak {
    pub count: usize,
    pub label: String,
}

struct Rule {
    freq: String,
    interval: u32,
    days: Vec<String>,
}

fn rule(series: &ActionSeries) -> Rule {
    let mut out = Rule { freq: String::new(), interval: 1, days: Vec::new() };
    for part in series.rrule.trim_start_matches("RRULE:").split(';') {
        match part.split_once('=') {
            Some(("FREQ", v)) => out.freq = v.to_string(),
            Some(("INTERVAL", v)) => out.interval = v.parse().unwrap_or(1).max(1),
            Some(("BYDAY", v)) => out.days = v.split(',').map(|d| d.trim_start_matches(|c: char| c == '-' || c.is_ascii_digit()).to_string()).collect(),
            _ => {}
        }
    }
    out
}

fn unit(series: &ActionSeries) -> (&'static str, &'static str) {
    let r = rule(series);
    match (r.freq.as_str(), r.interval) {
        ("DAILY", 1) => ("day", "days"),
        ("WEEKLY", 1) if r.days.len() <= 1 => ("week", "weeks"),
        _ => ("time", "times"),
    }
}

/// The streak of `series` at `now`: occurrences in `actions` that started before `now`,
/// newest first, skipping ones still planned, counted while done. Labelled by cadence:
/// daily "12 days", weekly on one day "4 weeks", anything else "3 times".
pub fn streak(series: &ActionSeries, actions: &[Action], now: DateTime<Utc>) -> Streak {
    let mut past: Vec<&Action> = actions
        .iter()
        .filter(|a| !a.deleted && a.series_id.as_deref() == Some(series.id.as_str()) && a.start_at <= now && a.status != ActionStatus::Planned)
        .collect();
    past.sort_by(|a, b| b.start_at.cmp(&a.start_at));
    let count = past.iter().take_while(|a| a.status == ActionStatus::Done).count();
    let (one, many) = unit(series);
    Streak { count, label: format!("{count} {}", if count == 1 { one } else { many }) }
}

fn day_name(code: &str) -> &str {
    match code {
        "MO" => "Mon",
        "TU" => "Tue",
        "WE" => "Wed",
        "TH" => "Thu",
        "FR" => "Fri",
        "SA" => "Sat",
        "SU" => "Sun",
        other => other,
    }
}

/// The cadence of `series` as the Routines table shows it: "Daily", "Weekly, Sat",
/// "Every 3 weeks", "Every 2 days", "Monthly".
pub fn cadence(series: &ActionSeries) -> String {
    let r = rule(series);
    let days: Vec<&str> = r.days.iter().map(|d| day_name(d)).collect();
    match (r.freq.as_str(), r.interval) {
        ("DAILY", 1) => "Daily".into(),
        ("DAILY", n) => format!("Every {n} days"),
        ("WEEKLY", 1) if days.is_empty() => "Weekly".into(),
        ("WEEKLY", 1) => format!("Weekly, {}", days.join(", ")),
        ("WEEKLY", n) => format!("Every {n} weeks"),
        ("MONTHLY", 1) => "Monthly".into(),
        ("MONTHLY", n) => format!("Every {n} months"),
        _ => "Repeats".into(),
    }
}
