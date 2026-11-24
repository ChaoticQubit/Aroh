//! Dashboard command (Q69): the figures of every dashboard goal over the whole local history,
//! plus a debug-only seed of long goal histories for simulator checks.

use super::app::after_write;
use super::with_core;
use crate::{AppState, Core};
use aroh_domain::dashboard::Dashboard;
use aroh_domain::rows::{Action, ActionSeries, ActionStatus, Autonomy, EntrySource, FieldKind, Goal, GoalStatus, Plugin, Tracker, TrackerEntry, TrackerField};
use aroh_plugins::versions::active;
use aroh_store::repo;
use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use std::collections::BTreeMap;
use tauri::{AppHandle, State};

const FILE: &str = "plugin.toml";

/// Field targets per tracker id, from each goal's active plugin version: a bundle tracker's
/// targets go to the goal's tracker row of the same name. A bundle that no longer validates
/// gives no targets rather than failing the dashboard.
fn targets(plugins: &[Plugin], trackers: &[Tracker]) -> BTreeMap<String, BTreeMap<String, f64>> {
    let mut out = BTreeMap::new();
    for t in trackers {
        let Some(plugin) = active(plugins, &t.goal_id) else { continue };
        let Ok(bundle) = aroh_plugins::parse(FILE, &plugin.bundle_toml) else { continue };
        if let Some(b) = bundle.trackers.iter().find(|b| b.name == t.name) {
            out.insert(t.id.clone(), b.targets.clone());
        }
    }
    out
}

/// Reads the dashboard now, in the device zone: per goal done vs due, projected finish and
/// pace, streaks, completion, minutes per week and the tracker trend with its plugin target.
#[tauri::command]
pub fn app_dashboard(state: State<'_, AppState>) -> Result<Dashboard, String> {
    with_core(&state, |c| {
        let mine = [("user_id", c.user_id.as_str())];
        let plugins: Vec<Plugin> = repo::list(&c.conn, &mine)?;
        let trackers: Vec<Tracker> = repo::list(&c.conn, &mine)?;
        aroh_app::dashboard::read(&c.conn, &c.user_id, &targets(&plugins, &trackers), Utc::now(), crate::zone())
    })
}

/// One seeded goal of the long dashboard history: title, category slug, deadline in days from
/// now, days of past sessions, days between them, every how many sessions one is missed or
/// skipped, its future rule (`None` for no future sessions) and a tracker with a weekly step.
struct LongGoal {
    title: &'static str,
    category: &'static str,
    deadline_days: Option<i64>,
    past_days: i64,
    every: i64,
    miss: i64,
    skip: i64,
    rule: Option<&'static str>,
    tracker: Option<(&'static str, &'static str, f64, f64)>,
}

const LONG: [LongGoal; 4] = [
    LongGoal { title: "Finish reading Harbor Lights", category: "learning", deadline_days: Some(40), past_days: 56, every: 2, miss: 0, skip: 0, rule: Some("FREQ=DAILY;INTERVAL=2;COUNT=10"), tracker: Some(("Chapters read", "", 1.0, 2.0)) },
    LongGoal { title: "Learn Spanish basics well enough to order food and ask for directions on the trip", category: "learning", deadline_days: Some(45), past_days: 120, every: 1, miss: 4, skip: 9, rule: Some("FREQ=DAILY;COUNT=40"), tracker: Some(("Words learned", "", 40.0, 22.0)) },
    LongGoal { title: "Run a half marathon", category: "health-fitness", deadline_days: Some(90), past_days: 90, every: 2, miss: 11, skip: 0, rule: Some("FREQ=WEEKLY;BYDAY=MO,WE,SA;COUNT=36"), tracker: Some(("Long run", "km", 6.0, 0.8)) },
    LongGoal { title: "Meditate daily", category: "habit", deadline_days: None, past_days: 70, every: 1, miss: 5, skip: 0, rule: Some("FREQ=DAILY"), tracker: None },
];

