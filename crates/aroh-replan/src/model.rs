use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use aroh_core::models::{Action, ActionStatus, DiffOp as Op, DiffSlot as Slot, Goal, GoalKind, GoalStatus};

/// One entry of `replans.diff_json`. `before` is null for create, `after`
/// is null for delete. `goal_id` and `title` are set only on create, so
/// apply can build the new action row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffOp {
    pub action_id: String,
    pub op: Op,
    pub before: Option<Slot>,
    pub after: Option<Slot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Minutes since local midnight, serialized as "HH:MM" (00:00 to 24:00).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct HhMm(pub u16);

impl HhMm {
    /// Parses "HH:MM" into minutes since midnight; None when malformed or past 24:00.
    pub fn parse(text: &str) -> Option<HhMm> {
        let (h, m) = text.split_once(':')?;
        if h.len() != 2 || m.len() != 2 {
            return None;
        }
        let h: u16 = h.parse().ok()?;
        let m: u16 = m.parse().ok()?;
        if m >= 60 || h * 60 + m > 24 * 60 {
            return None;
        }
        Some(HhMm(h * 60 + m))
    }
}

impl Serialize for HhMm {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("{:02}:{:02}", self.0 / 60, self.0 % 60))
    }
}

impl<'de> Deserialize<'de> for HhMm {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<HhMm, D::Error> {
        let text = String::deserialize(d)?;
        HhMm::parse(&text).ok_or_else(|| serde::de::Error::custom("expected HH:MM"))
    }
}

/// One weekly free window in local time. `weekday` 0 is Sunday, like JS getDay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub weekday: u8,
    pub start: HhMm,
    pub end: HhMm,
}

/// Weekly availability plus the local UTC offset in minutes.
/// shortcut: fixed offset ignores DST, swap to an IANA zone later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Availability {
    pub windows: Vec<Window>,
    pub utc_offset_minutes: i32,
}

/// Everything the planner sees: all goals, all actions, availability,
/// the current instant and the ids of the missed actions to recover.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanInput {
    pub goals: Vec<Goal>,
    pub actions: Vec<Action>,
    pub availability: Availability,
    pub now: DateTime<Utc>,
    pub missed_ids: Vec<String>,
}

/// Minutes of a goal's missed work that found no room before its deadline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Infeasible {
    pub goal_id: String,
    pub unplaced_minutes: i64,
}

/// Planner output: the diff to apply and what could not be placed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub diff: Vec<DiffOp>,
    #[serde(default)]
    pub infeasible: Vec<Infeasible>,
}

/// Replan size that decides auto-apply (Small) or owner review (Big).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Size {
    Small,
    Big,
}

/// Reason validate() rejects a diff. Each names the action ids involved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Violation {
    UnknownAction { action_id: String },
    UnknownGoal { action_id: String },
    DuplicateAction { action_id: String },
    BeforeMismatch { action_id: String },
    BadShape { action_id: String },
    InPast { action_id: String },
    OutsideWindow { action_id: String },
    AfterDeadline { action_id: String },
    Overlap { action_id: String, other_id: String },
    BadJson { message: String },
}
