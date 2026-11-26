//! The day check of planning runs (Q95, B113): a plan whose weeks do not use the days the
//! owner answered goes back to the planner with each mismatch named, at most
//! [`MAX_RETRIES`] times, then the run fails with a plain reason the app shows as is.

use crate::evals::{self, Slot};
use crate::questions::per_week_days;
use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Weekday};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use troupekit_tools::{typed_tool, Tool, ToolContext, ToolError, ToolOutput};

/// Times a plan that misses the owner's days goes back to the planner before the run fails.
pub const MAX_RETRIES: i64 = 2;

/// Start of the failure reason the app shows as is, without the node and tool around it.
pub const PLAIN_REASON: &str = "Could not plan the days you answered";

/// Question id the intake interviewer gives the weekdays the owner named.
pub const PREFERRED_DAYS: &str = "preferred_days";

/// Mismatches named to the planner in one round; the rest wait for the next check.
const MAX_NAMED: usize = 10;

/// The weekdays Monday first, indexed by `num_days_from_monday`.
const WEEK: [Weekday; 7] = [Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri, Weekday::Sat, Weekday::Sun];

/// The `day_fit` value a planning run starts with: nothing checked yet, nothing waived.
pub fn initial() -> Value {
    json!({"misses": [], "tries": 0, "waived": false})
}

/// The `day_fit` value once the owner's change request restated the days: clear for the rest
/// of the run, since the later word wins over the earlier answer.
fn waived() -> Value {
    json!({"misses": [], "tries": 0, "waived": true})
}

/// The owner's day answers: how many days a week (1 to 7) and the weekdays they named, as
/// indexes from Monday. Both empty when the owner skipped or was not asked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DayAnswers {
    pub per_week: Option<usize>,
    pub named: BTreeSet<usize>,
}

/// The lowercase words of `text`, split on anything not a letter.
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()).map(str::to_lowercase)
}

/// The weekday indexes a word names: one day, both weekend days, the five weekdays, or none.
fn days_of(word: &str) -> &'static [usize] {
    match word {
        "mon" | "monday" | "mondays" => &[0],
        "tue" | "tues" | "tuesday" | "tuesdays" => &[1],
        "wed" | "wednesday" | "wednesdays" => &[2],
        "thu" | "thur" | "thurs" | "thursday" | "thursdays" => &[3],
        "fri" | "friday" | "fridays" => &[4],
        "sat" | "saturday" | "saturdays" => &[5],
        "sun" | "sunday" | "sundays" => &[6],
        "weekend" | "weekends" => &[5, 6],
        "weekdays" => &[0, 1, 2, 3, 4],
        _ => &[],
    }
}

/// The answer the owner gave question `id`, None when skipped or not asked.
fn answer_of<'a>(answers: &'a Value, id: &str) -> Option<&'a str> {
    answers.as_array()?.iter().find(|a| a.get("id").and_then(Value::as_str) == Some(id))?.get("answer")?.as_str()
}

/// Whether per-week question `q` counts days off rather than days of work on the goal.
fn counts_days_off(q: &Value) -> bool {
    ["id", "text"].iter().any(|k| words(q.get(*k).and_then(Value::as_str).unwrap_or_default()).any(|w| w == "rest" || w == "off"))
}

/// The first whole number written in `text`.
fn first_number(text: &str) -> Option<usize> {
    text.split(|c: char| !c.is_ascii_digit()).find(|t| !t.is_empty())?.parse().ok()
}

/// Reads the owner's day answers from `intake` and `answers`: the days a week from the one
/// per-week day question `ready_questions` recognizes (none when several are answered, since
/// their sum is unknown, or when it counts rest days), and the weekdays answered to [`PREFERRED_DAYS`]. Returns them; empty when nothing was answered.
pub fn day_answers(intake: &Value, answers: &Value) -> DayAnswers {
    let questions = intake.get("questions").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    let counts: Vec<usize> = questions
        .iter()
        .filter(|q| per_week_days(q) && !counts_days_off(q))
        .filter_map(|q| answer_of(answers, q.get("id")?.as_str()?))
        .filter_map(first_number)
        .filter(|n| (1..=WEEK.len()).contains(n))
        .collect();
    let per_week = if counts.len() == 1 { counts.first().copied() } else { None };
    let named = answer_of(answers, PREFERRED_DAYS).map(|a| words(a).flat_map(|w| days_of(&w).iter().copied()).collect()).unwrap_or_default();
    DayAnswers { per_week, named }
}

