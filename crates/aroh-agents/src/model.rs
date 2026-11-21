use serde::{Deserialize, Serialize};

/// Kind of a goal, as stored in `goals.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GoalKind {
    Course,
    Habit,
    Custom,
}

/// Lifecycle of a goal, as stored in `goals.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GoalStatus {
    Draft,
    Active,
    Done,
    Archived,
}

/// State of a calendar action, as stored in `actions.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionStatus {
    Planned,
    Done,
    Missed,
    Skipped,
}

/// State of a plan, as stored in `plans.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanStatus {
    Proposed,
    Approved,
    Rejected,
}

/// One `goals` row. Mirrors the mvp-contract until `aroh_core::Goal` lands.
/// `target_date` is a UTC ISO 8601 date (`YYYY-MM-DD`) or date-time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub title: String,
    pub kind: GoalKind,
    pub source_url: Option<String>,
    pub target_date: String,
    pub status: GoalStatus,
    pub answers_json: Option<String>,
    pub created_at: String,
}

/// One `actions` row. Times are UTC ISO 8601 (RFC 3339) strings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub goal_id: String,
    pub title: String,
    pub notes: Option<String>,
    pub start_at: String,
    pub end_at: String,
    pub est_minutes: i64,
    pub status: ActionStatus,
    pub seq: i64,
    pub done_at: Option<String>,
}

/// One `plans` row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub goal_id: String,
    pub version: i64,
    pub status: PlanStatus,
    pub summary: String,
}
