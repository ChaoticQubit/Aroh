use crate::error::ServerError;
use crate::rows::{get_row, put_row};
use crate::tools::{local_text, ToolEnv};
use aroh_domain::calendar::{rules_for, DeviceEvent, RuleSource, DAY_GROUP, MAX_NAME};
use aroh_domain::rows::{CalendarRule, DayKind, RuleDecision};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::sync::Arc;
use troupekit_tools::{typed_tool, ToolContext, ToolError, ToolOutput, Toolkit};
use utoipa::ToSchema;

/// Most events one triage request may carry, one per series.
/// Larger calendars are sent in several runs, one after another.
/// Checked before the run starts.
pub const MAX_EVENTS: usize = 300;

const OTHER: &str = "Other";

/// One device calendar event to triage: EventKit ids, calendar name, title,
/// first occurrence (RFC 3339), the RFC 5545 recurrence when it repeats and
/// whether it is all-day. Notes, places and attendees never leave the device.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TriageEvent {
    pub event_id: String,
    pub calendar_id: String,
    pub calendar: String,
    pub title: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub rrule: Option<String>,
    #[serde(default)]
    pub all_day: bool,
}

/// Body of `POST /v1/calendar/triage`: the owner's IANA zone and 1 to 300
/// unmatched events. The run groups them and waits at `decide` for the
/// owner's keep or ignore per group, then writes `calendar_rules`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StartTriage {
    pub timezone: String,
    pub events: Vec<TriageEvent>,
}

/// Builds the run input of a triage: `events` for the persona (short keys,
/// local times) and `refs` for the rules tool, sorted by start, title and id
/// with one entry per event id. Returns `BadRequest` for a bad request.
pub fn triage_input(req: &StartTriage, tz: Tz) -> Result<Map<String, Value>, ServerError> {
    if req.events.is_empty() || req.events.len() > MAX_EVENTS {
        return Err(ServerError::BadRequest(format!("events must hold 1 to {MAX_EVENTS} events")));
    }
    let mut events: Vec<&TriageEvent> = req.events.iter().collect();
    for e in &events {
        let device = DeviceEvent {
            calendar_id: e.calendar_id.clone(),
            calendar: e.calendar.clone(),
            event_id: e.event_id.clone(),
            title: e.title.clone(),
            start_at: e.start_at,
            end_at: e.end_at,
            all_day: e.all_day,
            rrule: e.rrule.clone(),
            holiday: false,
        };
        device.validate().map_err(|e| ServerError::BadRequest(e.to_string()))?;
    }
    events.sort_by(|a, b| (a.start_at, &a.title, &a.event_id).cmp(&(b.start_at, &b.title, &b.event_id)));
    let mut seen = BTreeSet::new();
    events.retain(|e| seen.insert(e.event_id.clone()));
    let mut shown = Vec::with_capacity(events.len());
    let mut refs = Vec::with_capacity(events.len());
    for (i, e) in events.iter().enumerate() {
        let key = format!("e{}", i + 1);
        shown.push(json!({
            "key": key,
            "calendar": e.calendar,
            "title": e.title,
            "start": local_text(e.start_at.with_timezone(&tz).naive_local()),
            "minutes": (e.end_at - e.start_at).num_minutes().clamp(1, 24 * 60),
            "rrule": e.rrule,
            "all_day": e.all_day,
        }));
        refs.push(json!({"key": key, "event_id": e.event_id, "calendar_id": e.calendar_id, "title": e.title, "recurring": e.rrule.is_some(), "all_day": e.all_day}));
    }
    let mut input = Map::new();
    input.insert("events".into(), Value::Array(shown));
    input.insert("refs".into(), Value::Array(refs));
    Ok(input)
}

/// One triage group: name, kind, the keep suggestion, a reason and the keys
/// of its events. The persona writes them; `sort_groups` normalizes them.
/// The owner's decision is per name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Group {
    pub name: String,
    pub kind: String,
    pub keep: bool,
    pub reason: String,
    pub events: Vec<String>,
}

/// The persona's proposed day kind for one all-day event key: `reminder`,
/// `busy` or `away` (Q47). Shown preselected on the app's choice card and
/// never applied without the owner's tap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DayPick {
    pub event: String,
    pub kind: String,
}