/// Whether the owner's change request `change` restates their days, by naming a weekday or
/// saying day or week. The later word wins, so the check then stands aside.
pub fn restates_days(change: &str) -> bool {
    words(change).any(|w| !days_of(&w).is_empty() || w.starts_with("day") || w.starts_with("week"))
}

/// The local dates a session need not land on: `away_days` and every date a busy entry in
/// `busy` covers from 06:00 to 22:00.
pub fn excused_dates(away_days: &Value, busy: &[Slot]) -> BTreeSet<NaiveDate> {
    let mut out: BTreeSet<NaiveDate> = away_days.as_array().into_iter().flatten().filter_map(Value::as_str).filter_map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).collect();
    let (open, close) = (NaiveTime::from_hms_opt(6, 0, 0).expect("valid time"), NaiveTime::from_hms_opt(22, 0, 0).expect("valid time"));
    for b in busy {
        let mut day = b.start.date();
        while day <= b.end.date() {
            if b.start <= day.and_time(open) && b.end >= day.and_time(close) {
                out.insert(day);
            }
            day += Duration::days(1);
        }
    }
    out
}

/// Short day names joined with commas, such as `Mon, Tue, Thu`.
fn short_names(days: &BTreeSet<usize>) -> String {
    days.iter().map(|i| WEEK[*i].to_string()).collect::<Vec<_>>().join(", ")
}

/// Full day names joined as a sentence, such as `Saturday and Sunday`.
fn full_names(days: &BTreeSet<usize>) -> String {
    let names: Vec<String> = days.iter().map(|i| NaiveDate::from_isoywd_opt(2026, 1, WEEK[*i]).expect("a real week").format("%A").to_string()).collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

/// `n` days, singular for one.
fn day_count(n: usize) -> String {
    if n == 1 { "1 day".into() } else { format!("{n} days") }
}

/// Every way the plan misses `want`, judged on each Monday-to-Sunday week that lies fully
/// between the first and the last of `slots` (a partial first or last week is excepted). A
/// week must reach the days a week answered with any session, or every free day when fewer
/// dates are free (dates in `excused` are not), and must not pass it with the `regular`
/// (repeating) sessions; with named days and no larger count, regular sessions keep to those
/// weekdays. A one-off, such as an errand or a final quiz, never counts as an extra day.
/// Returns one plain line per mismatch, at most [`MAX_NAMED`].
pub fn misses(slots: &[Slot], regular: &[Slot], want: &DayAnswers, excused: &BTreeSet<NaiveDate>) -> Vec<String> {
    let (Some(first), Some(last)) = (slots.iter().map(|s| s.start.date()).min(), slots.iter().map(|s| s.start.date()).max()) else { return Vec::new() };
    let only_named = !want.named.is_empty() && want.per_week.is_none_or(|n| n <= want.named.len());
    let mut empty = BTreeSet::new();
    let mut off_named = BTreeSet::new();
    let mut wrong: BTreeMap<BTreeSet<usize>, Vec<NaiveDate>> = BTreeMap::new();
    let mut monday = first + Duration::days(i64::from((7 - first.weekday().num_days_from_monday()) % 7));
    while monday + Duration::days(6) <= last {
        let date = |i: usize| monday + Duration::days(i as i64);
        let week_days = |of: &[Slot]| -> BTreeSet<usize> { of.iter().map(|s| s.start.date()).filter(|d| (monday..=date(6)).contains(d)).map(|d| d.weekday().num_days_from_monday() as usize).collect() };
        let (used, repeating) = (week_days(slots), week_days(regular));
        assert!(repeating.is_subset(&used) && used.len() <= WEEK.len(), "regular sessions are sessions of a seven-day week");
        if only_named {
            off_named.extend(repeating.difference(&want.named).copied());
        }
        if let Some(n) = want.per_week {
            let free: Vec<usize> = (0..WEEK.len()).filter(|i| !excused.contains(&date(*i))).collect();
            let short = used.len() < n.min(free.len());
            if short && n == WEEK.len() {
                empty.extend(free.iter().filter(|i| !used.contains(i)));
            } else if short {
                wrong.entry(used).or_default().push(monday);
            } else if repeating.len() > n {
                wrong.entry(repeating).or_default().push(monday);
            }
        }
        monday += Duration::days(7);
    }
    let mut out = Vec::new();
    if !empty.is_empty() {
        out.push(format!("You answered 7 days a week; the plan leaves {} empty", full_names(&empty)));
    }
    let n = want.per_week.unwrap_or_default();
    let mut weeks: Vec<(&BTreeSet<usize>, &Vec<NaiveDate>)> = wrong.iter().collect();
    weeks.sort_by_key(|(_, mondays)| mondays[0]);
    for (used, mondays) in weeks {
        let more = match mondays.len() {
            1 => String::new(),
            2 => ", and 1 more week like it".into(),
            m => format!(", and {} more weeks like it", m - 1),
        };
        out.push(format!("You answered {} a week; the week of {} uses {} ({}){more}", day_count(n), mondays[0].format("%a %Y-%m-%d"), day_count(used.len()), short_names(used)));
    }
    if !off_named.is_empty() {
        out.push(format!("You named {}; the plan also uses {}", full_names(&want.named), full_names(&off_named)));
    }
    out.truncate(MAX_NAMED);
    out
}

/// The failure reason once the planner ran out of retries: the first mismatch.
pub fn plain_reason(first: &str) -> String {
    format!("{PLAIN_REASON}: {first}")
}

/// The `check_days` tool input, read from the planning run state.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CheckArgs {
    pub plan: Value,
    pub intake: Value,
    pub answers: Value,
    pub change: String,
    pub away_days: Value,
    pub day_fit: Value,
}

