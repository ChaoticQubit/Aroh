use aroh_domain::recur;
use aroh_domain::rows::ActionSeries;
use chrono::{Duration, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde_json::Value;
use std::sync::Arc;
use troupekit_agent::eval::lookup;
use troupekit_agent::EvalRegistry;

/// Days of a recurring item that the evals expand and check.
/// Matches the materialized window of the app engine.
/// The planner's later occurrences repeat the same pattern.
pub const EVAL_WINDOW_DAYS: u32 = 14;

const MAX_MINUTES: i64 = 24 * 60;
const PLACEHOLDER_ID: &str = "00000000-0000-7000-8000-000000000000";

/// Parses a local wall time `YYYY-MM-DDTHH:MM:SS` or `YYYY-MM-DDTHH:MM`, the
/// form plans and proposals use. Returns the naive time, or why it does
/// not parse.
pub fn parse_local(text: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M"))
        .map_err(|_| format!("`{text}` is not a local time YYYY-MM-DDTHH:MM:SS"))
}

/// One planned slot in local wall time, with the title it came from.
/// Evals compare slots; tools turn them into rows.
/// `end` is after `start`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub title: String,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}

/// Expands one item (`title`, `start`, `minutes`, optional `rrule`) into its
/// slots: one, or every occurrence in the first `EVAL_WINDOW_DAYS` days of
/// the rule. An item without `start` (a deletion) has no slots.
pub fn slots(item: &Value) -> Result<Vec<Slot>, String> {
    let title = item.get("title").and_then(Value::as_str).unwrap_or("item").to_string();
    let Some(start) = item.get("start").and_then(Value::as_str) else {
        return Ok(Vec::new());
    };
    let start = parse_local(start)?;
    let minutes = item.get("minutes").and_then(Value::as_i64).ok_or_else(|| format!("`{title}` has no minutes"))?;
    if !(1..=MAX_MINUTES).contains(&minutes) {
        return Err(format!("`{title}` lasts {minutes} minutes, outside 1 to {MAX_MINUTES}"));
    }
    let length = Duration::minutes(minutes);
    let Some(rule) = item.get("rrule").and_then(Value::as_str).filter(|r| !r.trim().is_empty()) else {
        return Ok(vec![Slot { title, start, end: start + length }]);
    };
    let series = ActionSeries {
        id: PLACEHOLDER_ID.into(),
        user_id: PLACEHOLDER_ID.into(),
        goal_id: PLACEHOLDER_ID.into(),
        title: title.clone(),
        notes: None,
        rrule: rule.to_string(),
        dtstart_local: start.format("%Y-%m-%dT%H:%M:%S").to_string(),
        timezone: "UTC".into(),
        duration_minutes: minutes,
        updated_at: aroh_domain::seed::SEED_STAMP.into(),
        deleted: false,
    };
    let from = Utc.from_utc_datetime(&start) - Duration::seconds(1);
    let occurrences = recur::materialize(&series, from, EVAL_WINDOW_DAYS, &Default::default())
        .map_err(|e| format!("`{title}`: {e}"))?;
    Ok(occurrences
        .into_iter()
        .map(|a| Slot { title: title.clone(), start: a.start_at.naive_utc(), end: a.end_at.naive_utc() })
        .collect())
}

fn path_slots(output: &Value, args: &[Value]) -> Result<Vec<Slot>, String> {
    let path = args.first().and_then(Value::as_str).ok_or("takes a path to a list of items first")?;
    let mut out = Vec::new();
    for found in lookup(output, path) {
        match found {
            Value::Array(items) => {
                for item in items {
                    out.extend(slots(item)?);
                }
            }
            item => out.extend(slots(item)?),
        }
    }
    Ok(out)
}

/// Eval `no_overlap(path)`: the slots of the items at `path` never overlap
/// one another. Returns the first clash, naming both titles and the time.
/// Touching slots (one ends as the next starts) are fine.
pub fn no_overlap(output: &Value, args: &[Value]) -> Result<(), String> {
    if args.len() != 1 {
        return Err("takes (path)".into());
    }
    let mut all = path_slots(output, args)?;
    all.sort_by(|a, b| (a.start, a.end).cmp(&(b.start, b.end)));
    for pair in all.windows(2) {
        if pair[1].start < pair[0].end {
            return Err(format!("`{}` overlaps `{}` at {}", pair[1].title, pair[0].title, pair[1].start));
        }
    }
    Ok(())
}

/// Eval `within_availability(path, "HH:MM", "HH:MM")`: every slot of the
/// items at `path` starts and ends inside the daily window on its own day.
/// Returns the first slot outside it.
pub fn within_availability(output: &Value, args: &[Value]) -> Result<(), String> {
    let time = |i: usize| {
        args.get(i)
            .and_then(Value::as_str)
            .and_then(|s| NaiveTime::parse_from_str(s, "%H:%M").ok())
            .ok_or_else(|| "takes (path, \"HH:MM\", \"HH:MM\")".to_string())
    };
    let (from, to) = (time(1)?, time(2)?);
    if args.len() != 3 || from >= to {
        return Err("takes (path, from, to) with from before to".into());
    }
    for slot in path_slots(output, args)? {
        let inside = slot.start.time() >= from && slot.end.date() == slot.start.date() && slot.end.time() <= to;
        if !inside {
            return Err(format!("`{}` at {} is outside {from} to {to}", slot.title, slot.start));
        }
    }
    Ok(())
}

/// Builds the eval registry every Aroh run uses: the built-in `min_items`
/// plus the domain evals `no_overlap` and `within_availability`.
/// Returns the registry.
pub fn registry() -> EvalRegistry {
    let mut r = EvalRegistry::new();
    r.register("no_overlap", Arc::new(no_overlap)).expect("fresh name");
    r.register("within_availability", Arc::new(within_availability)).expect("fresh name");
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plan(items: Value) -> Value {
        json!({ "items": items })
    }

    #[test]
    fn no_overlap_accepts_touching_and_rejects_clashes() {
        let ok = plan(json!([
            {"title": "Run", "start": "2026-11-23T07:00:00", "minutes": 30},
            {"title": "Core", "start": "2026-11-23T07:30", "minutes": 20}
        ]));
        assert!(no_overlap(&ok, &[json!("items")]).is_ok());
        let bad = plan(json!([
            {"title": "Run", "start": "2026-11-23T07:00:00", "minutes": 45},
            {"title": "Core", "start": "2026-11-23T07:30:00", "minutes": 20}
        ]));
        let e = no_overlap(&bad, &[json!("items")]).unwrap_err();
        assert!(e.contains("Core") && e.contains("Run"), "{e}");
        assert!(no_overlap(&ok, &[]).is_err());
    }

    #[test]
    fn expands_rrule_items() {
        let daily = json!({"title": "Plank", "start": "2026-11-23T07:00:00", "minutes": 10, "rrule": "FREQ=DAILY"});
        assert_eq!(slots(&daily).unwrap().len(), EVAL_WINDOW_DAYS as usize);
        let weekly = plan(json!([
            {"title": "Gym", "start": "2026-11-23T18:00:00", "minutes": 60, "rrule": "FREQ=WEEKLY;BYDAY=MO,WE"},
            {"title": "Call", "start": "2026-11-25T18:30:00", "minutes": 15}
        ]));
        let e = no_overlap(&weekly, &[json!("items")]).unwrap_err();
        assert!(e.contains("Call"), "{e}");
        let bad = json!({"title": "x", "start": "2026-11-23T07:00:00", "minutes": 10, "rrule": "FREQ=HOURLY"});
        assert!(slots(&bad).is_err());
    }

    #[test]
    fn within_availability_checks_the_daily_window() {
        let args = [json!("items"), json!("06:00"), json!("22:00")];
        let ok = plan(json!([{"title": "Run", "start": "2026-11-23T06:00:00", "minutes": 960}]));
        assert!(within_availability(&ok, &args).is_ok());
        let late = plan(json!([{"title": "Run", "start": "2026-11-23T21:30:00", "minutes": 45}]));
        assert!(within_availability(&late, &args).unwrap_err().contains("Run"));
        let early = plan(json!([{"title": "Run", "start": "2026-11-23T05:59:00", "minutes": 5}]));
        assert!(within_availability(&early, &args).is_err());
        assert!(within_availability(&ok, &[json!("items"), json!("22:00"), json!("06:00")]).is_err());
    }

    #[test]
    fn bad_items_fail_and_deletions_have_no_slots() {
        assert!(slots(&json!({"title": "x", "start": "tomorrow", "minutes": 5})).is_err());
        assert!(slots(&json!({"title": "x", "start": "2026-11-23T07:00:00", "minutes": 0})).is_err());
        assert!(slots(&json!({"title": "x", "start": null, "minutes": null})).unwrap().is_empty());
        let r = registry();
        assert!(r.get("no_overlap").is_some() && r.get("within_availability").is_some() && r.get("min_items").is_some());
    }
}
