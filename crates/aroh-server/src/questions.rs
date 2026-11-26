use crate::rows::{get_row, put_row};
use crate::tools::ToolEnv;
use aroh_domain::rows::{Goal, GoalStatus};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use troupekit_tools::ToolError;

/// The most days a week holds, the cap on every per-week day count (Q92).
pub const DAYS_PER_WEEK: f64 = 7.0;

/// Whether intake question `q` counts days in a week: a number question whose
/// unit is days and whose unit, id or text names a week. A span of days, such
/// as days until an exam, and minutes per day never count.
fn per_week_days(q: &Value) -> bool {
    let field = |k: &str| q.get(k).and_then(Value::as_str).unwrap_or_default().trim().to_lowercase();
    let unit = field("unit");
    q.get("kind").and_then(Value::as_str) == Some("number") && unit.starts_with("day") && [unit.as_str(), &field("id"), &field("text")].iter().any(|s| s.contains("week"))
}

/// Cuts every per-week day count in `intake.questions` to a max of at most 7
/// and a min of at least 0 below that max, whatever the model sent (B106).
/// Other questions stay as they are. Returns how many questions changed.
pub fn bound_questions(intake: &mut Value) -> usize {
    let mut changed = 0;
    for q in intake.get_mut("questions").and_then(Value::as_array_mut).into_iter().flatten() {
        if !per_week_days(q) {
            continue;
        }
        let max = q.get("max").and_then(Value::as_f64).filter(|m| *m >= 1.0).map_or(DAYS_PER_WEEK, |m| m.min(DAYS_PER_WEEK));
        let min = q.get("min").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, max - 1.0);
        assert!(0.0 <= min && min < max && max <= DAYS_PER_WEEK, "a per-week day count fits one week");
        let (lo, hi) = (whole(min), whole(max));
        if q.get("min") != Some(&lo) || q.get("max") != Some(&hi) {
            q["min"] = lo;
            q["max"] = hi;
            changed += 1;
        }
    }
    changed
}

/// Gives an intake saved before Q92 the keys the current `intake` state requires, null when
/// unset: `multi` on each question and `ordered`, `ordered_why` on the intake, so Run again
/// accepts it. Keys already present stay as they are.
pub fn current_shape(intake: &mut Value) {
    for q in intake.get_mut("questions").and_then(Value::as_array_mut).into_iter().flatten() {
        if let Some(q) = q.as_object_mut() {
            q.entry("multi").or_insert(Value::Null);
        }
    }
    if let Some(i) = intake.as_object_mut() {
        i.entry("ordered").or_insert(Value::Null);
        i.entry("ordered_why").or_insert(Value::Null);
    }
}

/// `n` as JSON, a whole number written without a fraction as a model writes it.
fn whole(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        json!(n as i64)
    } else {
        json!(n)
    }
}

/// The `ready_questions` tool input: the intake the interviewer wrote.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReadyArgs {
    pub intake: Value,
}