/// Checks `a.plan` (its `items` expanded over `days`) against the owner's day answers after
/// the previous check `a.day_fit`, excusing away days and dates `busy` fills. Returns the new
/// `day_fit`: no misses and tries back at 0 (also when nothing was answered), waived for the
/// rest of the run once the owner's change restates the days, or the misses named for the
/// planner and one more try. Errors with [`plain_reason`] once the previous check used
/// [`MAX_RETRIES`], or with why the plan does not expand.
pub fn check(a: &CheckArgs, busy: &[Slot], days: u32) -> Result<Value, ToolError> {
    if a.day_fit.get("waived").and_then(Value::as_bool) == Some(true) || restates_days(&a.change) {
        return Ok(waived());
    }
    let want = day_answers(&a.intake, &a.answers);
    if want.per_week.is_none() && want.named.is_empty() {
        return Ok(initial());
    }
    let (mut slots, mut regular) = (Vec::new(), Vec::new());
    for item in a.plan.get("items").and_then(Value::as_array).into_iter().flatten() {
        let expanded = evals::slots_within(item, days).map_err(ToolError::InvalidArgs)?;
        if item.get("rrule").and_then(Value::as_str).is_some_and(|r| !r.trim().is_empty()) {
            regular.extend(expanded.iter().cloned());
        }
        slots.extend(expanded);
    }
    let found = misses(&slots, &regular, &want, &excused_dates(&a.away_days, busy));
    let Some(first) = found.first() else { return Ok(initial()) };
    let tries = a.day_fit.get("tries").and_then(Value::as_i64).unwrap_or(0);
    assert!(tries >= 0, "tries never go below zero");
    if tries >= MAX_RETRIES {
        return Err(ToolError::Blocked(plain_reason(first)));
    }
    Ok(json!({"misses": found, "tries": tries + 1}))
}

