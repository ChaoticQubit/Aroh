mod common;

use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{
    Action, ActionStatus, Autonomy, Cadence, CadenceFreq, DayKind, EntrySource, FieldKind, Goal, GoalStatus, Preference, RuleOp, Tracker,
    TrackerEntry, TrackerField, TrackerRule,
};
use aroh_server::db::{self, PgPool};
use aroh_server::engine::{Engine, SetKey, StartReplan};
use aroh_server::fake::{body_text, json_reply, sse_text, FnTransport};
use aroh_server::insights;
use aroh_server::rows::{put_row, utc, ServerClock};
use aroh_server::triggers::SetReview;
use chrono::{DateTime, Duration, Utc};
use common::{deps, offline, pg, user, T0};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use troupekit_core::clock::FixedClock;
use troupekit_core::secret::Secret;
use troupekit_host::TickOptions;
use uuid::Uuid;

const DEVICE: &str = "00000000000000d1";
const FACT: &str = "Plan workouts in the evening, not the morning.";

struct World {
    pool: PgPool,
    engine: Engine,
    clock: Arc<FixedClock>,
    hlc: ServerClock,
    seen: Arc<Mutex<Vec<String>>>,
    analyst: Arc<Mutex<Value>>,
    user: Uuid,
}

fn models(seen: Arc<Mutex<Vec<String>>>, analyst: Arc<Mutex<Value>>) -> Arc<FnTransport> {
    FnTransport::new(move |req| {
        let body = body_text(req);
        seen.lock().unwrap().push(body.clone());
        if body.contains("You are Adaptation analyst") {
            return sse_text(&analyst.lock().unwrap().to_string());
        }
        if body.contains("You are Preference analyst") {
            return sse_text(&json!({"fact": FACT}).to_string());
        }
        json_reply(400, json!({"error": "unexpected request"}))
    })
}

async fn world(analyst: Value) -> World {
    let pool = pg().await;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let analyst = Arc::new(Mutex::new(analyst));
    let (deps, clock) = deps(&pool, models(seen.clone(), analyst.clone()), offline());
    let engine = Engine::new(deps);
    let u = user(&pool).await;
    let key = SetKey {
        vendor: "anthropic".into(),
        key: Secret::new("sk-ant-test-TRIGGERS-4321".into()).unwrap(),
        model: Some("claude-sonnet-5-5".into()),
        label: Some("Main".into()),
        is_default: Some(true),
    };
    engine.set_key(u, Uuid::now_v7(), key).await.unwrap();
    World { pool, engine, clock, hlc: ServerClock::default(), seen, analyst, user: u }
}

impl World {
    async fn goal(&self, autonomy: Autonomy) -> Uuid {
        let id = Uuid::now_v7();
        let mut g = Goal {
            id: id.to_string(),
            user_id: self.user.to_string(),
            category_id: None,
            title: "Six pack by Feb 1".into(),
            context: None,
            deadline: None,
            status: GoalStatus::Active,
            autonomy,
            created_at: utc(T0),
            updated_at: String::new(),
            deleted: false,
        };
        put_row(&self.pool, &self.hlc, self.user, &mut g).await.unwrap();
        self.engine
            .set_review(self.user, id, SetReview { weekday: 0, at: "18:00".into(), enabled: false, timezone: "Europe/Berlin".into() })
            .await
            .unwrap();
        id
    }

    async fn action(&self, goal: Uuid, start: &str) -> Action {
        let start: DateTime<Utc> = start.parse().unwrap();
        let mut a = Action {
            id: Uuid::now_v7().to_string(),
            user_id: self.user.to_string(),
            goal_id: goal.to_string(),
            series_id: None,
            occurrence: None,
            title: "Core workout".into(),
            notes: None,
            start_at: start,
            end_at: start + Duration::minutes(30),
            status: ActionStatus::Planned,
            locked: false,
            done_at: None,
            updated_at: String::new(),
            deleted: false,
        };
        put_row(&self.pool, &self.hlc, self.user, &mut a).await.unwrap();
        a
    }

