//! The overlap check of planning runs (Q88): a plan whose sessions clash with each other or
//! with a busy entry goes back to the planner with every clash named, at most
//! [`MAX_RETRIES`] times, then the run fails with a plain reason the app shows as is.

use crate::evals::{self, Slot};
use chrono::Datelike;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use troupekit_tools::{typed_tool, Tool, ToolContext, ToolError, ToolOutput};

/// Times a clashing plan goes back to the planner before the run fails.
pub const MAX_RETRIES: i64 = 2;

/// Start of the failure reason the app shows as is, without the node and tool around it.
pub const PLAIN_REASON: &str = "Could not fit sessions around your calendar";

/// Clashes named to the planner in one round; the rest wait for the next check.
const MAX_NAMED: usize = 10;

/// The `overlaps` value a planning run starts with: nothing checked yet.
pub fn initial() -> Value {
    json!({"clashes": [], "tries": 0})
}

/// One clash: a plan `slot` overlapping `other`, a busy entry when `busy`, else another session.
#[derive(Debug, Clone, PartialEq)]
pub struct Clash {
    pub slot: Slot,
    pub other: Slot,
    pub busy: bool,
}

/// Every clash of `all` (the plan's slots) with each other, then with `busy`, in start order.
/// Busy entries are never checked against each other. Empty when nothing clashes.
pub fn clashes(mut all: Vec<Slot>, busy: &[Slot]) -> Vec<Clash> {
    all.sort_by(|a, b| (a.start, a.end).cmp(&(b.start, b.end)));
    let mut out: Vec<Clash> = all.windows(2).filter(|p| p[1].start < p[0].end).map(|p| Clash { slot: p[1].clone(), other: p[0].clone(), busy: false }).collect();
    for slot in &all {
        if let Some(b) = busy.iter().find(|b| slot.start < b.end && b.start < slot.end) {
            out.push(Clash { slot: slot.clone(), other: b.clone(), busy: true });
        }
    }
    out
}

/// The planner's feedback: one line per session, clashing entry, weekday and hour, naming the
/// first date with both times and how many more dates of that pattern clash. At most
/// [`MAX_NAMED`] lines, in the order the clashes come.
pub fn feedback(found: &[Clash]) -> Vec<String> {
    let mut groups: BTreeMap<(String, String, String, String), (usize, &Clash)> = BTreeMap::new();
    let mut order = Vec::new();
    for c in found {
        let key = (c.slot.title.clone(), c.other.title.clone(), c.slot.start.weekday().to_string(), c.slot.start.format("%H:%M").to_string());
        match groups.get_mut(&key) {
            Some((n, _)) => *n += 1,
            None => {
                groups.insert(key.clone(), (1, c));
                order.push(key);
            }
        }
    }
    order
        .iter()
        .take(MAX_NAMED)
        .map(|key| {
            let (n, c) = groups[key];
            let hm = |s: &Slot| format!("{}-{}", s.start.format("%H:%M"), s.end.format("%H:%M"));
            let what = if c.busy { "busy " } else { "" };
            let more = if n > 1 { format!(", and on {} more {}s", n - 1, c.slot.start.format("%A")) } else { String::new() };
            format!("`{}` on {} {} overlaps {what}`{}` ({}){more}", c.slot.title, c.slot.start.format("%a %Y-%m-%d"), hm(&c.slot), c.other.title, hm(&c.other))
        })
        .collect()
}

/// The failure reason once the planner ran out of retries: the first clash, by title only.
pub fn plain_reason(first: &Clash) -> String {
    format!("{PLAIN_REASON}: {} clashes with {}", first.slot.title, first.other.title)
}

/// Checks `plan` (its `items` expanded over `days`) against `busy`, after the previous check
/// `prev`. Returns the new `overlaps` value: no clashes and tries back at 0, or the clashes
/// named for the planner and one more try. Errors with [`plain_reason`] once `prev` used
/// [`MAX_RETRIES`], or with why the plan does not expand.
pub fn check(plan: &Value, prev: &Value, busy: &[Slot], days: u32) -> Result<Value, ToolError> {
    let items = json!({"items": plan.get("items").cloned().unwrap_or(Value::Array(Vec::new()))});
    let slots = evals::path_slots_within(&items, &[json!("items")], days).map_err(ToolError::InvalidArgs)?;
    let found = clashes(slots, busy);
    let Some(first) = found.first() else { return Ok(initial()) };
    let tries = prev.get("tries").and_then(Value::as_i64).unwrap_or(0);
    assert!(tries >= 0, "tries never go below zero");
    if tries >= MAX_RETRIES {
        return Err(ToolError::Blocked(plain_reason(first)));
    }
    Ok(json!({"clashes": feedback(&found), "tries": tries + 1}))
}

