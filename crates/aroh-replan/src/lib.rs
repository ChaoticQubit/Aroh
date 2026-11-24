mod cap;
mod model;
mod parsed;
mod plan;
mod time;
mod validate;

pub use model::*;
pub use plan::plan;
pub use validate::validate;

use chrono::{DateTime, Duration, Utc};
use parsed::parse_time;
mod agent;
pub use agent::*;

/// Small iff at most 3 ops and every op is a move or resize; else Big.
pub fn classify(diff: &[DiffOp]) -> Size {
    let light = diff.iter().all(|d| matches!(d.op, Op::Move | Op::Resize));
    if diff.len() <= 3 && light {
        Size::Small
    } else {
        Size::Big
    }
}

/// Returns `actions` with `diff` applied. Delete tombstones the row,
/// create appends a planned row, a revive op makes its action planned.
/// Unknown ids are skipped.
pub fn apply(actions: &[Action], diff: &[DiffOp]) -> Vec<Action> {
    let mut out = actions.to_vec();
    for op in diff {
        match (op.op, &op.after) {
            (Op::Create, Some(slot)) if !out.iter().any(|a| a.id == op.action_id) => {
                let goal_id = op.goal_id.clone().unwrap_or_default();
                let seq = out.iter().filter(|a| a.goal_id == goal_id).map(|a| a.seq + 1).max().unwrap_or(0);
                out.push(Action {
                    id: op.action_id.clone(),
                    goal_id,
                    title: op.title.clone().unwrap_or_default(),
                    notes: None,
                    start_at: slot.start_at.clone(),
                    end_at: slot.end_at.clone(),
                    est_minutes: slot.est_minutes,
                    status: ActionStatus::Planned,
                    seq,
                    done_at: None,
                    updated_at: String::new(),
                    deleted: false,
                });
            }
            (Op::Move | Op::Resize, Some(slot)) => set_slot(&mut out, &op.action_id, slot, op.revive.then_some(ActionStatus::Planned)),
            (Op::Delete, _) => {
                if let Some(a) = out.iter_mut().find(|a| a.id == op.action_id) {
                    a.deleted = true;
                }
            }
            _ => {}
        }
    }
    out
}

/// Reverts `diff` on actions it was applied to: ops run in reverse,
/// creates are removed, deletes revived, slots set back to `before` and
/// revived actions marked missed again.
pub fn undo(actions: &[Action], diff: &[DiffOp]) -> Vec<Action> {
    let mut out = actions.to_vec();
    for op in diff.iter().rev() {
        match (op.op, &op.before) {
            (Op::Create, _) => out.retain(|a| a.id != op.action_id),
            (Op::Move | Op::Resize, Some(slot)) => set_slot(&mut out, &op.action_id, slot, op.revive.then_some(ActionStatus::Missed)),
            (Op::Delete, _) => {
                if let Some(a) = out.iter_mut().find(|a| a.id == op.action_id) {
                    a.deleted = false;
                }
            }
            _ => {}
        }
    }
    out
}

/// Sets start, end and estimate of the action with `id`, and its status
/// when `status` is Some, if the action is present.
fn set_slot(actions: &mut [Action], id: &str, slot: &Slot, status: Option<ActionStatus>) {
    if let Some(a) = actions.iter_mut().find(|a| a.id == id) {
        a.start_at = slot.start_at.clone();
        a.end_at = slot.end_at.clone();
        a.est_minutes = slot.est_minutes;
        a.status = status.unwrap_or(a.status);
    }
}

/// Ids of live planned actions whose parseable end plus `grace_min` minutes is
/// before `now`, sorted. Pure: the caller writes the status change.
pub fn detect_missed(actions: &[Action], now: DateTime<Utc>, grace_min: i64) -> Vec<String> {
    let mut ids: Vec<String> = actions
        .iter()
        .filter(|a| a.status == ActionStatus::Planned && !a.deleted)
        .filter(|a| parse_time(&a.end_at).is_some_and(|end| end + Duration::minutes(grace_min) < now))
        .map(|a| a.id.clone())
        .collect();
    ids.sort();
    ids
}
