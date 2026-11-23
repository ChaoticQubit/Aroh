//! Dashboard figures per goal (Q69): done vs due to date, projected finish and pace,
//! streaks, completion, minutes per week and the tracker trend. Pure, no I/O.

use crate::recur::rule_set;
use crate::rows::{Action, ActionSeries, ActionStatus, FieldKind, Goal, GoalStatus, Tracker, TrackerEntry};
use crate::seed::routines_id;
use crate::Result;
use chrono::{DateTime, Datelike, Days, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use std::collections::BTreeMap;

/// Weeks the minutes chart shows, the current week last.
pub const WEEKS: usize = 6;
/// Weeks the tracker trend shows, the current week last.
pub const TREND_WEEKS: usize = 7;
/// Share of due actions that must be done for a goal to keep pace.
pub const ON_TRACK_SHARE: f64 = 0.85;
/// Days before the deadline a finish must fall, with nothing owed, to count as ahead.
pub const AHEAD_DAYS: u64 = 7;

/// How a goal keeps up with its plan and deadline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pace {
    Ahead,
    OnTrack,
    Behind,
}

/// Done and due action counts of one window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub done: u32,
    pub due: u32,
}

/// One number field of a goal's tracker, one point per trend week (the last value
/// logged that week, `None` for a week without one), and the plugin target if any.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trend {
    pub tracker_id: String,
    pub label: String,
    pub unit: Option<String>,
    pub points: Vec<Option<f64>>,
    pub target: Option<f64>,
}

/// The dashboard figures of one goal. `missed` and `skipped` count due actions with
/// that status; `minutes` has one value per week of [`Dashboard::weeks`]; `plan_end`
/// and `finish` are local dates, `None` with no sessions or when `open_ended` (a series
/// without an end).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GoalStats {
    pub goal_id: String,
    pub done: u32,
    pub due: u32,
    pub missed: u32,
    pub skipped: u32,
    pub open_ended: bool,
    pub plan_end: Option<NaiveDate>,
    pub finish: Option<NaiveDate>,
    pub pace: Pace,
    pub streak: u32,
    pub best_streak: u32,
    pub week: Counts,
    pub month: Counts,
    pub minutes: Vec<i64>,
    pub trend: Option<Trend>,
}

/// Every dashboard goal in stable order (created, then id), with the Mondays of the
/// minutes chart and of the trend, oldest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Dashboard {
    pub weeks: Vec<NaiveDate>,
    pub trend_weeks: Vec<NaiveDate>,
    pub goals: Vec<GoalStats>,
}

/// The live rows the dashboard reads. `targets` maps a tracker id to its field
/// targets by field key, taken from the goal's active plugin version.
pub struct Rows<'a> {
    pub goals: &'a [Goal],
    pub actions: &'a [Action],
    pub series: &'a [ActionSeries],
    pub trackers: &'a [Tracker],
    pub entries: &'a [TrackerEntry],
    pub targets: &'a BTreeMap<String, BTreeMap<String, f64>>,
}

/// Whether `goal` belongs on the dashboard: not a draft, not archived and not the
/// built-in Routines goal.
pub fn shown(goal: &Goal) -> bool {
    !matches!(goal.status, GoalStatus::Draft | GoalStatus::Archived) && goal.id != routines_id()
}

/// Whether action `a` counts to date at `now`: its slot has ended or its status is
/// no longer planned (done early counts at once).
pub fn is_due(a: &Action, now: DateTime<Utc>) -> bool {
    a.end_at <= now || a.status != ActionStatus::Planned
}