/// The groups of a triage, as the `triage` and `sorted` state keys hold them,
/// plus the day kinds proposed for all-day events. After `sort_groups` every
/// event key is in exactly one group and each all-day key has at most one pick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Groups {
    pub groups: Vec<Group>,
    #[serde(default)]
    pub days: Vec<DayPick>,
}

/// Keeps the persona's day picks that name an all-day key of `all_day` with a
/// known kind, first pick per key, in key order of the picks.
/// Returns the cleaned picks.
pub fn sort_days(all_day: &BTreeSet<String>, days: Vec<DayPick>) -> Vec<DayPick> {
    let mut seen = BTreeSet::new();
    let out: Vec<DayPick> = days
        .into_iter()
        .filter(|d| all_day.contains(&d.event) && serde_json::from_value::<DayKind>(Value::String(d.kind.clone())).is_ok())
        .filter(|d| seen.insert(d.event.clone()))
        .collect();
    assert!(out.len() <= all_day.len(), "at most one pick per all-day event");
    out
}

/// Normalizes the persona's groups over the run's event `keys`: unknown and
/// repeated keys dropped, groups of the same name merged, empty groups
/// dropped, ungrouped events put in an `Other` group suggested as ignore.
pub fn sort_groups(keys: &[String], groups: Vec<Group>) -> Groups {
    let known: BTreeSet<&str> = keys.iter().map(String::as_str).collect();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<Group> = Vec::new();
    let add = |out: &mut Vec<Group>, g: Group, members: Vec<String>| {
        if let Some(existing) = out.iter_mut().find(|o| o.name.to_lowercase() == g.name.to_lowercase()) {
            existing.events.extend(members);
        } else if !members.is_empty() {
            out.push(Group { events: members, ..g });
        }
    };
    for mut g in groups {
        g.name = g.name.trim().chars().take(MAX_NAME).collect();
        if g.name.is_empty() {
            g.name = OTHER.into();
        }
        let members: Vec<String> = g.events.iter().filter(|k| known.contains(k.as_str()) && taken.insert((*k).clone())).cloned().collect();
        add(&mut out, g, members);
    }
    let rest: Vec<String> = keys.iter().filter(|k| !taken.contains(*k)).cloned().collect();
    if !rest.is_empty() {
        let other = Group { name: OTHER.into(), kind: "noise".into(), keep: false, reason: "Events the triage left ungrouped.".into(), events: Vec::new() };
        add(&mut out, other, rest);
    }
    let placed: usize = out.iter().map(|g| g.events.len()).sum();
    assert_eq!(placed, keys.len(), "every event is in exactly one group");
    Groups { groups: out, days: Vec::new() }
}

/// One event key of the `events` state key and whether the event is all-day.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct EventKey {
    pub key: String,
    #[serde(default)]
    pub all_day: bool,
}

/// Sorts the persona's `triage` over the run's `events`: groups over the timed keys only, so
/// an all-day key never enters a group or `Other`, and day picks over the all-day keys.
/// Output: the groups the owner answers, empty for an all-day-only run.
pub fn sort_triage(events: &[EventKey], triage: Groups) -> Groups {
    let all_day: BTreeSet<String> = events.iter().filter(|e| e.all_day).map(|e| e.key.clone()).collect();
    let keys: Vec<String> = events.iter().filter(|e| !e.all_day).map(|e| e.key.clone()).collect();
    let mut sorted = sort_groups(&keys, triage.groups);
    sorted.days = sort_days(&all_day, triage.days);
    sorted
}

#[derive(Deserialize, JsonSchema)]
struct SortArgs {
    events: Vec<EventKey>,
    triage: Groups,
}

/// One event of the `refs` state key: its short key, EventKit ids, title,
/// whether it repeats and whether it is all-day. The rules tool builds rules
/// from these, never from model output.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct EventRef {
    pub key: String,
    pub event_id: String,
    pub calendar_id: String,
    pub title: String,
    pub recurring: bool,
    #[serde(default)]
    pub all_day: bool,
}

/// The owner's answer for one group of the `decide` node: the group name
/// and whether its events are kept as busy time.
/// The app sends one per group.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Decision {
    pub group: String,
    pub keep: bool,
}

#[derive(Deserialize, JsonSchema)]
struct RulesArgs {
    refs: Vec<EventRef>,
    sorted: Groups,
    decisions: Vec<Decision>,
}