/// Router condition `overlap_gate()`: `clash` when the last check named clashes, else `clear`.
pub fn overlap_gate(state: &Map<String, Value>, _args: &[Value]) -> Result<String, String> {
    let list = state.get("overlaps").and_then(|o| o.get("clashes")).and_then(Value::as_array).ok_or("overlaps has no clashes")?;
    Ok(if list.is_empty() { "clear" } else { "clash" }.into())
}

/// The `check_overlap` tool input, read from the planning run state.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CheckArgs {
    pub plan: Value,
    pub overlaps: Value,
}

/// Builds the `check_overlap` tool of one run, closed over the run's `busy` slots and the
/// horizon `days` the `no_overlap` eval used, so both see the calendar alike.
/// Returns the tool; its output is the new `overlaps` value as JSON text.
pub fn tool(busy: Vec<Slot>, days: u32) -> Arc<dyn Tool> {
    let busy = Arc::new(busy);
    Arc::new(typed_tool("check_overlap", "Check that a draft plan's sessions overlap nothing on the owner's calendar.", move |a: CheckArgs, _ctx: ToolContext| {
        let busy = busy.clone();
        async move { check(&a.plan, &a.overlaps, &busy, days).map(|v| ToolOutput::text(v.to_string())) }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> chrono::NaiveDateTime {
        evals::parse_local(text).unwrap()
    }

    fn full_body() -> Vec<Slot> {
        ["2026-12-14T19:00:00", "2026-12-21T19:00:00", "2026-12-28T19:00:00"]
            .iter()
            .map(|s| Slot { title: "Gym: Full body A (legs, chest, back)".into(), start: at(s), end: at(s) + chrono::Duration::minutes(60) })
            .collect()
    }

    fn plan() -> Value {
        json!({"summary": "Gym", "timezone": "Europe/Berlin", "items": [
            {"title": "Gym: Upper body A", "start": "2026-11-30T19:00:00", "minutes": 60, "rrule": "FREQ=WEEKLY;BYDAY=MO"},
            {"title": "Gym: Lower body A", "start": "2026-12-02T19:00:00", "minutes": 60, "rrule": "FREQ=WEEKLY;BYDAY=WE"}
        ]})
    }

    #[test]
    fn a_clash_with_another_goals_series_goes_back_twice_then_fails_plainly() {
        let first = check(&plan(), &initial(), &full_body(), 60).unwrap();
        assert_eq!(first["tries"], 1);
        assert_eq!(
            first["clashes"],
            json!(["`Gym: Upper body A` on Mon 2026-12-14 19:00-20:00 overlaps busy `Gym: Full body A (legs, chest, back)` (19:00-20:00), and on 2 more Mondays"])
        );
        let second = check(&plan(), &first, &full_body(), 60).unwrap();
        assert_eq!(second["tries"], 2);
        let Err(ToolError::Blocked(reason)) = check(&plan(), &second, &full_body(), 60) else { panic!("the third clash fails the run") };
        assert_eq!(reason, "Could not fit sessions around your calendar: Gym: Upper body A clashes with Gym: Full body A (legs, chest, back)");
    }

    #[test]
    fn a_plan_moved_off_the_busy_slot_is_clear_and_resets_the_tries() {
        let mut moved = plan();
        moved["items"][0]["start"] = json!("2026-11-30T07:00:00");
        let clear = check(&moved, &json!({"clashes": ["x"], "tries": 2}), &full_body(), 60).unwrap();
        assert_eq!(clear, initial());
        let mut state = Map::new();
        state.insert("overlaps".into(), clear);
        assert_eq!(overlap_gate(&state, &[]).unwrap(), "clear");
        state.insert("overlaps".into(), json!({"clashes": ["x"], "tries": 1}));
        assert_eq!(overlap_gate(&state, &[]).unwrap(), "clash");
    }

    #[test]
    fn sessions_of_one_plan_that_overlap_each_other_are_named_too() {
        let both = json!({"items": [
            {"title": "Run", "start": "2026-12-01T18:00:00", "minutes": 60},
            {"title": "Swim", "start": "2026-12-01T18:30:00", "minutes": 60}
        ]});
        let found = check(&both, &initial(), &[], 60).unwrap();
        assert_eq!(found["clashes"], json!(["`Swim` on Tue 2026-12-01 18:30-19:30 overlaps `Run` (18:00-19:00)"]));
    }

    #[test]
    fn clashes_past_the_horizon_are_not_checked_and_names_are_capped() {
        assert_eq!(check(&plan(), &initial(), &full_body(), 14).unwrap(), initial());
        let many: Vec<Clash> = (0..15)
            .map(|i| {
                let s = Slot { title: format!("S{i}"), start: at("2026-12-01T18:00:00"), end: at("2026-12-01T19:00:00") };
                Clash { slot: s.clone(), other: s, busy: true }
            })
            .collect();
        assert_eq!(feedback(&many).len(), MAX_NAMED);
    }
}