/// The pace of a goal from its done and due counts, projected finish and deadline.
/// Behind under [`ON_TRACK_SHARE`] or past the deadline; ahead when nothing is owed and
/// the finish is at least [`AHEAD_DAYS`] before the deadline; else on track.
pub fn pace(done: u32, due: u32, finish: Option<NaiveDate>, deadline: Option<NaiveDate>) -> Pace {
    assert!(done <= due, "done {done} above due {due}");
    let short = due > 0 && f64::from(done) < ON_TRACK_SHARE * f64::from(due);
    let (Some(finish), Some(deadline)) = (finish, deadline) else {
        return if short { Pace::Behind } else { Pace::OnTrack };
    };
    if short || finish > deadline {
        return Pace::Behind;
    }
    let early = deadline.checked_sub_days(Days::new(AHEAD_DAYS)).is_some_and(|d| finish <= d);
    if done == due && early {
        Pace::Ahead
    } else {
        Pace::OnTrack
    }
}

/// The end of the last occurrence of a bounded series (COUNT or UNTIL), or `None`
/// when the series has no end. Errors with `Recurrence` on a rule that does not parse.
pub fn last_occurrence(series: &ActionSeries) -> Result<Option<DateTime<Utc>>> {
    let set = rule_set(series)?;
    let bounded = set.get_rrule().iter().all(|r| r.get_count().is_some() || r.get_until().is_some());
    if !bounded {
        return Ok(None);
    }
    let found = set.all(u16::MAX);
    if found.limited {
        return Ok(None);
    }
    let last = found.dates.last().map(|d| d.with_timezone(&Utc) + chrono::Duration::minutes(series.duration_minutes));
    Ok(last)
}

/// The plan end and projected finish of a goal: the local date of its last session
/// (latest action or bounded series end), then `owed` sessions at the average gap
/// between action starts (one day under 2 actions) added from the later of plan end
/// and `today`. `None` for both with no sessions, or when any series end is `None`.
pub fn projected_finish(actions: &[&Action], series_ends: &[Option<DateTime<Utc>>], owed: u32, today: NaiveDate, zone: Tz) -> (Option<NaiveDate>, Option<NaiveDate>) {
    if series_ends.iter().any(Option::is_none) {
        return (None, None);
    }
    let last = actions.iter().map(|a| a.end_at).chain(series_ends.iter().flatten().copied()).max();
    let Some(last) = last else {
        return (None, None);
    };
    let plan_end = last.with_timezone(&zone).date_naive();
    let first = actions.iter().map(|a| a.start_at).min();
    let latest = actions.iter().map(|a| a.start_at).max();
    let span_days = match (first, latest) {
        (Some(f), Some(l)) if actions.len() >= 2 && l > f => (l - f).num_seconds() as f64 / 86_400.0 / (actions.len() - 1) as f64,
        _ => 1.0,
    };
    let slip = (f64::from(owed) * span_days).ceil();
    assert!(slip >= 0.0 && slip.is_finite());
    let from = if owed > 0 { plan_end.max(today) } else { plan_end };
    let finish = from.checked_add_days(Days::new(slip as u64));
    (Some(plan_end), finish)
}

/// Current and best streak of a goal: runs of local days on which every due action
/// was done. Days with no due action or only skipped ones are neutral; on today a
/// still-planned action is neutral too, so an open today never breaks the run.
pub fn streaks(actions: &[&Action], now: DateTime<Utc>, zone: Tz) -> (u32, u32) {
    let today = now.with_timezone(&zone).date_naive();
    let mut days: BTreeMap<NaiveDate, Option<bool>> = BTreeMap::new();
    for a in actions.iter().filter(|a| is_due(a, now)) {
        let day = a.start_at.with_timezone(&zone).date_naive();
        let kept = match a.status {
            ActionStatus::Done => Some(true),
            ActionStatus::Skipped => None,
            ActionStatus::Planned if day == today => None,
            ActionStatus::Planned | ActionStatus::Missed => Some(false),
        };
        let entry = days.entry(day).or_insert(None);
        *entry = match (*entry, kept) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), _) | (_, Some(true)) => Some(true),
            (None, None) => None,
        };
    }
    let (mut run, mut best) = (0u32, 0u32);
    for kept in days.values().flatten() {
        run = if *kept { run + 1 } else { 0 };
        best = best.max(run);
    }
    (run, best)
}