async fn write_rules(env: &ToolEnv, args: RulesArgs) -> Result<Value, ToolError> {
    let user = env.user.to_string();
    let (mut rules, mut kept, mut ignored) = (0, 0, 0);
    for g in &args.sorted.groups {
        let decision = args
            .decisions
            .iter()
            .find(|d| d.group.trim().to_lowercase() == g.name.to_lowercase())
            .ok_or_else(|| ToolError::InvalidArgs(format!("no decision for group `{}`", g.name)))?;
        let members: Vec<RuleSource> = g
            .events
            .iter()
            .filter_map(|k| args.refs.iter().find(|r| &r.key == k))
            .map(|r| RuleSource { calendar_id: r.calendar_id.clone(), event_id: r.event_id.clone(), title: r.title.clone(), recurring: r.recurring })
            .collect();
        let choice = if decision.keep { RuleDecision::Keep } else { RuleDecision::Ignore };
        for mut rule in rules_for(&user, &g.name, choice, &members) {
            if let Some(old) = get_row::<CalendarRule>(&env.pool, env.user, &rule.id).await.map_err(|e| ToolError::Failed(e.to_string()))? {
                rule.updated_at = old.updated_at;
                rule.day_kind = old.day_kind;
            }
            put_row(&env.pool, &env.hlc, env.user, &mut rule).await.map_err(|e| ToolError::Failed(e.to_string()))?;
            rules += 1;
        }
        if decision.keep {
            kept += 1;
        } else {
            ignored += 1;
        }
    }
    for r in args.refs.iter().filter(|r| r.all_day) {
        let source = RuleSource { calendar_id: r.calendar_id.clone(), event_id: r.event_id.clone(), title: r.title.clone(), recurring: true };
        let mut day = rules_for(&user, DAY_GROUP, RuleDecision::Keep, &[source]);
        assert_eq!(day.len(), 1, "one all-day event gives one series rule");
        let mut rule = day.remove(0);
        if let Some(old) = get_row::<CalendarRule>(&env.pool, env.user, &rule.id).await.map_err(|e| ToolError::Failed(e.to_string()))? {
            rule.updated_at = old.updated_at;
            rule.day_kind = old.day_kind;
        }
        put_row(&env.pool, &env.hlc, env.user, &mut rule).await.map_err(|e| ToolError::Failed(e.to_string()))?;
        rules += 1;
    }
    Ok(json!({"rules": rules, "kept": kept, "ignored": ignored}))
}

