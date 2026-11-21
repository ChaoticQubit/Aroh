//! The six-pack sample: one fitness goal with two side goals, series, trackers, a week of
//! history, personas, a proposal and a suggestion. Used by tests and debug builds only.

use crate::Result;
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{
    Action, ActionSeries, ActionStatus, Autonomy, Budget, BudgetScope, Cadence, CadenceFreq, EntrySource, FieldKind, Goal, GoalStatus,
    Persona, Preference, Proposal, ProposalSize, ProposalStatus, Tracker, TrackerEntry, TrackerField,
};
use aroh_store::repo;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::Connection;
use serde_json::{json, Map};

const HISTORY_DAYS: u32 = 7;
const WEIGHT_DAYS: i64 = 21;
const PROTEIN: [f64; 7] = [138.0, 152.0, 118.0, 145.0, 162.0, 131.0, 155.0];

fn local(zone: Tz, date: NaiveDate, time: &str) -> DateTime<Utc> {
    let time = NaiveTime::parse_from_str(time, "%H:%M").expect("sample times are HH:MM");
    zone.from_local_datetime(&date.and_time(time)).earliest().expect("sample times exist in the zone").with_timezone(&Utc)
}

fn goal(conn: &Connection, clock: &mut Hlc, user_id: &str, title: &str, category: &str, deadline: Option<&str>, autonomy: Autonomy, now: DateTime<Utc>) -> Result<Goal> {
    let category_id = aroh_domain::seed::categories(user_id).into_iter().find(|c| c.name == category).map(|c| c.id);
    assert!(category_id.is_some(), "sample category {category} is seeded");
    let mut row = Goal {
        id: aroh_domain::new_id(),
        user_id: user_id.into(),
        category_id,
        title: title.into(),
        context: None,
        deadline: deadline.map(str::to_string),
        status: GoalStatus::Active,
        autonomy,
        created_at: now,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut row)?;
    Ok(row)
}

struct SeriesSpec {
    title: &'static str,
    rrule: &'static str,
    at: &'static str,
    minutes: i64,
}

fn series(conn: &Connection, clock: &mut Hlc, goal: &Goal, spec: &SeriesSpec, start: NaiveDate, zone: Tz) -> Result<ActionSeries> {
    let mut row = ActionSeries {
        id: aroh_domain::new_id(),
        user_id: goal.user_id.clone(),
        goal_id: goal.id.clone(),
        title: spec.title.into(),
        notes: None,
        rrule: spec.rrule.into(),
        dtstart_local: format!("{start}T{}:00", spec.at),
        timezone: zone.name().into(),
        duration_minutes: spec.minutes,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut row)?;
    Ok(row)
}

fn entry(conn: &Connection, clock: &mut Hlc, tracker: &Tracker, key: &str, value: f64, at: DateTime<Utc>) -> Result<()> {
    let mut data = Map::new();
    data.insert(key.into(), json!(value));
    let mut row = TrackerEntry {
        id: aroh_domain::new_id(),
        user_id: tracker.user_id.clone(),
        tracker_id: tracker.id.clone(),
        data,
        source: EntrySource::CheckIn,
        raw_text: None,
        logged_at: at,
        updated_at: String::new(),
        deleted: false,
    };
    aroh_domain::rows::validate_entry(tracker, &row)?;
    repo::put(conn, clock, &mut row)?;
    Ok(())
}

fn tracker(conn: &Connection, clock: &mut Hlc, goal: &Goal, name: &str, field: TrackerField, cadence: Option<Cadence>) -> Result<Tracker> {
    let mut row = Tracker {
        id: aroh_domain::new_id(),
        user_id: goal.user_id.clone(),
        goal_id: goal.id.clone(),
        plugin_id: None,
        name: name.into(),
        fields: vec![field],
        cadence,
        rules: vec![],
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut row)?;
    Ok(row)
}

fn number(key: &str, label: &str, unit: &str) -> TrackerField {
    TrackerField { key: key.into(), label: label.into(), kind: FieldKind::Number, unit: Some(unit.into()), choices: vec![] }
}

fn history(conn: &Connection, clock: &mut Hlc, all: &[ActionSeries], strength_id: &str, now: DateTime<Utc>) -> Result<Option<Action>> {
    let from = now - Duration::days(i64::from(HISTORY_DAYS));
    let mut missed = None;
    for s in all {
        for mut action in repo::materialize(conn, &s.id, from, HISTORY_DAYS)? {
            if action.end_at > now {
                continue;
            }
            action.status = ActionStatus::Done;
            action.done_at = Some(action.end_at);
            if s.id == strength_id {
                missed = Some(action.clone());
            }
            repo::put(conn, clock, &mut action)?;
        }
    }
    let Some(mut last) = missed else { return Ok(None) };
    last.status = ActionStatus::Missed;
    last.done_at = None;
    repo::put(conn, clock, &mut last)?;
    Ok(Some(last))
}