/// The Monday of the local week holding `day`.
fn monday(day: NaiveDate) -> NaiveDate {
    day.checked_sub_days(Days::new(u64::from(day.weekday().num_days_from_monday()))).expect("monday in range")
}

/// The Mondays of the last `n` local weeks at `now` in `zone`, oldest first, the
/// current week last. `n` must be at least 1.
pub fn week_starts(now: DateTime<Utc>, zone: Tz, n: usize) -> Vec<NaiveDate> {
    assert!(n >= 1, "need at least one week");
    let current = monday(now.with_timezone(&zone).date_naive());
    let weeks: Vec<NaiveDate> = (0..n).rev().map(|i| current.checked_sub_days(Days::new(7 * i as u64)).expect("week in range")).collect();
    assert_eq!(weeks.len(), n);
    weeks
}

/// Minutes of done actions per week of `weeks`, by the local week of each start.
/// An action's minutes are its slot length; weeks outside `weeks` are dropped.
pub fn weekly_minutes(actions: &[&Action], weeks: &[NaiveDate], zone: Tz) -> Vec<i64> {
    let mut minutes = vec![0i64; weeks.len()];
    for a in actions.iter().filter(|a| a.status == ActionStatus::Done) {
        let week = monday(a.start_at.with_timezone(&zone).date_naive());
        if let Some(i) = weeks.iter().position(|w| *w == week) {
            minutes[i] += (a.end_at - a.start_at).num_minutes().max(0);
        }
    }
    minutes
}

/// Done and due counts of the due actions starting on or after local date `from`.
fn counts_since(actions: &[&Action], from: NaiveDate, now: DateTime<Utc>, zone: Tz) -> Counts {
    let due: Vec<&&Action> = actions.iter().filter(|a| is_due(a, now) && a.start_at.with_timezone(&zone).date_naive() >= from).collect();
    let done = due.iter().filter(|a| a.status == ActionStatus::Done).count();
    Counts { done: done as u32, due: due.len() as u32 }
}

/// The trend of goal `goal_id`: its first number field with a target in `targets`, else
/// its first number field, trackers taken by name; that field's last value per week of
/// `weeks` and its target. `None` when the goal has no tracker with a number field.
fn trend(goal_id: &str, rows: &Rows, weeks: &[NaiveDate], zone: Tz) -> Option<Trend> {
    let mut trackers: Vec<&Tracker> = rows.trackers.iter().filter(|t| t.goal_id == goal_id).collect();
    trackers.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    let numbers: Vec<(&Tracker, &crate::rows::TrackerField)> = trackers.iter().flat_map(|t| t.fields.iter().filter(|f| f.kind == FieldKind::Number).map(move |f| (*t, f))).collect();
    let targeted = |(t, f): &&(&Tracker, &crate::rows::TrackerField)| rows.targets.get(&t.id).is_some_and(|m| m.contains_key(&f.key));
    let &(tracker, field) = numbers.iter().find(targeted).or_else(|| numbers.first())?;
    let mut points: Vec<Option<(DateTime<Utc>, f64)>> = vec![None; weeks.len()];
    for e in rows.entries.iter().filter(|e| e.tracker_id == tracker.id) {
        let Some(value) = e.data.get(&field.key).and_then(serde_json::Value::as_f64) else { continue };
        let week = monday(e.logged_at.with_timezone(&zone).date_naive());
        let Some(i) = weeks.iter().position(|w| *w == week) else { continue };
        if points[i].is_none_or(|(at, _)| e.logged_at >= at) {
            points[i] = Some((e.logged_at, value));
        }
    }
    Some(Trend {
        tracker_id: tracker.id.clone(),
        label: field.label.clone(),
        unit: field.unit.clone(),
        points: points.into_iter().map(|p| p.map(|(_, v)| v)).collect(),
        target: rows.targets.get(&tracker.id).and_then(|m| m.get(&field.key)).copied(),
    })
}

