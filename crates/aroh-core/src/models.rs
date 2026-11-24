//! Row structs for every table in the shared data model, matching docs in the MVP contract.

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};

macro_rules! text_enum {
    ($name:ident, $field:literal, { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[doc = concat!("Allowed values of `", $field, "`.")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name {
            $(#[serde(rename = $text)] $variant),+
        }

        impl $name {
            /// Returns the stored text form.
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }

            /// Parses the stored text form.
            pub fn parse(text: &str) -> crate::Result<Self> {
                match text {
                    $($text => Ok(Self::$variant),)+
                    other => Err(crate::Error::InvalidEnum { field: $field, value: other.chars().take(64).collect() }),
                }
            }
        }

        impl ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::from(self.as_str()))
            }
        }

        impl FromSql for $name {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                Self::parse(value.as_str()?).map_err(|e| FromSqlError::Other(Box::new(e)))
            }
        }
    };
}

text_enum!(GoalKind, "goals.kind", { Course => "course", Habit => "habit", Custom => "custom" });
text_enum!(GoalStatus, "goals.status", { Draft => "draft", Active => "active", Done => "done", Archived => "archived" });
text_enum!(ActionStatus, "actions.status", { Planned => "planned", Done => "done", Missed => "missed", Skipped => "skipped" });
text_enum!(PlanStatus, "plans.status", { Proposed => "proposed", Approved => "approved", Rejected => "rejected" });
text_enum!(ReplanSize, "replans.size", { Small => "small", Big => "big" });
text_enum!(ReplanStatus, "replans.status", { Proposed => "proposed", Applied => "applied", Rejected => "rejected", Undone => "undone" });
text_enum!(TrackerKind, "tracker_entries.kind", { Food => "food", Water => "water", Workout => "workout", Other => "other" });
text_enum!(DiffOp, "diff.op", { Move => "move", Create => "create", Delete => "delete", Resize => "resize" });

/// A goal row. Dates are UTC ISO 8601 strings.
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
    pub updated_at: String,
    pub deleted: bool,
}

/// An action row: one scheduled block of work toward a goal.
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
    pub updated_at: String,
    pub deleted: bool,
}

/// A plan row: one proposed or approved version of a goal's actions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub goal_id: String,
    pub version: i64,
    pub status: PlanStatus,
    pub summary: Option<String>,
    pub updated_at: String,
    pub deleted: bool,
}

/// A replan row. `diff_json` holds a JSON array of [`DiffEntry`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Replan {
    pub id: String,
    pub trigger_action_id: Option<String>,
    pub diff_json: String,
    pub size: ReplanSize,
    pub status: ReplanStatus,
    pub created_at: String,
    pub updated_at: String,
    pub deleted: bool,
}

/// Time slot of an action before or after a replan op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffSlot {
    pub start_at: String,
    pub end_at: String,
    pub est_minutes: i64,
}

/// One entry of a replan diff. `before` is null for create, `after` is null for delete.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffEntry {
    pub action_id: String,
    pub op: DiffOp,
    pub before: Option<DiffSlot>,
    pub after: Option<DiffSlot>,
}

/// A tracker entry row: food, water, workout or other intake.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackerEntry {
    pub id: String,
    pub kind: TrackerKind,
    pub amount: Option<f64>,
    pub unit: Option<String>,
    pub note: Option<String>,
    pub raw_text: Option<String>,
    pub logged_at: String,
    pub updated_at: String,
    pub deleted: bool,
}

/// A YAML plugin row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub yaml: String,
    pub enabled: bool,
    pub updated_at: String,
    pub deleted: bool,
}

/// A key/value settings row. Never holds secrets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}
