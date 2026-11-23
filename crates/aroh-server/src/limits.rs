//! Plans never exceed the owner's daily minutes (Q64): the check after the
//! plan node, its router condition and the limits card it builds.

use crate::evals::{self, Slot};
use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use troupekit_tools::{typed_tool, Tool, ToolContext, ToolError, ToolOutput};

/// Question id the intake uses for the owner's minutes a day.
/// The limit check reads the answer to it before any other source.
/// Number kind, unit minutes per day.
pub const DAILY_MINUTES_ID: &str = "daily_minutes";

/// Profile topic that keeps the owner's minutes a day across goals (Q62).
/// The limit check reads it when this run's intake did not ask.
/// The fact text ends with the number, such as `Free minutes a day: 20`.
pub const DAILY_MINUTES_TOPIC: &str = "daily_minutes";

/// The owner's daily minutes for a plan check, from `state`: a raise picked
/// on the limits card, then the intake answer, then the profile fact, then
/// the minutes the plan says it used. Returns `None` when no source gives one.
pub fn daily_cap(state: &Map<String, Value>) -> Option<i64> {
    let raised = (choice(state) == Some(Choice::Raise)).then(|| answer(state, "limit_answers", NEW_MINUTES_ID)).flatten();
    let answered = answer(state, "answers", DAILY_MINUTES_ID);
    let known = state
        .get("profile")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|f| f.get("topic").and_then(Value::as_str) == Some(DAILY_MINUTES_TOPIC))
        .and_then(|f| f.get("fact").and_then(Value::as_str))
        .and_then(last_number);
    let planned = state.get("plan").and_then(|p| p.get("daily_minutes")).and_then(Value::as_i64);
    raised.or(answered).or(known).or(planned).filter(|m| (1..=MAX_DAILY_MINUTES).contains(m))
}

/// Checks `state`'s draft plan against the daily cap and the goal's target
/// date. Returns the `limits` state value: `fits`, `tries`, the `problems`
/// the planner must fix and, when it does not fit, the card's questions.
pub fn check(state: &Map<String, Value>) -> Result<Value, String> {
    let goal = state.get("goal").ok_or("goal is not set")?;
    let plan = state.get("plan").ok_or("plan is not set")?;
    let today = date_at(goal, "today").ok_or("goal has no today")?;
    let deadline = date_at(goal, "deadline").filter(|_| choice(state) != Some(Choice::Move));
    let mut slots = Vec::new();
    for item in plan.get("items").and_then(Value::as_array).into_iter().flatten() {
        slots.extend(evals::slots_within(item, HORIZON_DAYS)?);
    }
    slots.sort_by_key(|s| s.start);
    let cap = daily_cap(state);
    let mut per_day: BTreeMap<NaiveDate, i64> = BTreeMap::new();
    for s in &slots {
        *per_day.entry(s.start.date()).or_default() += (s.end - s.start).num_minutes();
    }
    let problems: Vec<String> = match cap {
        Some(cap) => per_day
            .iter()
            .filter(|(_, m)| **m > cap)
            .take(MAX_PROBLEMS)
            .map(|(d, m)| format!("{d} has {m} minutes, over the owner's {cap} a day; never add sessions to catch up"))
            .collect(),
        None => Vec::new(),
    };
    let prev_tries = state.get("limits").and_then(|l| l.get("tries")).and_then(Value::as_i64).unwrap_or(0);
    let needed = plan.get("needed_minutes").and_then(Value::as_i64).filter(|n| *n > 0);
    let finish = needed.and_then(|n| finish_date(&slots, n));
    let late = match (needed, deadline) {
        (Some(_), Some(d)) => finish.map_or(d <= today + chrono::Days::new(u64::from(HORIZON_DAYS)), |f| f > d),
        _ => false,
    };
    if problems.is_empty() && !late {
        return Ok(json!({"fits": true, "tries": 0, "problems": [], "questions": []}));
    }
    let tries = if problems.is_empty() { prev_tries } else { prev_tries + 1 };
    let questions = card(cap, finish, deadline.filter(|_| late), today);
    Ok(json!({"fits": false, "tries": tries, "problems": problems, "questions": questions}))
}