/// The figures of one dashboard goal at `now` in `zone`.
fn goal_stats(goal: &Goal, rows: &Rows, weeks: &[NaiveDate], trend_weeks: &[NaiveDate], now: DateTime<Utc>, zone: Tz) -> GoalStats {
    let actions: Vec<&Action> = rows.actions.iter().filter(|a| a.goal_id == goal.id).collect();
    let due: Vec<&&Action> = actions.iter().filter(|a| is_due(a, now)).collect();
    let count = |s: ActionStatus| due.iter().filter(|a| a.status == s).count() as u32;
    let (done, missed, skipped) = (count(ActionStatus::Done), count(ActionStatus::Missed), count(ActionStatus::Skipped));
    let due = due.len() as u32;
    let ends: Vec<Option<DateTime<Utc>>> = rows.series.iter().filter(|s| s.goal_id == goal.id).map(|s| last_occurrence(s).ok().flatten()).collect();
    let today = now.with_timezone(&zone).date_naive();
    let (plan_end, finish) = projected_finish(&actions, &ends, due - done, today, zone);
    let deadline = goal.deadline.as_deref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    let (streak, best_streak) = streaks(&actions, now, zone);
    let month_start = today.with_day(1).expect("day 1 exists");
    GoalStats {
        goal_id: goal.id.clone(),
        done,
        due,
        missed,
        skipped,
        open_ended: ends.iter().any(Option::is_none),
        plan_end,
        finish,
        pace: pace(done, due, finish, deadline),
        streak,
        best_streak,
        week: counts_since(&actions, monday(today), now, zone),
        month: counts_since(&actions, month_start, now, zone),
        minutes: weekly_minutes(&actions, weeks, zone),
        trend: trend(&goal.id, rows, trend_weeks, zone),
    }
}

