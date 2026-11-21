use crate::model::{Action, ActionStatus, DiffOp, Op, PlanInput, Slot, Violation};
use crate::time::{deadline, inside_window};
use std::collections::{BTreeMap, BTreeSet};

/// Checks a diff against the current actions, independent of plan():
/// ops match current rows, new slots are in the future, inside one window,
/// before the goal deadline and clear of every planned action.
/// Returns every violation found, or Ok when there are none.
pub fn validate(input: &PlanInput, diff: &[DiffOp]) -> Result<(), Vec<Violation>> {
    let mut violations = Vec::new();
    let actions: BTreeMap<&str, &Action> =
        input.actions.iter().filter(|a| !a.deleted).map(|a| (a.id.as_str(), a)).collect();
    let goals: BTreeMap<&str, _> = input.goals.iter().filter(|g| !g.deleted).map(|g| (g.id.as_str(), g)).collect();
    let missed: BTreeSet<&str> = input.missed_ids.iter().map(String::as_str).collect();
    let mut seen = BTreeSet::new();

    for op in diff {
        let id = op.action_id.clone();
        if !seen.insert(op.action_id.as_str()) {
            violations.push(Violation::DuplicateAction { action_id: id.clone() });
        }
        let shape_ok = match op.op {
            Op::Create => op.before.is_none() && op.after.is_some() && op.goal_id.is_some(),
            Op::Move => op.before.is_some() && op.after.is_some() && op.before.map(|s| s.est_minutes) == op.after.map(|s| s.est_minutes),
            Op::Resize => op.before.is_some() && op.after.is_some(),
            Op::Delete => op.before.is_some() && op.after.is_none(),
        };
        let end_ok = op.after.map_or(true, |s| s.start_at < s.end_at && s.est_minutes >= 0);
        if !shape_ok || !end_ok || missed.contains(op.action_id.as_str()) {
            violations.push(Violation::BadShape { action_id: id.clone() });
            continue;
        }
        let goal_id = match (op.op, actions.get(op.action_id.as_str())) {
            (Op::Create, None) => op.goal_id.clone().expect("shape checked"),
            (Op::Create, Some(_)) => {
                violations.push(Violation::DuplicateAction { action_id: id.clone() });
                continue;
            }
            (_, None) => {
                violations.push(Violation::UnknownAction { action_id: id.clone() });
                continue;
            }
            (_, Some(current)) => {
                let slot = Slot { start_at: current.start_at, end_at: current.end_at, est_minutes: current.est_minutes };
                if op.before != Some(slot) {
                    violations.push(Violation::BeforeMismatch { action_id: id.clone() });
                }
                current.goal_id.clone()
            }
        };
        let Some(goal) = goals.get(goal_id.as_str()) else {
            violations.push(Violation::UnknownGoal { action_id: id.clone() });
            continue;
        };
        if let Some(after) = op.after {
            if after.start_at < input.now {
                violations.push(Violation::InPast { action_id: id.clone() });
            }
            if !inside_window(after.start_at, after.end_at, &input.availability) {
                violations.push(Violation::OutsideWindow { action_id: id.clone() });
            }
            if after.end_at > deadline(goal, &input.availability) {
                violations.push(Violation::AfterDeadline { action_id: id.clone() });
            }
        }
    }
    if violations.is_empty() {
        violations.extend(overlaps(input, diff, &missed));
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

/// Overlaps after applying `diff`, each pair reported once and only when
/// at least one side was placed by the diff.
fn overlaps(input: &PlanInput, diff: &[DiffOp], missed: &BTreeSet<&str>) -> Vec<Violation> {
    let touched: BTreeSet<&str> = diff.iter().filter(|d| d.after.is_some()).map(|d| d.action_id.as_str()).collect();
    let applied = crate::apply(&input.actions, diff);
    let live: Vec<&Action> = applied
        .iter()
        .filter(|a| a.status == ActionStatus::Planned && !a.deleted && !missed.contains(a.id.as_str()))
        .collect();
    let mut found = Vec::new();
    for a in live.iter().filter(|a| touched.contains(a.id.as_str())) {
        for b in &live {
            let both = touched.contains(b.id.as_str());
            if a.id == b.id || (both && b.id < a.id) {
                continue;
            }
            if a.start_at < b.end_at && b.start_at < a.end_at {
                found.push(Violation::Overlap { action_id: a.id.clone(), other_id: b.id.clone() });
            }
        }
    }
    found
}