/// Router condition `limits_gate()` of the new-goal workflow: `fits` when
/// the draft keeps every limit, `over` when the planner should fix days over
/// the cap (at most 3 tries), else `short` for the owner's limits card.
pub fn limits_gate(state: &Map<String, Value>, _args: &[Value]) -> Result<String, String> {
    let limits = state.get("limits").ok_or("limits is not set")?;
    if limits.get("fits").and_then(Value::as_bool).ok_or("limits has no fits")? {
        return Ok("fits".into());
    }
    let over = limits.get("problems").and_then(Value::as_array).is_some_and(|p| !p.is_empty());
    let tries = limits.get("tries").and_then(Value::as_i64).unwrap_or(0);
    Ok(if over && tries < MAX_TRIES { "over" } else { "short" }.into())
}

/// The `check_limits` tool input, read from the new-goal run state.
/// Fields stay JSON: `check` reads each the way `daily_cap` documents.
/// Every field is set before the plan node runs.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CheckArgs {
    pub goal: Value,
    pub plan: Value,
    pub answers: Value,
    pub profile: Value,
    pub limits: Value,
    pub limit_answers: Value,
}

/// Builds the `check_limits` tool of the new-goal workflow: runs `check`
/// on its arguments and returns the new `limits` value as JSON text.
/// A plan that does not expand is an `InvalidArgs` error.
pub fn tool() -> Arc<dyn Tool> {
    Arc::new(typed_tool(
        "check_limits",
        "Check a draft plan against the owner's daily minutes and the goal's date.",
        |a: CheckArgs, _ctx: ToolContext| async move {
            let state: Map<String, Value> = [
                ("goal", a.goal),
                ("plan", a.plan),
                ("answers", a.answers),
                ("profile", a.profile),
                ("limits", a.limits),
                ("limit_answers", a.limit_answers),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
            check(&state).map(|v| ToolOutput::text(v.to_string())).map_err(ToolError::InvalidArgs)
        },
    ))
}

/// The `limits` value a new-goal run starts with: nothing checked yet.
/// `limit_answers` starts as an empty list beside it.
/// Returns the JSON the engine puts in the run input.
pub fn initial() -> Value {
    json!({"fits": true, "tries": 0, "problems": [], "questions": []})
}

const NEW_MINUTES_ID: &str = "new_minutes";
const CHOICE_ID: &str = "limit_choice";
const MAX_DAILY_MINUTES: i64 = 16 * 60;
const HORIZON_DAYS: u32 = 366;
const MAX_PROBLEMS: usize = 5;
const MAX_TRIES: i64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Move,
    Raise,
    Lower,
}

fn choice(state: &Map<String, Value>) -> Option<Choice> {
    let text = state
        .get("limit_answers")
        .and_then(Value::as_array)?
        .iter()
        .find(|a| a.get("id").and_then(Value::as_str) == Some(CHOICE_ID))?
        .get("answer")?
        .as_str()?
        .to_ascii_lowercase();
    [("move", Choice::Move), ("raise", Choice::Raise), ("lower", Choice::Lower)]
        .into_iter()
        .find(|(word, _)| text.starts_with(word))
        .map(|(_, c)| c)
}

fn answer(state: &Map<String, Value>, key: &str, id: &str) -> Option<i64> {
    state
        .get(key)?
        .as_array()?
        .iter()
        .find(|a| a.get("id").and_then(Value::as_str) == Some(id))?
        .get("answer")?
        .as_str()
        .and_then(last_number)
}

fn last_number(text: &str) -> Option<i64> {
    text.split(|c: char| !c.is_ascii_digit()).filter(|w| !w.is_empty()).last()?.parse().ok()
}