/// The dashboard at `now` in `zone`: every [`shown`] goal of `rows` in stable order
/// (created, then id) with its figures, plus the chart weeks. Rows of other goals are
/// ignored; a series whose rule fails to parse counts as unbounded.
pub fn dashboard(rows: &Rows, now: DateTime<Utc>, zone: Tz) -> Dashboard {
    let weeks = week_starts(now, zone, WEEKS);
    let trend_weeks = week_starts(now, zone, TREND_WEEKS);
    let mut goals: Vec<&Goal> = rows.goals.iter().filter(|g| shown(g)).collect();
    goals.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
    let goals: Vec<GoalStats> = goals.into_iter().map(|g| goal_stats(g, rows, &weeks, &trend_weeks, now, zone)).collect();
    assert!(goals.iter().all(|g| g.minutes.len() == WEEKS && g.done <= g.due));
    Dashboard { weeks, trend_weeks, goals }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rows::{Autonomy, EntrySource, TrackerField};
    use chrono::TimeZone;
    use serde_json::json;

    const BERLIN: Tz = chrono_tz::Europe::Berlin;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn goal(id: &str, status: GoalStatus, deadline: Option<&str>, created: DateTime<Utc>) -> Goal {
        Goal {
            id: id.into(),
            user_id: "u".into(),
            category_id: None,
            title: id.into(),
            context: None,
            deadline: deadline.map(Into::into),
            status,
            autonomy: Autonomy::Ask,
            created_at: created,
            reminders: true,
            use_calendars: false,
            run_limit_minutes: None,
            updated_at: String::new(),
            deleted: false,
        }
    }

    fn action(goal: &str, start: DateTime<Utc>, minutes: i64, status: ActionStatus) -> Action {
        Action {
            id: format!("{goal}-{}", start.timestamp()),
            user_id: "u".into(),
            goal_id: goal.into(),
            series_id: None,
            occurrence: None,
            title: "a".into(),
            notes: None,
            start_at: start,
            end_at: start + chrono::Duration::minutes(minutes),
            status,
            locked: false,
            done_at: None,
            updated_at: String::new(),
            deleted: false,
        }
    }

    fn series(goal: &str, rrule: &str) -> ActionSeries {
        ActionSeries {
            id: format!("s-{goal}"),
            user_id: "u".into(),
            goal_id: goal.into(),
            title: "s".into(),
            notes: None,
            rrule: rrule.into(),
            dtstart_local: "2026-11-02T07:00:00".into(),
            timezone: "Europe/Berlin".into(),
            duration_minutes: 30,
            updated_at: String::new(),
            deleted: false,
        }
    }

    fn refs(actions: &[Action]) -> Vec<&Action> {
        actions.iter().collect()
    }

    #[test]
    fn shown_hides_draft_archived_and_routines() {
        let t = at(2026, 11, 1, 9, 0);
        assert!(shown(&goal("g", GoalStatus::Active, None, t)));
        assert!(shown(&goal("g", GoalStatus::Paused, None, t)));
        assert!(shown(&goal("g", GoalStatus::Done, None, t)));
        assert!(!shown(&goal("g", GoalStatus::Draft, None, t)));
        assert!(!shown(&goal("g", GoalStatus::Archived, None, t)));
        assert!(!shown(&goal(&routines_id(), GoalStatus::Active, None, t)));
    }

    #[test]
    fn due_counts_ended_or_decided_actions() {
        let now = at(2026, 11, 23, 12, 0);
        assert!(is_due(&action("g", at(2026, 11, 23, 9, 0), 30, ActionStatus::Planned), now));
        assert!(!is_due(&action("g", at(2026, 11, 23, 11, 45), 30, ActionStatus::Planned), now));
        assert!(is_due(&action("g", at(2026, 11, 24, 9, 0), 30, ActionStatus::Done), now));
        assert!(!is_due(&action("g", at(2026, 11, 24, 9, 0), 30, ActionStatus::Planned), now));
    }

    #[test]
    fn pace_rules() {
        let d = Some(date(2027, 1, 31));
        assert_eq!(pace(17, 20, Some(date(2027, 1, 1)), d), Pace::OnTrack);
        assert_eq!(pace(16, 20, Some(date(2027, 1, 1)), d), Pace::Behind);
        assert_eq!(pace(20, 20, Some(date(2027, 2, 1)), d), Pace::Behind);
        assert_eq!(pace(20, 20, Some(date(2027, 1, 24)), d), Pace::Ahead);
        assert_eq!(pace(20, 20, Some(date(2027, 1, 25)), d), Pace::OnTrack);
        assert_eq!(pace(19, 20, Some(date(2027, 1, 1)), d), Pace::OnTrack);
        assert_eq!(pace(20, 20, Some(date(2026, 12, 1)), None), Pace::OnTrack);
        assert_eq!(pace(0, 0, None, d), Pace::OnTrack);
        assert_eq!(pace(1, 4, None, None), Pace::Behind);
    }

    #[test]
    fn bounded_series_last_occurrence_and_unbounded_none() {
        let counted = series("g", "FREQ=DAILY;COUNT=3");
        assert_eq!(last_occurrence(&counted).unwrap(), Some(at(2026, 11, 4, 6, 30)));
        let until = series("g", "FREQ=WEEKLY;UNTIL=20261117T000000Z");
        assert_eq!(last_occurrence(&until).unwrap(), Some(at(2026, 11, 16, 6, 30)));
        assert_eq!(last_occurrence(&series("g", "FREQ=DAILY")).unwrap(), None);
        assert!(last_occurrence(&series("g", "FREQ=HOURLY")).is_err());
    }

    #[test]
    fn finish_slips_by_owed_sessions_at_average_gap() {
        let actions: Vec<Action> = (0..5).map(|i| action("g", at(2026, 11, 2 + 2 * i, 9, 0), 30, ActionStatus::Done)).collect();
        let a = refs(&actions);
        assert_eq!(projected_finish(&a, &[], 0, date(2026, 11, 1), BERLIN), (Some(date(2026, 11, 10)), Some(date(2026, 11, 10))));
        assert_eq!(projected_finish(&a, &[], 3, date(2026, 11, 1), BERLIN), (Some(date(2026, 11, 10)), Some(date(2026, 11, 16))));
        assert_eq!(projected_finish(&a[..1], &[], 2, date(2026, 11, 1), BERLIN), (Some(date(2026, 11, 2)), Some(date(2026, 11, 4))));
        assert_eq!(projected_finish(&[], &[], 0, date(2026, 11, 1), BERLIN), (None, None));
        assert_eq!(projected_finish(&a, &[None], 0, date(2026, 11, 1), BERLIN), (None, None));
        let end = Some(at(2026, 12, 1, 8, 0));
        assert_eq!(projected_finish(&a, &[end], 1, date(2026, 11, 1), BERLIN), (Some(date(2026, 12, 1)), Some(date(2026, 12, 3))));
        assert_eq!(projected_finish(&a, &[], 3, date(2026, 11, 20), BERLIN), (Some(date(2026, 11, 10)), Some(date(2026, 11, 26))));
        assert_eq!(projected_finish(&a, &[], 0, date(2026, 11, 20), BERLIN), (Some(date(2026, 11, 10)), Some(date(2026, 11, 10))));
    }

    #[test]
    fn streak_skips_empty_and_skipped_days_and_open_today() {
        let now = at(2026, 11, 23, 8, 0);
        let actions = vec![
            action("g", at(2026, 11, 16, 9, 0), 30, ActionStatus::Missed),
            action("g", at(2026, 11, 17, 9, 0), 30, ActionStatus::Done),
            action("g", at(2026, 11, 18, 9, 0), 30, ActionStatus::Skipped),
            action("g", at(2026, 11, 20, 9, 0), 30, ActionStatus::Done),
            action("g", at(2026, 11, 22, 9, 0), 30, ActionStatus::Done),
            action("g", at(2026, 11, 22, 18, 0), 30, ActionStatus::Done),
            action("g", at(2026, 11, 23, 6, 0), 30, ActionStatus::Planned),
        ];
        assert_eq!(streaks(&refs(&actions), now, BERLIN), (3, 3));
        let mut broken = actions.clone();
        broken[5].status = ActionStatus::Missed;
        assert_eq!(streaks(&refs(&broken), now, BERLIN), (0, 2));
    }

    #[test]
    fn best_streak_is_longest_run() {
        let now = at(2026, 11, 23, 20, 0);
        let statuses = [true, true, true, true, false, true, true];
        let actions: Vec<Action> =
            statuses.iter().enumerate().map(|(i, ok)| action("g", at(2026, 11, 10 + i as u32, 9, 0), 30, if *ok { ActionStatus::Done } else { ActionStatus::Missed })).collect();
        assert_eq!(streaks(&refs(&actions), now, BERLIN), (2, 4));
        assert_eq!(streaks(&[], now, BERLIN), (0, 0));
    }

    #[test]
    fn weekly_minutes_buckets_by_local_week() {
        let now = at(2026, 11, 25, 12, 0);
        let weeks = week_starts(now, BERLIN, WEEKS);
        assert_eq!(weeks.first(), Some(&date(2026, 10, 19)));
        assert_eq!(weeks.last(), Some(&date(2026, 11, 23)));
        let actions = vec![
            action("g", at(2026, 11, 22, 23, 30), 45, ActionStatus::Done),
            action("g", at(2026, 11, 22, 22, 30), 20, ActionStatus::Done),
            action("g", at(2026, 11, 20, 9, 0), 60, ActionStatus::Missed),
            action("g", at(2026, 10, 1, 9, 0), 60, ActionStatus::Done),
        ];
        assert_eq!(weekly_minutes(&refs(&actions), &weeks, BERLIN), vec![0, 0, 0, 0, 20, 45]);
        let utc_weeks = week_starts(now, Tz::UTC, WEEKS);
        assert_eq!(weekly_minutes(&refs(&actions), &utc_weeks, Tz::UTC), vec![0, 0, 0, 0, 65, 0]);
    }

    #[test]
    fn dashboard_goal_order_completion_and_trend() {
        let now = at(2026, 11, 25, 12, 0);
        let goals = vec![
            goal("b", GoalStatus::Active, Some("2026-12-31"), at(2026, 11, 2, 9, 0)),
            goal("a", GoalStatus::Active, None, at(2026, 11, 1, 9, 0)),
            goal("draft", GoalStatus::Draft, None, at(2026, 10, 1, 9, 0)),
        ];
        let actions = vec![
            action("b", at(2026, 11, 3, 9, 0), 30, ActionStatus::Done),
            action("b", at(2026, 11, 24, 9, 0), 30, ActionStatus::Done),
            action("b", at(2026, 11, 25, 7, 0), 30, ActionStatus::Missed),
            action("b", at(2026, 11, 27, 9, 0), 30, ActionStatus::Planned),
            action("draft", at(2026, 11, 24, 9, 0), 30, ActionStatus::Done),
        ];
        let field = |key: &str, kind: FieldKind| TrackerField { key: key.into(), label: key.into(), kind, unit: Some("kg".into()), choices: vec![], health: None };
        let tracker = |id: &str, name: &str, fields: Vec<TrackerField>| Tracker {
            id: id.into(),
            user_id: "u".into(),
            goal_id: "b".into(),
            plugin_id: None,
            name: name.into(),
            fields,
            cadence: None,
            rules: vec![],
            updated_at: String::new(),
            deleted: false,
        };
        let trackers = vec![tracker("t0", "Arms", vec![field("cm", FieldKind::Number)]), tracker("t", "Body", vec![field("note", FieldKind::Text), field("weight", FieldKind::Number)])];
        let entry = |id: &str, t: DateTime<Utc>, v: f64| TrackerEntry {
            id: id.into(),
            user_id: "u".into(),
            tracker_id: "t".into(),
            data: json!({ "weight": v }).as_object().unwrap().clone(),
            source: EntrySource::Log,
            raw_text: None,
            logged_at: t,
            updated_at: String::new(),
            deleted: false,
        };
        let entries = vec![entry("e1", at(2026, 11, 24, 8, 0), 80.0), entry("e2", at(2026, 11, 25, 8, 0), 79.5), entry("e3", at(2026, 11, 10, 8, 0), 81.0)];
        let targets = BTreeMap::from([("t".to_string(), BTreeMap::from([("weight".to_string(), 75.0)]))]);
        let open = vec![series("a", "FREQ=DAILY")];
        let rows = Rows { goals: &goals, actions: &actions, series: &open, trackers: &trackers, entries: &entries, targets: &targets };
        let d = dashboard(&rows, now, BERLIN);
        assert_eq!(d.goals.iter().map(|g| g.goal_id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(d.trend_weeks.len(), TREND_WEEKS);
        let a = &d.goals[0];
        assert_eq!((a.done, a.due, a.finish, a.open_ended, a.pace, a.trend.is_none()), (0, 0, None, true, Pace::OnTrack, true));
        assert!(!d.goals[1].open_ended);
        let b = &d.goals[1];
        assert_eq!((b.done, b.due, b.missed, b.skipped), (2, 3, 1, 0));
        assert_eq!(b.week, Counts { done: 1, due: 2 });
        assert_eq!(b.month, Counts { done: 2, due: 3 });
        assert_eq!(b.pace, Pace::Behind);
        assert_eq!(b.minutes, vec![0, 0, 30, 0, 0, 30]);
        let trend = b.trend.as_ref().unwrap();
        assert_eq!((trend.label.as_str(), trend.unit.as_deref(), trend.target), ("weight", Some("kg"), Some(75.0)));
        assert_eq!(trend.points, vec![None, None, None, None, Some(81.0), None, Some(79.5)]);
    }
}