/// Readies the intake before the owner sees it: bounds per-week day counts
/// and, on a goal still in draft, sets `ordered` from the interviewer's
/// reading of the research (B109), so the switch shows before the questions.
/// The owner's later switch wins, since a live goal is never touched here.
/// Returns the bounded intake.
pub async fn ready_questions(env: &ToolEnv, args: ReadyArgs) -> Result<Value, ToolError> {
    let mut intake = args.intake;
    if !intake.get("questions").is_some_and(Value::is_array) {
        return Err(ToolError::InvalidArgs("intake has no questions list".into()));
    }
    bound_questions(&mut intake);
    if let (Some(ordered), Some(goal)) = (intake.get("ordered").and_then(Value::as_bool), env.goal) {
        let row = get_row::<Goal>(&env.pool, env.user, &goal.to_string()).await.map_err(|e| ToolError::Failed(e.to_string()))?;
        if let Some(mut g) = row.filter(|g| g.status == GoalStatus::Draft && !g.deleted && g.ordered != ordered) {
            g.ordered = ordered;
            put_row(&env.pool, &env.hlc, env.user, &mut g).await.map_err(|e| ToolError::Failed(e.to_string()))?;
        }
    }
    Ok(intake)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(id: &str, text: &str, unit: &str, min: i64, max: i64) -> Value {
        json!({"id": id, "text": text, "kind": "number", "choices": [], "unit": unit, "min": min, "max": max})
    }

    #[test]
    fn b106_per_week_day_counts_are_cut_to_seven() {
        let mut intake = json!({"questions": [
            number("reading_days", "How many days a week can you read?", "days per week", 1, 14),
            number("days", "On how many days can you train?", "days a week", 0, 30),
            number("practice_days", "Practice days?", "days", 9, 10),
            {"id": "gym_days", "text": "How many days each week?", "kind": "number", "choices": [], "unit": "days", "min": null, "max": null},
        ]});
        assert_eq!(bound_questions(&mut intake), 3);
        let bounds: Vec<(Value, Value)> = intake["questions"].as_array().unwrap().iter().map(|q| (q["min"].clone(), q["max"].clone())).collect();
        assert_eq!(bounds[0], (json!(1), json!(7)));
        assert_eq!(bounds[1], (json!(0), json!(7)));
        assert_eq!(bounds[2], (json!(9), json!(10)), "no week named: a plain day count keeps its range");
        assert_eq!(bounds[3], (json!(0), json!(7)), "a missing range gets the week");
    }

    #[test]
    fn b106_a_week_max_keeps_min_below_it() {
        let mut intake = json!({"questions": [number("days_per_week", "Days?", "days", 7, 9), number("week_days", "Days?", "days per week", 3, 5)]});
        assert_eq!(bound_questions(&mut intake), 1, "a range inside the week stays");
        assert_eq!((intake["questions"][0]["min"].clone(), intake["questions"][0]["max"].clone()), (json!(6), json!(7)));
        assert_eq!((intake["questions"][1]["min"].clone(), intake["questions"][1]["max"].clone()), (json!(3), json!(5)));
    }

    #[test]
    fn b106_spans_and_minutes_stay() {
        let original = json!({"questions": [
            number("days_left", "How many days until your exam?", "days", 1, 60),
            number("daily_minutes", "Minutes a day on weekdays?", "minutes per day", 5, 480),
            {"id": "week_plan", "text": "What does your week look like?", "kind": "text", "choices": []},
        ]});
        let mut intake = original.clone();
        assert_eq!(bound_questions(&mut intake), 0);
        assert_eq!(intake, original);
    }

    #[test]
    fn b105_b109_the_new_goal_run_readies_questions_before_the_owner_sees_them() {
        let wf = crate::workflows::workflow(crate::workflows::NEW_GOAL).unwrap();
        let next = |from: &str| wf.edges.iter().filter(|e| e.from == from).map(|e| e.to.clone()).collect::<Vec<_>>();
        assert_eq!(next("intake"), ["ready_questions"]);
        assert_eq!(next("ready_questions"), ["questions"]);
        let node = |id: &str| wf.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(node("ready_questions").writes.as_deref(), Some("intake"));
        assert!(node("write").reads.iter().any(|r| r == "intake"), "write_plan sees whether the intake set the order");
        assert!(node("intake").reads.iter().any(|r| r == "goal"), "the description reaches the intake as goal.context");
        assert!(wf.state["goal"].to_string().contains("context"), "the goal state carries the description");
    }

    #[test]
    fn b107_an_intake_saved_before_multi_and_ordered_still_starts_run_again() {
        let wf = crate::workflows::workflow(crate::workflows::RUN_AGAIN).unwrap();
        let mut old = json!({"questions": [
            {"id": "when", "text": "When?", "kind": "choice", "choices": ["Morning"], "unit": null, "min": null, "max": null, "lasting": null, "known": null},
            {"id": "pick", "text": "Pick?", "kind": "choice", "choices": ["A"], "multi": true, "unit": null, "min": null, "max": null, "lasting": null, "known": null},
        ]});
        assert!(!wf.check_value("intake", &old).is_empty(), "the old shape lacks the new keys");
        current_shape(&mut old);
        assert_eq!(wf.check_value("intake", &old), Vec::<String>::new());
        assert_eq!(old["questions"][1]["multi"], json!(true), "a set key stays");
        assert_eq!((old["ordered"].clone(), old["ordered_why"].clone()), (Value::Null, Value::Null));
    }

    #[test]
    fn b105_b107_b109_the_interviewer_is_told_each_rule() {
        let text = crate::workflows::shipped("intake-interviewer").unwrap();
        for rule in ["goal.context is the description", "source goal description", "max 7, never more", "Never list Other", "Set multi true", "Set ordered from the research", "multi: bool?", "ordered = \"bool?\""] {
            assert!(text.contains(rule), "{rule}");
        }
    }
}