fn proposal(conn: &Connection, clock: &mut Hlc, goal: &Goal, missed: &Action, now: DateTime<Utc>, zone: Tz) -> Result<()> {
    let tomorrow = (now.with_timezone(&zone) + Duration::days(1)).date_naive();
    let start = local(zone, tomorrow, "18:00");
    let label = |t: DateTime<Utc>| t.with_timezone(&zone).format("%a %-d %b %H:%M").to_string();
    let change = json!({
        "op": "move",
        "action_id": missed.id,
        "title": missed.title,
        "before": label(missed.start_at),
        "after": label(start),
        "start_at": start,
        "end_at": start + Duration::hours(1),
    });
    let mut row = Proposal {
        id: aroh_domain::new_id(),
        user_id: goal.user_id.clone(),
        goal_id: goal.id.clone(),
        run_id: None,
        summary: format!("Move the missed strength session to {}.", label(start)),
        diff: json!({ "changes": [change] }),
        size: ProposalSize::Small,
        status: ProposalStatus::Proposed,
        undo: None,
        created_at: now,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut row)?;
    Ok(())
}

fn extras(conn: &Connection, clock: &mut Hlc, goal: &Goal, now: DateTime<Utc>) -> Result<()> {
    for name in ["Fitness coach", "Nutritionist", "Planner"] {
        let mut persona = Persona {
            id: aroh_domain::new_id(),
            user_id: goal.user_id.clone(),
            goal_id: Some(goal.id.clone()),
            name: name.into(),
            toml: format!("name = \"{name}\"\n"),
            updated_at: String::new(),
            deleted: false,
        };
        repo::put(conn, clock, &mut persona)?;
    }
    let mut preference = Preference {
        id: aroh_domain::new_id(),
        user_id: goal.user_id.clone(),
        fact: "You moved workouts to evenings 3 times. Plan them 18:00 to 20:00 from now on?".into(),
        source: "Planner".into(),
        noted_at: now,
        confirmed: false,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut preference)?;
    let mut budget = Budget {
        id: aroh_domain::new_id(),
        user_id: goal.user_id.clone(),
        scope: BudgetScope::UserMonth,
        goal_id: None,
        limit_usd_micros: 20_000_000,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut budget)?;
    Ok(())
}

/// Loads the six-pack sample for `user_id` when the user has no goal yet.
/// Series start four weeks before `now` in `zone`; the past week is materialized and marked
/// done except the last strength session, which is missed and has a move proposal.
/// Output: true when the sample was loaded, false when goals already existed.
pub fn seed(conn: &Connection, clock: &mut Hlc, user_id: &str, now: DateTime<Utc>, zone: Tz) -> Result<bool> {
    if !repo::list::<Goal>(conn, &[("user_id", user_id)])?.is_empty() {
        return Ok(false);
    }
    for category in aroh_domain::seed::categories(user_id) {
        repo::insert_derived(conn, &category)?;
    }
    let six = goal(conn, clock, user_id, "Six pack by Feb 1", "Health and Fitness", Some("2027-02-01"), Autonomy::Balanced, now)?;
    let piano = goal(conn, clock, user_id, "Learn piano", "Hobby and Skill", None, Autonomy::Ask, now)?;
    let save = goal(conn, clock, user_id, "Save $10k", "Financial", Some("2027-06-30"), Autonomy::Balanced, now)?;
    let start = (now - Duration::days(28)).with_timezone(&zone).date_naive();
    let plan = [
        (&six, SeriesSpec { title: "Strength session", rrule: "FREQ=WEEKLY;BYDAY=MO,WE,FR", at: "18:00", minutes: 60 }),
        (&six, SeriesSpec { title: "Core session", rrule: "FREQ=WEEKLY;BYDAY=TU,TH", at: "07:00", minutes: 30 }),
        (&six, SeriesSpec { title: "Meal prep", rrule: "FREQ=WEEKLY;BYDAY=SU", at: "10:00", minutes: 30 }),
        (&piano, SeriesSpec { title: "Piano practice", rrule: "FREQ=DAILY", at: "20:00", minutes: 30 }),
        (&save, SeriesSpec { title: "Weekly budget review", rrule: "FREQ=WEEKLY;BYDAY=FR", at: "12:00", minutes: 30 }),
    ];
    let mut all = Vec::new();
    for (g, spec) in &plan {
        all.push(series(conn, clock, g, spec, start, zone)?);
    }
    let strength_id = all[0].id.clone();
    let body = tracker(conn, clock, &six, "Weight", number("weight", "Weight", "kg"), Some(Cadence { freq: CadenceFreq::Daily, weekdays: vec![], at: "07:00".into() }))?;
    let food = tracker(conn, clock, &six, "Protein", number("protein", "Protein", "g"), None)?;
    let today = now.with_timezone(&zone).date_naive();
    for back in 1..=WEIGHT_DAYS {
        let weight = (840.0 - 0.8 * (WEIGHT_DAYS - back) as f64).round() / 10.0;
        entry(conn, clock, &body, "weight", weight, local(zone, today - Duration::days(back), "07:00"))?;
    }
    for (i, grams) in PROTEIN.iter().enumerate() {
        let at = local(zone, today - Duration::days(PROTEIN.len() as i64 - i as i64), "20:00");
        entry(conn, clock, &food, "protein", *grams, at)?;
    }
    if let Some(missed) = history(conn, clock, &all, &strength_id, now)? {
        proposal(conn, clock, &six, &missed, now, zone)?;
    }
    extras(conn, clock, &six, now)?;
    Ok(true)
}
