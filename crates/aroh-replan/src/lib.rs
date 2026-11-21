mod model;
mod plan;
mod time;
mod validate;

pub use model::*;
pub use plan::plan;
pub use validate::validate;

use chrono::{DateTime, Duration, Utc};

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
/// create appends a planned row. Unknown ids are skipped.
pub fn apply(actions: &[Action], diff: &[DiffOp]) -> Vec<Action> {
    let mut out = actions.to_vec();
    for op in diff {
        match (op.op, op.after) {
            (Op::Create, Some(slot)) => {
                let goal_id = op.goal_id.clone().unwrap_or_default();
                let seq = out.iter().filter(|a| a.goal_id == goal_id).map(|a| a.seq + 1).max().unwrap_or(0);
                out.push(Action {
                    id: op.action_id.clone(),
                    goal_id,
                    title: op.title.clone().unwrap_or_default(),
                    notes: None,
                    start_at: slot.start_at,
                    end_at: slot.end_at,
                    est_minutes: slot.est_minutes,
                    status: ActionStatus::Planned,
                    seq,
                    done_at: None,
                    updated_at: String::new(),
                    deleted: false,
                });
            }
            (Op::Move | Op::Resize, Some(slot)) => set_slot(&mut out, &op.action_id, slot),
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
/// creates are removed, deletes revived, slots set back to `before`.
pub fn undo(actions: &[Action], diff: &[DiffOp]) -> Vec<Action> {
    let mut out = actions.to_vec();
    for op in diff.iter().rev() {
        match (op.op, op.before) {
            (Op::Create, _) => out.retain(|a| a.id != op.action_id),
            (Op::Move | Op::Resize, Some(slot)) => set_slot(&mut out, &op.action_id, slot),
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

/// Sets start, end and estimate of the action with `id`, if present.
fn set_slot(actions: &mut [Action], id: &str, slot: Slot) {
    if let Some(a) = actions.iter_mut().find(|a| a.id == id) {
        a.start_at = slot.start_at;
        a.end_at = slot.end_at;
        a.est_minutes = slot.est_minutes;
    }
}

/// Ids of live planned actions whose end plus `grace_min` minutes is
/// before `now`, sorted. Pure: the caller writes the status change.
pub fn detect_missed(actions: &[Action], now: DateTime<Utc>, grace_min: i64) -> Vec<String> {
    let mut ids: Vec<String> = actions
        .iter()
        .filter(|a| a.status == ActionStatus::Planned && !a.deleted && a.end_at + Duration::minutes(grace_min) < now)
        .map(|a| a.id.clone())
        .collect();
    ids.sort();
    ids
}