/// Writes one long goal: past actions at 07:00 local with the given miss and skip pattern, a
/// future series from tomorrow, and a number tracker with one entry a week.
fn seed_goal(c: &mut Core, spec: &LongGoal, rank: i64, now: DateTime<Utc>) -> aroh_app::Result<()> {
    let zone = crate::zone();
    let today = now.with_timezone(&zone).date_naive();
    let at = |days: i64, hour: u32| zone.from_local_datetime(&(today + Duration::days(days)).and_time(NaiveTime::from_hms_opt(hour, 0, 0).expect("valid hour"))).earliest().expect("local time exists").with_timezone(&Utc);
    let mut goal = Goal {
        id: aroh_domain::new_id(),
        user_id: c.user_id.clone(),
        category_id: aroh_domain::seed::category_id(&c.user_id, spec.category),
        title: spec.title.into(),
        context: None,
        deadline: spec.deadline_days.map(|d| (today + Duration::days(d)).format("%Y-%m-%d").to_string()),
        status: GoalStatus::Active,
        autonomy: Autonomy::Ask,
        created_at: now - Duration::days(spec.past_days + 10 - rank),
        reminders: false,
        use_calendars: false,
        ordered: false,
        run_limit_minutes: None,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(&c.conn, &mut c.clock, &mut goal)?;
    for (n, back) in (1..=spec.past_days).rev().step_by(spec.every as usize).enumerate() {
        let n = n as i64 + 1;
        let status = if spec.miss > 0 && n % spec.miss == 0 { ActionStatus::Missed } else if spec.skip > 0 && n % spec.skip == 0 { ActionStatus::Skipped } else { ActionStatus::Done };
        let start = at(-back, 7);
        let mut a = Action {
            id: aroh_domain::new_id(),
            user_id: c.user_id.clone(),
            goal_id: goal.id.clone(),
            series_id: None,
            occurrence: None,
            title: "Session".into(),
            notes: None,
            start_at: start,
            end_at: start + Duration::minutes(20 + 10 * (n % 4)),
            status,
            locked: false,
            done_at: (status == ActionStatus::Done).then(|| start + Duration::minutes(30)),
            card: None,
            updated_at: String::new(),
            deleted: false,
        };
        repo::put(&c.conn, &mut c.clock, &mut a)?;
    }
    if let Some(rule) = spec.rule {
        let mut s = ActionSeries {
            id: aroh_domain::new_id(),
            user_id: c.user_id.clone(),
            goal_id: goal.id.clone(),
            title: "Session".into(),
            notes: None,
            rrule: rule.into(),
            dtstart_local: format!("{}T07:00:00", (today + Duration::days(1)).format("%Y-%m-%d")),
            timezone: zone.name().into(),
            duration_minutes: 30,
            card: None,
            updated_at: String::new(),
            deleted: false,
        };
        repo::put(&c.conn, &mut c.clock, &mut s)?;
    }
    if let Some((label, unit, start, step)) = spec.tracker {
        let key = "value";
        let mut t = Tracker {
            id: aroh_domain::new_id(),
            user_id: c.user_id.clone(),
            goal_id: goal.id.clone(),
            plugin_id: None,
            name: label.into(),
            fields: vec![TrackerField { key: key.into(), label: label.into(), kind: FieldKind::Number, unit: (!unit.is_empty()).then(|| unit.into()), choices: vec![], health: None }],
            cadence: None,
            rules: vec![],
            updated_at: String::new(),
            deleted: false,
        };
        repo::put(&c.conn, &mut c.clock, &mut t)?;
        for week in 0..8i64 {
            let value = ((start + step * week as f64) * 10.0).round() / 10.0;
            let mut e = TrackerEntry {
                id: aroh_domain::new_id(),
                user_id: c.user_id.clone(),
                tracker_id: t.id.clone(),
                data: serde_json::json!({ key: value }).as_object().expect("object").clone(),
                source: EntrySource::Log,
                raw_text: None,
                logged_at: at(-7 * (7 - week) - 1, 20),
                updated_at: String::new(),
                deleted: false,
            };
            repo::put(&c.conn, &mut c.clock, &mut e)?;
        }
    }
    Ok(())
}

/// Debug builds only: seeds the sample goals, then four goals with up to 120 days of history,
/// bounded and open-ended future plans and weekly tracker entries, so the Dashboard shows
/// more than six goals, every pace and long titles. Ticks after, so series materialize.
#[tauri::command]
pub fn app_dev_dashboard_seed(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if !cfg!(debug_assertions) {
        return Err("debug builds only".into());
    }
    with_core(&state, |c| {
        let now = Utc::now();
        aroh_app::sample::seed(&c.conn, &mut c.clock, &c.user_id, now, crate::zone())?;
        for (rank, spec) in LONG.iter().enumerate() {
            seed_goal(c, spec, rank as i64, now)?;
        }
        aroh_app::tick::tick(&c.conn, &mut c.clock, &c.user_id, now, crate::zone()).map(drop)
    })?;
    after_write(&app);
    Ok(())
}