    async fn owner_moves(&self, action: &Action, hours: i64) {
        let current: String = db::query_scalar("select updated_at from public.actions where id::text = $1")
            .bind(&action.id)
            .fetch_one(&self.pool)
            .await
            .unwrap();
        let stamp = Hlc::new(DEVICE).stamp_after(&current).unwrap();
        db::query(
            "update public.actions set start_at = start_at + make_interval(hours => $2::int), end_at = end_at + make_interval(hours => $2::int),
             locked = true, updated_at = $3 where id::text = $1",
        )
        .bind(&action.id)
        .bind(hours as i32)
        .bind(stamp)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    async fn tick(&self) {
        let report = self.engine.tick(TickOptions::default()).await.unwrap();
        assert_eq!(report.errors, Vec::<String>::new());
    }

    fn later(&self, ms: i64) {
        self.clock.advance(ms);
    }

    async fn count(&self, sql: &str) -> i64 {
        db::query_scalar(sql).bind(self.user).fetch_one(&self.pool).await.unwrap()
    }
}

fn move_reply(action_id: &str) -> Value {
    json!({"summary": "Move the missed workout to Monday evening.", "size": "small",
        "changes": [{"op": "move", "action_id": action_id, "title": "Core workout", "start": "2026-11-23T18:00:00", "minutes": 30}]})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn missed_action_produces_a_proposal_without_an_owner_message() {
    let w = world(json!(null)).await;
    let goal = w.goal(Autonomy::Balanced).await;
    let missed = w.action(goal, "2026-11-22T09:00:00Z").await;
    *w.analyst.lock().unwrap() = move_reply(&missed.id);
    let _old = w.action(goal, "2026-11-18T09:00:00Z").await;
    let engine = &w.engine;

    let report = engine.tick(TickOptions::default()).await.unwrap();
    assert_eq!(report.errors, Vec::<String>::new());
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1 and workflow = 'replan'").await, 1);
    let (status, summary): (String, String) = db::query_as("select status, summary from public.proposals where user_id = $1")
        .bind(w.user)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!((status.as_str(), summary.as_str()), ("applied", "Move the missed workout to Monday evening."));
    let moved: DateTime<Utc> =
        db::query_scalar("select start_at from public.actions where id::text = $1").bind(&missed.id).fetch_one(&w.pool).await.unwrap();
    assert_eq!(moved.to_rfc3339(), "2026-11-23T17:00:00+00:00");
    let (source, reason, run): (String, String, Option<Uuid>) =
        db::query_as("select source, reason, run_id from public.adaptations where user_id = $1").bind(w.user).fetch_one(&w.pool).await.unwrap();
    assert_eq!(source, "actions");
    assert_eq!(reason, "Missed: Core workout (Sun 10:00).");
    assert_eq!(w.count("select count(*) from public.action_events where user_id = $1 and not handled").await, 0);
    let claimed: Option<Uuid> = db::query_scalar("select handled_by from public.action_events where user_id = $1")
        .bind(w.user)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(claimed, run);

    let report = engine.tick(TickOptions::default()).await.unwrap();
    assert_eq!(report.processed, 0, "the scan slot of this process is taken for 30 s");
    let insights = insights::list(&w.pool, w.user, Some(goal)).await.unwrap();
    assert_eq!(insights.len(), 1);
    assert_eq!(insights[0].body, "Missed: Core workout (Sun 10:00). Move the missed workout to Monday evening. Applied 1 of 1 changes.");
    assert_eq!(insights[0].state.as_deref(), Some("done"));
    assert!(insights::list(&w.pool, user(&w.pool).await, None).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn away_days_start_one_adaptation_per_goal_that_always_asks() {
    let w = world(json!(null)).await;
    let goal = w.goal(Autonomy::Auto).await;
    let quiet = w.goal(Autonomy::Auto).await;
    let workout = w.action(goal, "2026-11-27T09:00:00Z").await;
    let _before = w.action(quiet, "2026-11-25T09:00:00Z").await;
    *w.analyst.lock().unwrap() = move_reply(&workout.id);
    let mut trip = aroh_domain::rows::BusyBlock {
        id: Uuid::now_v7().to_string(),
        user_id: w.user.to_string(),
        calendar_id: "home".into(),
        event_id: "trip".into(),
        title: "Vacation".into(),
        start_at: "2026-11-26T23:00:00Z".parse().unwrap(),
        end_at: "2026-11-28T23:00:00Z".parse().unwrap(),
        all_day: true,
        group_name: None,
        day_kind: Some(DayKind::Away),
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&w.pool, &w.hlc, w.user, &mut trip).await.unwrap();
    w.tick().await;
    let rows: Vec<(String, String, String, Option<Uuid>)> =
        db::query_as("select source, window_key, reason, goal_id from public.adaptations where user_id = $1").bind(w.user).fetch_all(&w.pool).await.unwrap();
    assert_eq!(rows.len(), 1, "only the goal with actions on the away days adapts: {rows:?}");
    let (source, window, reason, on) = &rows[0];
    assert_eq!((source.as_str(), window.as_str(), *on), ("calendar", "away:2026-11-27:2026-11-28", Some(goal)));
    assert!(reason.starts_with("Away (Vacation): Fri 27 Nov to Sat 28 Nov. Adapt"), "{reason}");
    let status: String = db::query_scalar("select status from public.proposals where user_id = $1").bind(w.user).fetch_one(&w.pool).await.unwrap();
    assert_eq!(status, "proposed", "an Away change waits in the Inbox even for an Auto goal");
    let kept: DateTime<Utc> = db::query_scalar("select start_at from public.actions where id::text = $1").bind(&workout.id).fetch_one(&w.pool).await.unwrap();
    assert_eq!(kept, workout.start_at);
    let analyst = w.seen.lock().unwrap().iter().find(|b| b.contains("You are Adaptation analyst")).cloned().unwrap();
    assert!(analyst.contains("2026-11-27") && analyst.contains("away_days"), "the planner sees the away days");
    let insight = insights::list(&w.pool, w.user, Some(goal)).await.unwrap().remove(0);
    assert_eq!((insight.source.as_str(), insight.title.as_str()), ("calendar", "Away days"));

    w.later(31_000);
    w.tick().await;
    assert_eq!(w.count("select count(*) from public.adaptations where user_id = $1").await, 1, "a second scan claims nothing");
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1 and workflow = 'replan'").await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn dedupe_starts_one_replan_per_goal_and_window() {
    let w = world(json!({"summary": "On track.", "size": "small", "changes": []})).await;
    let goal = w.goal(Autonomy::Ask).await;
    let first = w.action(goal, "2026-11-22T08:00:00Z").await;
    w.tick().await;
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1 and workflow = 'replan'").await, 1);
    let mut skipped = w.action(goal, "2026-11-22T10:00:00Z").await;
    let stamp = Hlc::new(DEVICE).stamp_after(&skipped.updated_at).unwrap();
    db::query("update public.actions set status = 'skipped', updated_at = $2 where id::text = $1")
        .bind(&skipped.id)
        .bind(&stamp)
        .execute(&w.pool)
        .await
        .unwrap();
    skipped.status = ActionStatus::Skipped;
    w.later(31_000);
    w.tick().await;
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1 and workflow = 'replan'").await, 1, "same goal, same day");
    assert_eq!(w.count("select count(*) from public.action_events where user_id = $1 and not handled").await, 1);
    w.later(24 * 3_600_000);
    w.tick().await;
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1 and workflow = 'replan'").await, 2, "next day's window");
    assert_eq!(w.count("select count(*) from public.proposals where user_id = $1").await, 0, "no change writes no proposal");
    let _ = first;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn scheduled_review_runs_from_tick() {
    let w = world(json!({"summary": "On track, keep the plan.", "size": "small", "changes": []})).await;
    let goal = w.goal(Autonomy::Ask).await;
    let review = w
        .engine
        .set_review(w.user, goal, SetReview { weekday: 0, at: "13:10".into(), enabled: true, timezone: "Europe/Berlin".into() })
        .await
        .unwrap();
    assert_eq!(review.next_at.to_rfc3339(), "2026-11-22T12:10:00+00:00");
    w.tick().await;
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1").await, 0, "not due yet");
    w.later(11 * 60_000);
    w.tick().await;
    let state: String = db::query_scalar("select state from public.runs where user_id = $1 and workflow = 'replan'")
        .bind(w.user)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(state, "done", "no change ends the run even under Ask");
    assert_eq!(w.engine.review(w.user, goal).await.unwrap().next_at.to_rfc3339(), "2026-11-29T12:10:00+00:00");
    assert_eq!(w.count("select count(*) from public.proposals where user_id = $1").await, 0);
    let insight = insights::list(&w.pool, w.user, Some(goal)).await.unwrap().remove(0);
    assert_eq!(insight.source, "review");
    assert!(insight.body.starts_with("Weekly review of the last 7 days: 0 done, 0 missed, 0 skipped"), "{}", insight.body);
    assert!(insight.body.ends_with("No change needed. On track, keep the plan."), "{}", insight.body);
    let analyst = w.seen.lock().unwrap().iter().find(|b| b.contains("You are Adaptation analyst")).cloned().unwrap();
    assert!(analyst.contains("Weekly review"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn tracker_rule_breach_starts_a_replan() {
    let w = world(json!({"summary": "Add a cardio block.", "size": "big", "changes": []})).await;
    let goal = w.goal(Autonomy::Balanced).await;
    let mut tracker = Tracker {
        id: Uuid::now_v7().to_string(),
        user_id: w.user.to_string(),
        goal_id: goal.to_string(),
        plugin_id: None,
        name: "Body".into(),
        fields: vec![TrackerField { key: "weight".into(), label: "Weight".into(), kind: FieldKind::Number, unit: Some("kg".into()), choices: vec![], health: None }],
        cadence: Some(Cadence { freq: CadenceFreq::Daily, weekdays: vec![], at: "08:00".into() }),
        rules: vec![TrackerRule { field: "weight".into(), op: RuleOp::Flat, value: 0.3, window_days: 14 }],
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&w.pool, &w.hlc, w.user, &mut tracker).await.unwrap();
    for d in 0..14 {
        let mut e = TrackerEntry {
            id: Uuid::now_v7().to_string(),
            user_id: w.user.to_string(),
            tracker_id: tracker.id.clone(),
            data: json!({"weight": 82.0 + (d % 2) as f64 * 0.1}).as_object().unwrap().clone(),
            source: EntrySource::CheckIn,
            raw_text: None,
            logged_at: utc(T0) - Duration::days(13 - d) - Duration::hours(1),
            updated_at: String::new(),
            deleted: false,
        };
        put_row(&w.pool, &w.hlc, w.user, &mut e).await.unwrap();
    }
    w.tick().await;
    let (source, reason): (String, String) =
        db::query_as("select source, reason from public.adaptations where user_id = $1").bind(w.user).fetch_one(&w.pool).await.unwrap();
    assert_eq!(source, "rule");
    assert_eq!(reason, "Body: Weight stayed between 82.0 and 82.1 kg for 14 days.");
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1 and workflow = 'replan' and state = 'done'").await, 1);
    w.later(31_000);
    w.tick().await;
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1").await, 1, "a rule fires once per window");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn three_similar_edits_produce_a_preference_proposal() {
    let w = world(json!({"summary": "On track.", "size": "small", "changes": []})).await;
    let goal = w.goal(Autonomy::Balanced).await;
    let mut actions = Vec::new();
    for day in 23..=26 {
        actions.push(w.action(goal, &format!("2026-11-{day}T06:00:00Z")).await);
    }
    assert_eq!(w.count("select count(*) from public.action_events where user_id = $1").await, 0, "server writes are not edits");
    w.owner_moves(&actions[0], 11).await;
    w.owner_moves(&actions[1], 12).await;
    w.tick().await;
    assert_eq!(w.count("select count(*) from public.action_events where user_id = $1 and kind = 'move'").await, 2);
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1").await, 0, "two edits only lock");
    let locked: bool = db::query_scalar("select locked from public.actions where id::text = $1").bind(&actions[0].id).fetch_one(&w.pool).await.unwrap();
    assert!(locked);

    w.owner_moves(&actions[2], 11).await;
    w.later(31_000);
    w.tick().await;
    let prefs: Vec<(String, String, bool)> = db::query_as("select fact, source, confirmed from public.preferences where user_id = $1")
        .bind(w.user)
        .fetch_all(&w.pool)
        .await
        .unwrap();
    assert_eq!(prefs, vec![(FACT.to_string(), "Preference analyst".to_string(), false)]);
    assert_eq!(w.count("select count(*) from public.action_events where user_id = $1 and not handled").await, 0);
    let insight = insights::list(&w.pool, w.user, None).await.unwrap().remove(0);
    assert_eq!(insight.source, "edits");
    assert_eq!(insight.body, format!("3 similar edits: moved actions from the morning to the evening. Proposed preference: {FACT}"));

    w.owner_moves(&actions[3], 11).await;
    w.later(31_000);
    w.tick().await;
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1 and workflow = 'learn-preference'").await, 1);

    let mut pref: Preference = serde_json::from_value(
        db::query_scalar::<_, Value>("select to_jsonb(p) - 'server_seq' from public.preferences p where user_id = $1")
            .bind(w.user)
            .fetch_one(&w.pool)
            .await
            .unwrap(),
    )
    .unwrap();
    pref.confirmed = true;
    put_row(&w.pool, &w.hlc, w.user, &mut pref).await.unwrap();
    w.engine
        .start_replan(w.user, goal, StartReplan { reason: "Busy week.".into(), timezone: "Europe/Berlin".into() })
        .await
        .unwrap();
    w.tick().await;
    let analyst = w.seen.lock().unwrap().iter().rev().find(|b| b.contains("You are Adaptation analyst")).cloned().unwrap();
    assert!(analyst.contains(FACT), "confirmed preferences reach planning");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn timezone_moves_every_review_of_the_owner() {
    let w = world(json!(null)).await;
    let first = w.goal(Autonomy::Ask).await;
    let second = w.goal(Autonomy::Ask).await;
    w.engine
        .set_review(w.user, second, SetReview { weekday: 3, at: "07:30".into(), enabled: true, timezone: "Europe/Berlin".into() })
        .await
        .unwrap();
    let other = user(&w.pool).await;
    let theirs = Uuid::now_v7();
    let mut g = Goal {
        id: theirs.to_string(),
        user_id: other.to_string(),
        category_id: None,
        title: "Run a marathon".into(),
        context: None,
        deadline: None,
        status: GoalStatus::Active,
        autonomy: Autonomy::Ask,
        created_at: utc(T0),
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&w.pool, &w.hlc, other, &mut g).await.unwrap();
    w.engine.set_review(other, theirs, SetReview { weekday: 0, at: "18:00".into(), enabled: true, timezone: "Europe/Berlin".into() }).await.unwrap();

    assert!(matches!(w.engine.set_timezone(w.user, "Mars/Olympus").await, Err(aroh_server::error::ServerError::BadRequest(_))));
    assert_eq!(w.engine.set_timezone(w.user, "America/New_York").await.unwrap(), 2);

    let a = w.engine.review(w.user, first).await.unwrap();
    assert_eq!((a.timezone.as_str(), a.weekday, a.at.as_str(), a.enabled), ("America/New_York", 0, "18:00", false));
    assert_eq!(a.next_at.to_rfc3339(), "2026-11-22T23:00:00+00:00");
    let b = w.engine.review(w.user, second).await.unwrap();
    assert_eq!((b.timezone.as_str(), b.weekday, b.at.as_str(), b.enabled), ("America/New_York", 3, "07:30", true));
    assert_eq!(b.next_at.to_rfc3339(), "2026-11-25T12:30:00+00:00");
    assert_eq!(w.engine.review(other, theirs).await.unwrap().timezone, "Europe/Berlin");
    assert_eq!(w.engine.set_timezone(w.user, "America/New_York").await.unwrap(), 2);
}
