use aroh_domain::recur;
use aroh_domain::rows::ActionSeries;
use chrono::{Duration, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde_json::Value;
use std::collections::BTreeSet;
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

fn clash(mut all: Vec<Slot>, busy: &[Slot]) -> Result<(), String> {
    all.sort_by(|a, b| (a.start, a.end).cmp(&(b.start, b.end)));
    for pair in all.windows(2) {
        if pair[1].start < pair[0].end {
            return Err(format!("`{}` overlaps `{}` at {}", pair[1].title, pair[0].title, pair[1].start));
        }
    }
    for slot in &all {
        if let Some(b) = busy.iter().find(|b| slot.start < b.end && b.start < slot.end) {
            return Err(format!("`{}` overlaps busy `{}` at {}", slot.title, b.title, b.start));
        }
    }
    Ok(())
}

/// Eval `no_overlap(path)`: the slots of the items at `path` never overlap
/// one another. Returns the first clash, naming both titles and the time.
/// Touching slots (one ends as the next starts) are fine.
pub fn no_overlap(output: &Value, args: &[Value]) -> Result<(), String> {
    if args.len() != 1 {
        return Err("takes (path)".into());
    }
    clash(path_slots(output, args)?, &[])
}

/// Reads a run's `busy` state value, a list of `{title, start, minutes}` in
/// local wall time, into slots. Busy slots may overlap one another.
/// Returns why an entry does not parse.
pub fn busy_slots(busy: &Value) -> Result<Vec<Slot>, String> {
    let mut out = Vec::new();
    for item in busy.as_array().map(Vec::as_slice).unwrap_or_default() {
        out.extend(slots(item)?);
    }
    Ok(out)
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

/// Eval `valid_plugin(path)`: the string at `path` is a plugin bundle the
/// server would store (bundle rules, allowed tools, valid personas).
/// Returns the validator's error naming the file and field.
pub fn valid_plugin(output: &Value, args: &[Value]) -> Result<(), String> {
    let path = args.first().and_then(Value::as_str).filter(|_| args.len() == 1).ok_or("takes (path)")?;
    let text = lookup(output, path).into_iter().find_map(Value::as_str).ok_or_else(|| format!("no bundle text at `{path}`"))?;
    crate::plugins::check(text).map(|_| ()).map_err(|e| e.to_string())
}

const PRIVATE_SUFFIXES: [&str; 4] = [".localhost", ".local", ".internal", ".lan"];

fn public_url(text: &str) -> Result<(), String> {
    let url = url::Url::parse(text).map_err(|e| format!("`{text}` is not a URL: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("`{text}` is not http or https"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(format!("`{text}` carries credentials"));
    }
    let public = match url.host() {
        Some(url::Host::Ipv4(ip)) => troupekit_tools::http::is_public_ip(ip.into()),
        Some(url::Host::Ipv6(ip)) => troupekit_tools::http::is_public_ip(ip.into()),
        Some(url::Host::Domain(d)) => {
            let d = d.trim_end_matches('.').to_ascii_lowercase();
            d.contains('.') && d != "localhost" && !PRIVATE_SUFFIXES.iter().any(|s| d.ends_with(s))
        }
        None => false,
    };
    if !public {
        return Err(format!("`{text}` is not a public address"));
    }
    Ok(())
}

/// Eval `public_sources(path)`: every `url` of the list at `path` is an
/// http or https address on a public host, without credentials. An empty
/// list passes, so research without search still succeeds.
pub fn public_sources(output: &Value, args: &[Value]) -> Result<(), String> {
    let path = args.first().and_then(Value::as_str).filter(|_| args.len() == 1).ok_or("takes (path)")?;
    for url in lookup(output, &format!("{path}.url")) {
        public_url(url.as_str().ok_or_else(|| format!("{path}: a url is not a string"))?)?;
    }
    Ok(())
}

/// Phrases of questions that make the owner work something out instead of
/// telling what they know: what is left is the planner's sum, and a speed is
/// the planner's estimate from minutes and progress.
const COMPUTE_PHRASES: [&str; 12] =
    ["are left", "is left", "left to ", "left for ", "pages left", "chapters left", "remaining", "remain ", "per hour", "per minute", "in an hour", "pages an hour"];

/// Eval `owner_terms(path)`: no question at `path` asks the owner to work
/// something out (what is left, a reading speed), and every number question
/// names its unit and a range with `min` below `max`. Errors name each failing id.
pub fn owner_terms(output: &Value, args: &[Value]) -> Result<(), String> {
    let path = args.first().and_then(Value::as_str).filter(|_| args.len() == 1).ok_or("takes (path)")?;
    let mut failed = Vec::new();
    for q in lookup(output, path).into_iter().flat_map(|v| v.as_array().cloned().unwrap_or_default()) {
        let id = q.get("id").and_then(Value::as_str).unwrap_or("?").to_string();
        let text = q.get("text").and_then(Value::as_str).unwrap_or_default().to_lowercase();
        if COMPUTE_PHRASES.iter().any(|p| text.contains(p)) {
            failed.push(format!("{id}: ask what the owner knows (such as chapters read or minutes a day), never what they must work out"));
        }
        if q.get("kind").and_then(Value::as_str) == Some("number") {
            let unit = q.get("unit").and_then(Value::as_str).map(str::trim).unwrap_or_default();
            let (min, max) = (q.get("min").and_then(Value::as_f64), q.get("max").and_then(Value::as_f64));
            if unit.is_empty() || !matches!((min, max), (Some(lo), Some(hi)) if lo < hi) {
                failed.push(format!("{id}: a number question needs a unit and a range with min below max"));
            }
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("; "))
    }
}

/// Builds the eval registry every Aroh run uses: the built-in `min_items`
/// plus the domain evals `no_overlap`, `within_availability`, `valid_plugin`,
/// `keeps_fixed`, `public_sources` and `owner_terms`. Returns the registry, with no busy time
/// or fixed action bound.
pub fn registry() -> EvalRegistry {
    registry_with_busy(Vec::new(), Default::default())
}

/// Rejects a move, resize or delete at `path` whose `action_id` is in `fixed`,
/// the locked actions no agent changes (Q53). The error names the action so
/// the repair round asks for a change of a flexible one instead.
fn keeps_fixed(output: &Value, args: &[Value], fixed: &BTreeSet<String>) -> Result<(), String> {
    let [path] = args else { return Err("takes (path)".to_string()) };
    let path = path.as_str().ok_or("takes a path to a list of changes")?;
    for change in lookup(output, path).into_iter().flat_map(|v| v.as_array().cloned().unwrap_or_else(|| vec![v.clone()])) {
        let op = change.get("op").and_then(Value::as_str).unwrap_or("");
        let id = change.get("action_id").and_then(Value::as_str).unwrap_or("");
        if matches!(op, "move" | "resize" | "delete") && fixed.contains(id) {
            let title = change.get("title").and_then(Value::as_str).unwrap_or(id);
            return Err(format!("`{title}` is a fixed action: never move, resize or delete it; change a flexible action instead"));
        }
    }
    Ok(())
}

/// Builds the registry with `busy` bound into `no_overlap`, so the items at
/// its path also never overlap a kept device event (busy slots are never
/// checked against each other), and the `fixed` action ids into `keeps_fixed`.
pub fn registry_with_busy(busy: Vec<Slot>, fixed: BTreeSet<String>) -> EvalRegistry {
    let mut r = EvalRegistry::new();
    r.register("keeps_fixed", Arc::new(move |output: &Value, args: &[Value]| keeps_fixed(output, args, &fixed))).expect("fresh name");
    r.register("public_sources", Arc::new(public_sources)).expect("fresh name");
    r.register("owner_terms", Arc::new(owner_terms)).expect("fresh name");
    let overlap = move |output: &Value, args: &[Value]| {
        if args.len() != 1 {
            return Err("takes (path)".to_string());
        }
        clash(path_slots(output, args)?, &busy)
    };
    r.register("no_overlap", Arc::new(overlap)).expect("fresh name");
    r.register("within_availability", Arc::new(within_availability)).expect("fresh name");
    r.register("valid_plugin", Arc::new(valid_plugin)).expect("fresh name");
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
    fn keeps_fixed_rejects_any_change_to_a_locked_action() {
        let fixed: BTreeSet<String> = ["a-fixed".to_string()].into();
        let registry = registry_with_busy(vec![], fixed);
        let keeps = registry.get("keeps_fixed").unwrap();
        let change = |op: &str, id: Option<&str>| json!({"op": op, "action_id": id, "title": "Dentist", "start": null, "minutes": null});
        for op in ["move", "resize", "delete"] {
            let err = keeps(&json!({"changes": [change(op, Some("a-fixed"))]}), &[json!("changes")]).unwrap_err();
            assert!(err.contains("Dentist") && err.contains("fixed"), "{err}");
        }
        assert!(keeps(&json!({"changes": [change("move", Some("a-free")), change("create", None)]}), &[json!("changes")]).is_ok());
        assert!(keeps(&json!({"changes": []}), &[]).is_err(), "takes (path)");
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

    #[test]
    fn no_overlap_rejects_busy_clash_and_allows_overlapping_busy() {
        let busy = busy_slots(&json!([
            {"title": "Standup", "start": "2026-11-23T09:00:00", "minutes": 30},
            {"title": "Sync", "start": "2026-11-23T09:15:00", "minutes": 30}
        ]))
        .unwrap();
        let r = registry_with_busy(busy, Default::default());
        let f = r.get("no_overlap").unwrap();
        let clear = plan(json!([{"title": "Run", "start": "2026-11-23T07:00:00", "minutes": 30, "rrule": "FREQ=DAILY"}]));
        assert!(f(&clear, &[json!("items")]).is_ok());
        let over = plan(json!([{"title": "Run", "start": "2026-11-23T09:40:00", "minutes": 30}]));
        let e = f(&over, &[json!("items")]).unwrap_err();
        assert!(e.contains("overlaps busy `Sync`"), "{e}");
        let daily = plan(json!([{"title": "Core", "start": "2026-11-22T09:00:00", "minutes": 20, "rrule": "FREQ=DAILY"}]));
        assert!(f(&daily, &[json!("items")]).unwrap_err().contains("busy `Standup`"));
        assert!(registry().get("no_overlap").unwrap()(&over, &[json!("items")]).is_ok());
        assert!(f(&over, &[]).is_err());
        assert!(busy_slots(&json!([{"title": "x", "start": "soon", "minutes": 5}])).is_err());
        assert!(busy_slots(&Value::Null).unwrap().is_empty());
    }

    #[test]
    fn valid_plugin_checks_the_bundle_text() {
        let args = [json!("bundle_toml")];
        let good = json!({"bundle_toml": aroh_plugins::template("fitness").unwrap()});
        assert!(valid_plugin(&good, &args).is_ok());
        let e = valid_plugin(&json!({"bundle_toml": "aroh_plugin = 1\nname = \"x1\"\ntitle = \"X\""}), &args).unwrap_err();
        assert!(e.contains("plugin.toml: sections: missing"), "{e}");
        let tool = format!("tools = [\"send_mail\"]\n{}", aroh_plugins::template("fitness").unwrap());
        assert!(valid_plugin(&json!({"bundle_toml": tool}), &args).unwrap_err().contains("unknown tool `send_mail`"));
        let persona = format!("{}\n[[personas]]\nname = \"coach\"\ntoml = \"name = 3\"\n", aroh_plugins::template("fitness").unwrap());
        assert!(valid_plugin(&json!({"bundle_toml": persona}), &args).unwrap_err().contains("personas[0].toml"));
        assert!(valid_plugin(&json!({}), &args).is_err());
        assert!(valid_plugin(&good, &[]).is_err());
        assert!(registry().get("valid_plugin").is_some());
    }

    #[test]
    fn public_sources_accepts_public_and_empty_lists_only() {
        let args = [json!("sources")];
        let with = |urls: &[&str]| json!({"sources": urls.iter().map(|u| json!({"title": "t", "url": u})).collect::<Vec<_>>()});
        assert!(public_sources(&with(&[]), &args).is_ok());
        assert!(public_sources(&json!({"summary": "no search"}), &args).is_ok());
        assert!(public_sources(&with(&["https://example.org/core", "http://8.8.8.8/x", "https://[2606:4700::1]/"]), &args).is_ok());
        for bad in [
            "http://localhost:8080/x",
            "http://127.0.0.1/x",
            "http://10.0.0.5/x",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest",
            "http://[::1]/",
            "https://user:pw@example.org/",
            "file:///etc/passwd",
            "https://printer.local/",
            "https://intranet/",
            "not a url",
        ] {
            assert!(public_sources(&with(&["https://example.org/", bad]), &args).is_err(), "{bad}");
        }
        assert!(public_sources(&json!({"sources": [{"title": "t", "url": 3}]}), &args).is_err());
        assert!(public_sources(&with(&[]), &[]).is_err());
        assert!(registry().get("public_sources").is_some());
    }

    /// The six intake questions of the owner's stuck production run, in the
    /// shape the new schema takes, with the units and bounds they lacked.
    fn prod_intake() -> Value {
        json!({"questions": [
            {"id": "pages_remaining", "kind": "number", "text": "About how many pages are left for you to read in The Joy of X?", "choices": [], "unit": "pages", "min": 0, "max": 400},
            {"id": "daily_reading_minutes", "kind": "number", "text": "On a typical day this week, how many minutes can you set aside for reading?", "choices": [], "unit": "minutes per day", "min": 0, "max": 180},
            {"id": "preferred_reading_time", "kind": "choice", "text": "When do you prefer to read?", "choices": ["Early morning", "Midday", "Evening", "Late night", "It varies"], "unit": null, "min": null, "max": null},
            {"id": "reading_pace", "kind": "number", "text": "Roughly how many pages do you read per hour?", "choices": [], "unit": "pages per hour", "min": 0, "max": 200},
            {"id": "book_format", "kind": "choice", "text": "What format are you reading the book in?", "choices": ["Print book", "E-reader", "Phone or tablet", "Audiobook"], "unit": null, "min": null, "max": null},
            {"id": "busy_days_limits", "kind": "text", "text": "Are there days this week when you will be unavailable or have limits like eye strain or travel?", "choices": [], "unit": null, "min": null, "max": null}
        ]})
    }

    #[test]
    fn owner_terms_fails_questions_the_owner_must_work_out() {
        let args = [json!("questions")];
        let err = owner_terms(&prod_intake(), &args).unwrap_err();
        assert!(err.contains("pages_remaining") && err.contains("reading_pace"), "{err}");
        assert!(!err.contains("daily_reading_minutes"), "minutes per day is the owner's own number: {err}");
        let mut fixed = prod_intake();
        fixed["questions"][0] = json!({"id": "chapters_done", "kind": "number", "text": "How many chapters have you read so far?", "choices": [], "unit": "chapters", "min": 0, "max": 30});
        fixed["questions"][3] = json!({"id": "reading_days", "kind": "number", "text": "On how many days this week can you read?", "choices": [], "unit": "days", "min": 1, "max": 7});
        assert_eq!(owner_terms(&fixed, &args), Ok(()));
        for (key, value) in [("unit", json!(null)), ("unit", json!(" ")), ("min", json!(null)), ("max", json!(null)), ("max", json!(-1)), ("min", json!(180))] {
            let mut bad = fixed.clone();
            bad["questions"][1][key] = value.clone();
            assert!(owner_terms(&bad, &args).unwrap_err().contains("daily_reading_minutes"), "{key} = {value}");
        }
        let mut speed = fixed.clone();
        speed["questions"][5]["text"] = json!("How many pages can you read in an hour?");
        assert!(owner_terms(&speed, &args).is_err());
        let mut plain = fixed.clone();
        plain["questions"][5]["text"] = json!("Do you have an hour a day, or pain in your left knee?");
        assert_eq!(owner_terms(&plain, &args), Ok(()), "an hour a day and a left knee are the owner's own words");
        assert!(owner_terms(&fixed, &[]).is_err());
        assert!(registry().get("owner_terms").is_some());
    }
}