fn date_at(goal: &Value, key: &str) -> Option<NaiveDate> {
    let text = goal.get(key)?.as_str()?;
    NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()
}

fn finish_date(slots: &[Slot], needed: i64) -> Option<NaiveDate> {
    let mut done = 0;
    for s in slots {
        done += (s.end - s.start).num_minutes();
        if done >= needed {
            return Some(s.start.date());
        }
    }
    None
}

fn card(cap: Option<i64>, finish: Option<NaiveDate>, target: Option<NaiveDate>, today: NaiveDate) -> Value {
    let pace = cap.map_or("At this plan's pace".to_string(), |c| format!("At {c} min/day"));
    let (text, choices) = match (target, finish) {
        (None, _) => (
            format!("The plan does not fit in {} a day. What should change?", cap.map_or("your minutes".into(), |c| format!("{c} minutes"))),
            vec!["Raise the daily minutes".into(), "Lower the target".into()],
        ),
        (Some(target), Some(f)) => {
            let late = (f - target).num_days();
            let day = |n: i64| if n == 1 { "day" } else { "days" };
            (
                format!("{pace} you finish on {}, {late} {} after your target. What should change?", f.format("%b %-d"), day(late)),
                vec![format!("Move the date to {}", f.format("%b %-d")), "Raise the daily minutes".into(), "Lower the target".into()],
            )
        }
        (Some(_), None) => (
            format!("{pace} you do not finish within a year of {today}. What should change?"),
            vec!["Move the date".into(), "Raise the daily minutes".into(), "Lower the target".into()],
        ),
    };
    let min = cap.map_or(1, |c| (c + 1).min(MAX_DAILY_MINUTES));
    json!([
        {"id": CHOICE_ID, "text": text, "kind": "choice", "choices": choices, "unit": null, "min": null, "max": null, "lasting": null, "known": null},
        {"id": NEW_MINUTES_ID, "text": "If you raise them, how many minutes a day?", "kind": "number", "choices": [],
         "unit": "minutes per day", "min": min, "max": MAX_DAILY_MINUTES, "lasting": null, "known": null},
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn goal(deadline: Option<&str>) -> Value {
        json!({"id": "g", "title": "Read a book", "context": null, "deadline": deadline, "timezone": "UTC", "today": "2026-11-23"})
    }

    fn daily(minutes: i64) -> Value {
        json!({"title": "Read", "start": "2026-11-24T20:00:00", "minutes": minutes, "rrule": "FREQ=DAILY", "notes": "Chapter by chapter"})
    }

    fn plan(items: Vec<Value>, needed: Option<i64>) -> Value {
        json!({"summary": "s", "timezone": "UTC", "items": items, "trackers": [], "daily_minutes": 20, "needed_minutes": needed})
    }

    fn run(plan: Value, deadline: Option<&str>, answer: Option<&str>) -> Map<String, Value> {
        state(json!({
            "goal": goal(deadline),
            "plan": plan,
            "answers": [{"id": DAILY_MINUTES_ID, "answer": answer}],
            "profile": [],
            "limits": {"fits": true, "tries": 0, "problems": [], "questions": []},
            "limit_answers": [],
        }))
    }

    #[test]
    fn cap_comes_from_the_strongest_source() {
        let mut s = run(plan(vec![], None), None, Some("20"));
        assert_eq!(daily_cap(&s), Some(20));
        s.insert("answers".into(), json!([{"id": DAILY_MINUTES_ID, "answer": null}]));
        s.insert("profile".into(), json!([{"id": "f", "fact": "Free minutes a day: 30", "topic": DAILY_MINUTES_TOPIC, "source": "g", "date": "d"}]));
        assert_eq!(daily_cap(&s), Some(30));
        s.insert("profile".into(), json!([]));
        assert_eq!(daily_cap(&s), Some(20));
        s.insert("limit_answers".into(), json!([{"id": "limit_choice", "answer": "Raise the daily minutes"}, {"id": "new_minutes", "answer": "45"}]));
        assert_eq!(daily_cap(&s), Some(45));
    }

    #[test]
    fn catch_up_sessions_on_one_day_are_over_the_cap() {
        let three = (0..3)
            .map(|i| json!({"title": format!("Read {i}"), "start": format!("2026-11-24T{:02}:00:00", 8 + i * 2), "minutes": 20, "rrule": null, "notes": "n"}))
            .collect();
        let s = run(plan(three, None), None, Some("20"));
        let limits = check(&s).unwrap();
        assert_eq!(limits["fits"], json!(false));
        let problems = limits["problems"].as_array().unwrap();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].as_str().unwrap().contains("2026-11-24 has 60 minutes"), "{problems:?}");
        assert_eq!(limits["tries"], json!(1));
        let mut over = s.clone();
        over.insert("limits".into(), limits);
        assert_eq!(limits_gate(&over, &[]).unwrap(), "over");
    }

    #[test]
    fn a_target_past_the_date_gets_the_limits_card() {
        let s = run(plan(vec![daily(20)], Some(22 * 20)), Some("2026-12-05"), Some("20"));
        let limits = check(&s).unwrap();
        assert_eq!(limits["fits"], json!(false));
        assert_eq!(limits["problems"], json!([]));
        let q = &limits["questions"][0];
        assert_eq!(q["id"], "limit_choice");
        assert_eq!(q["text"], "At 20 min/day you finish on Dec 15, 10 days after your target. What should change?");
        assert_eq!(q["choices"], json!(["Move the date to Dec 15", "Raise the daily minutes", "Lower the target"]));
        assert_eq!(limits["questions"][1]["id"], "new_minutes");
        assert_eq!(limits["questions"][1]["kind"], "number");
        let mut short = s.clone();
        short.insert("limits".into(), limits);
        assert_eq!(limits_gate(&short, &[]).unwrap(), "short");
    }

    #[test]
    fn a_plan_that_fits_goes_to_approval() {
        let s = run(plan(vec![daily(20)], Some(5 * 20)), Some("2026-12-05"), Some("20"));
        let limits = check(&s).unwrap();
        assert_eq!(limits, json!({"fits": true, "tries": 0, "problems": [], "questions": []}));
        let mut ok = s.clone();
        ok.insert("limits".into(), limits);
        assert_eq!(limits_gate(&ok, &[]).unwrap(), "fits");
        let moved = state(json!({"limit_answers": [{"id": "limit_choice", "answer": "Move the date to Dec 15"}]}));
        let mut late = run(plan(vec![daily(20)], Some(22 * 20)), Some("2026-12-05"), Some("20"));
        late.extend(moved);
        assert_eq!(check(&late).unwrap()["fits"], json!(true));
    }

    #[test]
    fn open_habits_and_unknown_caps_only_need_no_late_finish() {
        let mut open = plan(vec![daily(45)], None);
        open["daily_minutes"] = Value::Null;
        let s = run(open, None, None);
        assert_eq!(daily_cap(&s), None);
        assert_eq!(check(&s).unwrap()["fits"], json!(true));
    }

    #[test]
    fn a_deadline_past_the_horizon_is_never_called_late() {
        let s = run(plan(vec![daily(20)], Some(400 * 20)), Some("2028-06-01"), Some("20"));
        assert_eq!(check(&s).unwrap()["fits"], json!(true));
        let s = run(plan(vec![daily(20)], Some(400 * 20)), Some("2027-06-01"), Some("20"));
        assert_eq!(check(&s).unwrap()["questions"][0]["text"], "At 20 min/day you do not finish within a year of 2026-11-23. What should change?");
    }

    #[test]
    fn three_failed_tries_go_to_the_owner() {
        let mut s = run(plan(vec![], None), None, Some("20"));
        s.insert("limits".into(), json!({"fits": false, "tries": 3, "problems": ["x"], "questions": [{"id": "limit_choice"}]}));
        assert_eq!(limits_gate(&s, &[]).unwrap(), "short");
    }
}
