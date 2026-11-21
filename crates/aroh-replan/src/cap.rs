use crate::model::{ActionStatus, DiffOp, PlanInput, Violation};
use crate::parsed::{self, PAction};
use crate::time::local_date;
use chrono::NaiveDate;
use std::collections::{BTreeMap, BTreeSet};

/// Extra minutes any day may take on top of a goal's usual daily load.
const EXTRA_MIN: i64 = 60;

/// Most minutes of one goal a local day may hold after a replan: the goal's
/// usual daily load plus 50% or plus EXTRA_MIN, whichever is larger, and
/// never less than what the day already held as planned work.
pub(crate) struct Caps {
    goal_cap: BTreeMap<String, i64>,
    planned: BTreeMap<(String, NaiveDate), i64>,
}

impl Caps {
    /// Computes caps from every live action of the input: the usual load is
    /// the mean minutes per local day that holds any of the goal's actions.
    pub(crate) fn new(input: &PlanInput) -> Caps {
        let missed: BTreeSet<&str> = input.missed_ids.iter().map(String::as_str).collect();
        let mut per_day: BTreeMap<(String, NaiveDate), i64> = BTreeMap::new();
        let mut planned: BTreeMap<(String, NaiveDate), i64> = BTreeMap::new();
        for action in parsed::actions(&input.actions) {
            let key = (action.goal_id.clone(), local_date(action.start_at, &input.availability));
            let minutes = span_minutes(&action);
            *per_day.entry(key.clone()).or_default() += minutes;
            if action.status == ActionStatus::Planned && !missed.contains(action.id.as_str()) {
                *planned.entry(key).or_default() += minutes;
            }
        }
        let mut totals: BTreeMap<String, (i64, i64)> = BTreeMap::new();
        for ((goal_id, _), minutes) in &per_day {
            let entry = totals.entry(goal_id.clone()).or_default();
            entry.0 += minutes;
            entry.1 += 1;
        }
        let goal_cap = totals
            .into_iter()
            .map(|(goal_id, (total, days))| {
                assert!(days > 0);
                let usual = total / days;
                (goal_id, (usual * 3 / 2).max(usual + EXTRA_MIN))
            })
            .collect();
        Caps { goal_cap, planned }
    }

    /// Daily cap of the goal before any per-day allowance; EXTRA_MIN for a goal with no actions.
    pub(crate) fn goal_cap(&self, goal_id: &str) -> i64 {
        self.goal_cap.get(goal_id).copied().unwrap_or(EXTRA_MIN)
    }

    /// Minutes of the goal that `day` may hold after the replan.
    pub(crate) fn allowed(&self, goal_id: &str, day: NaiveDate) -> i64 {
        let held = self.planned.get(&(goal_id.to_string(), day)).copied().unwrap_or(0);
        self.goal_cap(goal_id).max(held)
    }

    /// Caps of every live goal keyed by goal id, for the replan agent prompt.
    pub(crate) fn all(&self, input: &PlanInput) -> BTreeMap<String, i64> {
        input.goals.iter().filter(|g| !g.deleted).map(|g| (g.id.clone(), self.goal_cap(&g.id))).collect()
    }
}

/// Length of an action's slot in minutes.
pub(crate) fn span_minutes(action: &PAction) -> i64 {
    (action.end_at - action.start_at).num_minutes()
}

/// One OverDailyCap per goal and local day that `diff` places work on and
/// that holds more of the goal than Caps::allowed after applying it.
pub(crate) fn over_cap(input: &PlanInput, diff: &[DiffOp]) -> Vec<Violation> {
    let caps = Caps::new(input);
    let missed: BTreeSet<&str> = input.missed_ids.iter().map(String::as_str).collect();
    let touched: BTreeSet<&str> = diff.iter().filter(|d| d.after.is_some()).map(|d| d.action_id.as_str()).collect();
    let applied = parsed::actions(&crate::apply(&input.actions, diff));
    let live = applied
        .iter()
        .filter(|a| a.status == ActionStatus::Planned && (!missed.contains(a.id.as_str()) || touched.contains(a.id.as_str())));
    let mut load: BTreeMap<(String, NaiveDate), i64> = BTreeMap::new();
    for action in live {
        *load.entry((action.goal_id.clone(), local_date(action.start_at, &input.availability))).or_default() += span_minutes(action);
    }
    let days: BTreeSet<(String, NaiveDate)> = applied
        .iter()
        .filter(|a| touched.contains(a.id.as_str()))
        .map(|a| (a.goal_id.clone(), local_date(a.start_at, &input.availability)))
        .collect();
    days.into_iter()
        .filter(|key| load.get(key).copied().unwrap_or(0) > caps.allowed(&key.0, key.1))
        .map(|(goal_id, day)| Violation::OverDailyCap { goal_id, date: day.format("%Y-%m-%d").to_string() })
        .collect()
}
