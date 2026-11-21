use crate::model::{Action, ActionStatus, Goal, Slot};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};

/// A goal with its target date parsed. `target_date` is the last local day
/// the goal may hold work.
pub(crate) struct PGoal {
    pub id: String,
    pub target_date: NaiveDate,
}

/// An action with parsed UTC times. `original` holds the stored slot for
/// rows read from the input, None for rows created by the planner.
#[derive(Clone)]
pub(crate) struct PAction {
    pub id: String,
    pub goal_id: String,
    pub title: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub est_minutes: i64,
    pub status: ActionStatus,
    pub original: Option<Slot>,
}

/// Parses an ISO 8601 UTC instant; None when malformed.
pub(crate) fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text).ok().map(|t| t.with_timezone(&Utc))
}

/// Formats an instant the way the planner writes it, such as "2026-11-23T09:00:00Z".
pub(crate) fn format_time(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Parses a slot's times; None when either is malformed or end is not after start.
pub(crate) fn parse_slot(slot: &Slot) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let start = parse_time(&slot.start_at)?;
    let end = parse_time(&slot.end_at)?;
    (start < end).then_some((start, end))
}

/// Live goals with a parseable target date ("YYYY-MM-DD" or a full timestamp).
pub(crate) fn goals(goals: &[Goal]) -> Vec<PGoal> {
    goals
        .iter()
        .filter(|g| !g.deleted)
        .filter_map(|g| {
            let date = NaiveDate::parse_from_str(g.target_date.get(..10)?, "%Y-%m-%d").ok()?;
            Some(PGoal { id: g.id.clone(), target_date: date })
        })
        .collect()
}

/// Live actions with parseable times. Rows with malformed times are skipped.
pub(crate) fn actions(actions: &[Action]) -> Vec<PAction> {
    actions
        .iter()
        .filter(|a| !a.deleted)
        .filter_map(|a| {
            let original = Slot { start_at: a.start_at.clone(), end_at: a.end_at.clone(), est_minutes: a.est_minutes };
            let (start_at, end_at) = parse_slot(&original)?;
            Some(PAction {
                id: a.id.clone(),
                goal_id: a.goal_id.clone(),
                title: a.title.clone(),
                start_at,
                end_at,
                est_minutes: a.est_minutes,
                status: a.status,
                original: Some(original),
            })
        })
        .collect()
}