/// The calendar toolkit of one run: `sort_groups` and `write_rules`, used by
/// the tool nodes of the calendar-triage workflow. Rules are written as
/// `env.user`; no goal is needed.
pub fn triage_toolkit(env: ToolEnv) -> Toolkit {
    let sort = typed_tool("sort_groups", "Normalize triage groups so every event is in exactly one group.", |a: SortArgs, _ctx: ToolContext| async move {
        let sorted = sort_triage(&a.events, a.triage);
        Ok(ToolOutput::text(serde_json::to_string(&sorted).expect("groups serialize")))
    });
    let write = typed_tool("write_rules", "Store the owner's keep or ignore decisions as calendar rules.", move |a: RulesArgs, _ctx: ToolContext| {
        let env = env.clone();
        async move { write_rules(&env, a).await.map(|v| ToolOutput::text(v.to_string())) }
    });
    Toolkit::new("calendar").with(Arc::new(sort)).with(Arc::new(write))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(name: &str, keep: bool, events: &[&str]) -> Group {
        Group { name: name.into(), kind: "work".into(), keep, reason: "r".into(), events: events.iter().map(|e| e.to_string()).collect() }
    }

    #[test]
    fn sort_groups_merges_dedupes_and_adds_other() {
        let keys: Vec<String> = ["e1", "e2", "e3", "e4", "e5"].iter().map(|k| k.to_string()).collect();
        let sorted = sort_groups(&keys, vec![
            group(" Work ", true, &["e1", "e9", "e1"]),
            group("work", true, &["e2"]),
            group("Gym", true, &["e2", "e3"]),
            group("Empty", false, &["e7"]),
            group("", false, &["e4"]),
        ]);
        let names: Vec<&str> = sorted.groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Work", "Gym", "Other"]);
        assert_eq!(sorted.groups[0].events, ["e1", "e2"]);
        assert_eq!(sorted.groups[1].events, ["e3"]);
        assert_eq!(sorted.groups[2].events, ["e4", "e5"]);
        assert!(sort_groups(&[], vec![group("Work", true, &["e1"])]).groups.is_empty());
    }

    #[test]
    fn sort_days_keeps_one_known_kind_per_all_day_event() {
        let all_day: BTreeSet<String> = ["e2", "e3"].iter().map(|k| k.to_string()).collect();
        let pick = |e: &str, k: &str| DayPick { event: e.into(), kind: k.into() };
        let days = sort_days(&all_day, vec![pick("e1", "busy"), pick("e2", "away"), pick("e2", "busy"), pick("e3", "holiday"), pick("e9", "reminder")]);
        assert_eq!(days, vec![pick("e2", "away")]);
        assert!(sort_days(&BTreeSet::new(), vec![pick("e2", "away")]).is_empty());
    }

    #[test]
    fn sort_triage_keeps_all_day_keys_out_of_every_group() {
        let key = |k: &str, all_day: bool| EventKey { key: k.into(), all_day };
        let events = [key("e1", false), key("e2", true), key("e3", false), key("e4", true)];
        let picks = vec![DayPick { event: "e2".into(), kind: "busy".into() }, DayPick { event: "e3".into(), kind: "away".into() }];
        let sorted = sort_triage(&events, Groups { groups: vec![group("Work", true, &["e1", "e2"])], days: picks });
        let grouped: Vec<&str> = sorted.groups.iter().flat_map(|g| g.events.iter().map(String::as_str)).collect();
        assert_eq!(grouped, ["e1", "e3"]);
        assert_eq!(sorted.groups[1].name, "Other");
        assert_eq!(sorted.days, vec![DayPick { event: "e2".into(), kind: "busy".into() }]);
        let only_days = sort_triage(&[key("e1", true)], Groups { groups: vec![group("Trips", true, &["e1"])], days: vec![] });
        assert!(only_days.groups.is_empty(), "an all-day-only triage has no group");
    }

    #[test]
    fn triage_input_sorts_keys_and_rejects_bad_events() {
        let at = |h: u32| format!("2026-11-23T{h:02}:00:00Z").parse::<DateTime<Utc>>().unwrap();
        let ev = |id: &str, title: &str, h: u32| TriageEvent {
            event_id: id.into(),
            calendar_id: "work".into(),
            calendar: "Work".into(),
            title: title.into(),
            start_at: at(h),
            end_at: at(h + 1),
            rrule: None,
            all_day: false,
        };
        let req = StartTriage { timezone: "Europe/Berlin".into(), events: vec![ev("b", "Later", 10), ev("a", "First", 8), ev("a", "First", 9)] };
        let input = triage_input(&req, chrono_tz::Europe::Berlin).unwrap();
        assert_eq!(input["events"][0], json!({"key": "e1", "calendar": "Work", "title": "First", "start": "2026-11-23T09:00:00", "minutes": 60, "rrule": null, "all_day": false}));
        assert_eq!(input["events"].as_array().unwrap().len(), 2);
        assert_eq!(input["refs"][1]["event_id"], "b");
        let mut bad = req.clone();
        bad.events[0].end_at = bad.events[0].start_at;
        assert!(triage_input(&bad, chrono_tz::UTC).is_err());
        let empty = StartTriage { timezone: "UTC".into(), events: vec![] };
        assert!(triage_input(&empty, chrono_tz::UTC).is_err());
        let mut trip = ev("t", "Trip", 0);
        (trip.all_day, trip.end_at) = (true, at(0) + chrono::Duration::days(3));
        let input = triage_input(&StartTriage { timezone: "UTC".into(), events: vec![trip] }, chrono_tz::UTC).unwrap();
        assert_eq!((input["events"][0]["all_day"].clone(), input["events"][0]["minutes"].clone()), (json!(true), json!(1440)));
        assert_eq!(input["refs"][0]["all_day"], json!(true), "the rules tool knows which events are all-day");
        let many = StartTriage { timezone: "UTC".into(), events: (0..=MAX_EVENTS).map(|i| ev(&format!("x{i}"), "t", 8)).collect() };
        assert!(triage_input(&many, chrono_tz::UTC).is_err());
    }
}