/// Router condition `day_gate()`: `miss` when the last check named misses, else `clear`.
pub fn day_gate(state: &Map<String, Value>, _args: &[Value]) -> Result<String, String> {
    let list = state.get("day_fit").and_then(|o| o.get("misses")).and_then(Value::as_array).ok_or("day_fit has no misses")?;
    Ok(if list.is_empty() { "clear" } else { "miss" }.into())
}

/// Builds the `check_days` tool of one run, closed over the run's `busy` slots and the
/// horizon `days` the overlap check uses, so both see the calendar alike.
/// Returns the tool; its output is the new `day_fit` value as JSON text.
pub fn tool(busy: Vec<Slot>, days: u32) -> Arc<dyn Tool> {
    let busy = Arc::new(busy);
    Arc::new(typed_tool("check_days", "Check that a draft plan uses the days a week and weekdays the owner answered.", move |a: CheckArgs, _ctx: ToolContext| {
        let busy = busy.clone();
        async move { check(&a, &busy, days).map(|v| ToolOutput::text(v.to_string())) }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::{workflow, NEW_GOAL, RUN_AGAIN};
    use troupekit_graph::NodeKind;

    fn intake() -> Value {
        json!({"questions": [
            {"id": "reading_days", "text": "How many days a week can you read?", "kind": "number", "choices": [], "unit": "days per week", "min": 1, "max": 7},
            {"id": "daily_minutes", "text": "Minutes a day?", "kind": "number", "choices": [], "unit": "minutes per day", "min": 5, "max": 480},
            {"id": "preferred_days", "text": "Which days suit you?", "kind": "choice", "choices": ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"], "multi": true}
        ]})
    }

    fn args(byday: &str, answers: Value) -> CheckArgs {
        let plan = json!({"summary": "Book", "timezone": "Europe/Berlin", "items": [
            {"title": "Read", "start": "2026-11-30T07:00:00", "minutes": 45, "rrule": format!("FREQ=WEEKLY;BYDAY={byday}")}
        ]});
        CheckArgs { plan, intake: intake(), answers, change: String::new(), away_days: json!([]), day_fit: initial() }
    }

    fn seven() -> Value {
        json!([{"id": "reading_days", "answer": "7"}, {"id": "daily_minutes", "answer": "60"}, {"id": "preferred_days", "answer": null}])
    }

    #[test]
    fn b113_seven_days_answered_and_sunday_left_empty_goes_back_twice_then_fails_plainly() {
        let mut a = args("MO,TU,WE,TH,FR,SA", seven());
        let first = check(&a, &[], 28).unwrap();
        assert_eq!(first, json!({"misses": ["You answered 7 days a week; the plan leaves Sunday empty"], "tries": 1}));
        a.day_fit = first;
        let second = check(&a, &[], 28).unwrap();
        assert_eq!(second["tries"], 2);
        a.day_fit = second;
        let Err(ToolError::Blocked(reason)) = check(&a, &[], 28) else { panic!("the third miss fails the run") };
        assert_eq!(reason, "Could not plan the days you answered: You answered 7 days a week; the plan leaves Sunday empty");
    }

    #[test]
    fn b113_a_plan_on_every_weekday_is_clear_and_resets_the_tries() {
        let mut a = args("MO,TU,WE,TH,FR,SA,SU", seven());
        a.day_fit = json!({"misses": ["x"], "tries": 2});
        let clear = check(&a, &[], 28).unwrap();
        assert_eq!(clear, initial());
        let mut state = Map::new();
        state.insert("day_fit".into(), clear);
        assert_eq!(day_gate(&state, &[]).unwrap(), "clear");
        state.insert("day_fit".into(), json!({"misses": ["x"], "tries": 1}));
        assert_eq!(day_gate(&state, &[]).unwrap(), "miss");
    }

    #[test]
    fn b113_partial_first_and_last_weeks_are_excepted() {
        let mut a = args("MO,TU,WE,TH,FR,SA,SU", seven());
        a.plan["items"][0]["start"] = json!("2026-12-03T07:00:00");
        a.plan["items"].as_array_mut().unwrap().push(json!({"title": "Finish", "start": "2026-12-15T07:00:00", "minutes": 30, "rrule": null}));
        assert_eq!(check(&a, &[], 12).unwrap(), initial(), "Thu to Sun, then a full week, then Monday and one Tuesday");
        let short = args("MO,TU,WE,TH,FR,SA", seven());
        assert_eq!(check(&short, &[], 6).unwrap(), initial(), "no full week inside Mon to Sat");
    }

    #[test]
    fn b113_fewer_days_answered_names_the_extra_days_week_by_week() {
        let answers = json!([{"id": "reading_days", "answer": "3"}]);
        let found = check(&args("MO,TU,TH,SA", answers.clone()), &[], 21).unwrap();
        assert_eq!(found["misses"], json!(["You answered 3 days a week; the week of Mon 2026-11-30 uses 4 days (Mon, Tue, Thu, Sat), and 1 more week like it"]));
        let found = check(&args("MO,TH", answers.clone()), &[], 21).unwrap();
        assert_eq!(found["misses"], json!(["You answered 3 days a week; the week of Mon 2026-11-30 uses 2 days (Mon, Thu), and 1 more week like it"]));
        assert_eq!(check(&args("MO,WE,FR", answers), &[], 21).unwrap(), initial());
    }

    #[test]
    fn b113_away_days_and_days_busy_all_day_are_excused() {
        let mut a = args("MO,TU,WE,TH,FR,SA", seven());
        a.away_days = json!(["2026-12-06"]);
        assert_eq!(check(&a, &[], 14).unwrap(), initial(), "the one full week's Sunday is an away day");
        a.away_days = json!([]);
        let start = evals::parse_local("2026-12-06T00:00:00").unwrap();
        let busy = [Slot { title: "Wedding".into(), start, end: start + Duration::days(1) }];
        assert_eq!(check(&a, &busy, 14).unwrap(), initial(), "a Sunday busy all day");
        let evening = [Slot { title: "Dinner".into(), start: start + Duration::hours(19), end: start + Duration::hours(21) }];
        assert_eq!(check(&a, &evening, 14).unwrap()["tries"], 1, "a busy evening leaves the day free");
    }

    #[test]
    fn b113_no_answer_a_skip_rest_days_or_two_counts_check_nothing() {
        let plan = |answers: Value| check(&args("MO", answers), &[], 28).unwrap();
        assert_eq!(plan(json!([])), initial());
        assert_eq!(plan(json!([{"id": "reading_days", "answer": null}])), initial());
        let mut a = args("MO", json!([{"id": "rest_days", "answer": "1"}]));
        a.intake["questions"][0]["id"] = json!("rest_days");
        a.intake["questions"][0]["text"] = json!("How many rest days a week?");
        assert_eq!(check(&a, &[], 28).unwrap(), initial());
        let mut a = args("MO", json!([{"id": "reading_days", "answer": "5"}]));
        a.intake["questions"][0]["text"] = json!("How many days this week can you read?");
        a.intake["questions"][0]["unit"] = json!("days");
        assert_eq!(check(&a, &[], 28).unwrap()["tries"], 1, "a week's day count, however worded, is the weekly rule");
        let mut a = args("MO", json!([{"id": "reading_days", "answer": "4"}, {"id": "cardio_days", "answer": "2"}]));
        a.intake["questions"].as_array_mut().unwrap().push(json!({"id": "cardio_days", "text": "Cardio days?", "kind": "number", "choices": [], "unit": "days per week"}));
        assert_eq!(check(&a, &[], 28).unwrap(), initial(), "two counts, sum unknown");
    }

    #[test]
    fn b113_a_change_that_restates_the_days_wins_over_the_earlier_answer() {
        let mut a = args("MO,TU,WE,TH,FR,SA", seven());
        a.change = "Drop Sundays, I go to church".into();
        assert_eq!(check(&a, &[], 28).unwrap(), json!({"misses": [], "tries": 0, "waived": true}));
        a.day_fit = check(&a, &[], 28).unwrap();
        assert_eq!(a.day_fit["waived"], true);
        a.change = "Make the sessions shorter".into();
        assert_eq!(check(&a, &[], 28).unwrap()["waived"], true, "a later change about something else keeps the days the owner restated");
        a.day_fit = initial();
        assert_eq!(check(&a, &[], 28).unwrap()["tries"], 1, "a change about something else keeps the check");
        assert!(restates_days("Only five days a week") && restates_days("weekends free") && !restates_days("Read in the morning today"));
    }

    #[test]
    fn b113_a_one_off_errand_is_no_extra_day_but_still_fills_one() {
        let mut a = args("MO,WE,FR", json!([{"id": "reading_days", "answer": "3"}]));
        a.plan["items"].as_array_mut().unwrap().push(json!({"title": "Buy the book", "start": "2026-12-01T18:00:00", "minutes": 30, "rrule": null}));
        assert_eq!(check(&a, &[], 21).unwrap(), initial(), "three reading days and one errand on Tuesday");
        let mut a = args("MO,TU,WE,TH,FR,SA", seven());
        a.plan["items"].as_array_mut().unwrap().push(json!({"title": "Review", "start": "2026-12-06T18:00:00", "minutes": 30, "rrule": null}));
        assert_eq!(check(&a, &[], 14).unwrap(), initial(), "a one-off fills the one full week's Sunday");
    }

    #[test]
    fn b113_named_days_keep_sessions_on_those_weekdays() {
        let answers = json!([{"id": "preferred_days", "answer": "Mon, Wed, Fri"}]);
        let found = check(&args("MO,TU,WE,FR", answers.clone()), &[], 21).unwrap();
        assert_eq!(found["misses"], json!(["You named Monday, Wednesday and Friday; the plan also uses Tuesday"]));
        assert_eq!(check(&args("MO,WE,FR", answers), &[], 21).unwrap(), initial());
        let more = json!([{"id": "reading_days", "answer": "5"}, {"id": "preferred_days", "answer": "Sat, Sun"}]);
        assert_eq!(check(&args("MO,TU,WE,SA,SU", more), &[], 21).unwrap(), initial(), "a larger count wins over too few named days");
        assert_eq!(day_answers(&intake(), &json!([{"id": "preferred_days", "answer": "Weekends, Other: tuesdays"}])).named, BTreeSet::from([1, 5, 6]));
    }

    #[test]
    fn b113_every_planner_node_reads_the_answers_and_every_plan_passes_the_day_check() {
        for name in [NEW_GOAL, RUN_AGAIN] {
            let wf = workflow(name).unwrap();
            let next = |from: &str, when: Option<&str>| wf.edges.iter().filter(|e| e.from == from && e.when.as_deref() == when).map(|e| e.to.clone()).collect::<Vec<_>>();
            assert_eq!(next("overlap_gate", Some("clear")), ["day_check"], "{name}");
            assert_eq!(next("day_check", None), ["day_gate"], "{name}");
            assert_eq!(next("day_gate", Some("clear")), ["check_limits"], "{name}");
            assert_eq!(next("day_gate", Some("miss")), ["fit_days"], "{name}");
            assert_eq!(next("fit_days", None), ["place_check"], "{name}");
            for from in ["plan", "fit", "revise", "place"] {
                assert_eq!(next(from, None), ["place_check"], "{name}: {from} reaches the day check through the overlap check");
            }
            let planners: Vec<_> = wf.nodes.iter().filter(|n| matches!(&n.kind, NodeKind::Agent { persona, .. } if persona == "goal-planner")).collect();
            assert_eq!(planners.len(), 5, "{name}");
            for n in planners {
                assert!(["intake", "answers", "goal", "profile"].iter().all(|k| n.reads.iter().any(|r| r == k)), "{name}: {} reads the owner's answers", n.id);
            }
            let check = wf.nodes.iter().find(|n| n.id == "day_check").unwrap();
            assert_eq!(check.writes.as_deref(), Some("day_fit"));
            assert!(wf.check_value("day_fit", &initial()).is_empty(), "{name}");
        }
    }
}
