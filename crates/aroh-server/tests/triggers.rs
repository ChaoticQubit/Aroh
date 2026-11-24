mod common;

use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{
    Action, ActionStatus, Autonomy, Cadence, CadenceFreq, DayKind, EntrySource, FieldKind, Goal, GoalStatus, Preference, RuleOp, Tracker,
    TrackerEntry, TrackerField, TrackerRule,
};
use aroh_server::db::{self, PgPool};
use aroh_server::engine::{Engine, SetKey, StartGoal, StartReplan};
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
        if body.contains("You are Card writer") {
            return sse_text(&common::card_writer_reply(&body));
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
            reminders: true,
            use_calendars: true,
            run_limit_minutes: None,
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
            card: None,
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
        db::query_as("select source, window_key, reason, goal_id from public.adaptations where user_id = $1 and window_key not like 'fill:%'").bind(w.user).fetch_all(&w.pool).await.unwrap();
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
    assert_eq!(w.count("select count(*) from public.adaptations where user_id = $1 and window_key not like 'fill:%'").await, 1, "a second scan claims nothing");
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
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1 and workflow <> 'fill-cards'").await, 0, "two edits only lock");
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
async fn planning_sees_named_places_with_links_and_never_coordinates() {
    use aroh_domain::rows::{ActionSeries, Place, PlaceLink, PlaceSource};
    let w = world(json!(null)).await;
    let goal = w.goal(Autonomy::Ask).await;
    let workout = w.action(goal, "2026-11-27T09:00:00Z").await;
    *w.analyst.lock().unwrap() = move_reply(&workout.id);
    let mut series = ActionSeries {
        id: Uuid::now_v7().to_string(),
        user_id: w.user.to_string(),
        goal_id: goal.to_string(),
        title: "Strength session".into(),
        notes: None,
        rrule: "FREQ=WEEKLY;BYDAY=MO".into(),
        dtstart_local: "2026-11-23T18:00:00".into(),
        timezone: "Europe/Berlin".into(),
        duration_minutes: 60,
        card: None,
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&w.pool, &w.hlc, w.user, &mut series).await.unwrap();
    let mut place = Place {
        id: Uuid::now_v7().to_string(),
        user_id: w.user.to_string(),
        name: "Iron Works Gym".into(),
        address: Some("1200 Market St".into()),
        lat_e6: 37_771_234,
        lon_e6: -122_415_678,
        radius_m: 150,
        source: PlaceSource::Prompt,
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&w.pool, &w.hlc, w.user, &mut place).await.unwrap();
    for series_id in [None, Some(series.id.clone())] {
        let mut link = PlaceLink { id: Uuid::now_v7().to_string(), user_id: w.user.to_string(), place_id: place.id.clone(), goal_id: goal.to_string(), series_id, updated_at: String::new(), deleted: false };
        put_row(&w.pool, &w.hlc, w.user, &mut link).await.unwrap();
    }
    let mut gone = Place { id: Uuid::now_v7().to_string(), name: "Old pool".into(), deleted: true, ..place.clone() };
    put_row(&w.pool, &w.hlc, w.user, &mut gone).await.unwrap();

    w.engine
        .start_replan(w.user, goal, StartReplan { reason: "Busy week.".into(), timezone: "Europe/Berlin".into() })
        .await
        .unwrap();
    w.tick().await;
    let analyst = w.seen.lock().unwrap().iter().rev().find(|b| b.contains("You are Adaptation analyst")).cloned().unwrap();
    assert!(analyst.contains("places") && analyst.contains("Iron Works Gym"), "named places reach planning");
    assert!(analyst.contains("Six pack by Feb 1") && analyst.contains("Strength session"), "with linked goals and routines");
    assert!(!analyst.contains("Old pool"), "deleted places stay out");
    for secret in ["37771234", "122415678", "37.771", "122.415", "1200 Market"] {
        assert!(!analyst.contains(secret), "no coordinates or address reach the model: {secret}");
    }
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
        reminders: true,
        use_calendars: true,
        run_limit_minutes: None,
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_deleted_goal_stops_its_runs_and_jobs_and_never_comes_back() {
    let w = world(move_reply("none")).await;
    let goal = w.goal(Autonomy::Auto).await;
    let _missed = w.action(goal, "2026-11-22T09:00:00Z").await;
    let started = w
        .engine
        .start_replan(w.user, goal, StartReplan { reason: "Busy week.".into(), timezone: "Europe/Berlin".into() })
        .await
        .unwrap();
    let current: String = db::query_scalar("select updated_at from public.goals where id = $1").bind(goal).fetch_one(&w.pool).await.unwrap();
    let stamp = Hlc::new(DEVICE).stamp_after(&current).unwrap();
    db::query("update public.goals set deleted = true, updated_at = $2 where id = $1").bind(goal).bind(&stamp).execute(&w.pool).await.unwrap();

    w.tick().await;
    let state: String = db::query_scalar("select state from public.runs where id = $1").bind(started.run_id).fetch_one(&w.pool).await.unwrap();
    assert_eq!(state, "failed", "the run of a deleted goal is cancelled");
    assert_eq!(w.count("select count(*) from public.jobs where user_id = $1 and status in ('queued', 'running')").await, 0);
    assert!(w.seen.lock().unwrap().is_empty(), "no model call for a deleted goal");
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1").await, 1, "the scan starts nothing for it");
    assert_eq!(w.count("select count(*) from public.proposals where user_id = $1").await, 0);
    assert_eq!(w.count("select count(*) from public.review_schedules where user_id = $1 and enabled").await, 0);
    assert_eq!(w.count("select count(*) from public.action_events where user_id = $1 and not handled").await, 0);

    assert!(w.engine.retry(w.user, started.run_id).await.is_err(), "a run of a deleted goal is not retried");
    let again = StartGoal {
        goal_id: Some(goal),
        title: "Six pack by Feb 1".into(),
        context: None,
        deadline: None,
        category_id: None,
        autonomy: None,
        timezone: "Europe/Berlin".into(),
        reminders: None,
        use_calendars: None,
        run_limit_minutes: None,
    };
    assert!(w.engine.start_goal(w.user, again).await.is_err(), "a deleted goal id is not planned again");
    let deleted: bool = db::query_scalar("select deleted from public.goals where id = $1").bind(goal).fetch_one(&w.pool).await.unwrap();
    assert!(deleted, "the server never resurrects a deleted goal");
    w.later(31_000);
    w.tick().await;
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1").await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn one_owner_move_into_an_overlap_replans_the_other_goal() {
    let w = world(json!({"summary": "Nothing to change.", "size": "small", "changes": []})).await;
    let read = w.goal(Autonomy::Balanced).await;
    let run = w.goal(Autonomy::Balanced).await;
    let flexible = w.action(read, "2026-11-24T10:00:00Z").await;
    let moved = w.action(run, "2026-11-24T08:00:00Z").await;
    let alone = w.action(run, "2026-11-25T08:00:00Z").await;
    w.owner_moves(&alone, 1).await;
    w.tick().await;
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1 and workflow = 'replan'").await, 0, "no overlap, no run");
    assert_eq!(w.count("select count(*) from public.action_events where user_id = $1 and replan_checked_at is not null").await, 1);

    w.owner_moves(&moved, 2).await;
    w.later(31_000);
    w.tick().await;
    let (goal, reason, run_id): (Uuid, String, Uuid) =
        db::query_as("select goal_id, reason, run_id from public.adaptations where user_id = $1 and source = 'edits'").bind(w.user).fetch_one(&w.pool).await.unwrap();
    assert_eq!(goal, read, "the goal of the flexible action replans");
    assert!(reason.contains("It now overlaps \"Core workout\""), "{reason}");
    let state = w.engine.edit_replan(w.user, moved.id.parse().unwrap()).await.unwrap();
    assert!(state.checked);
    assert_eq!(state.run_id, Some(run_id));
    assert_eq!(w.count("select count(*) from public.action_events where user_id = $1 and kind = 'move' and not handled").await, 2, "learning still reads its events");
    let untouched = w.engine.edit_replan(w.user, flexible.id.parse().unwrap()).await.unwrap();
    assert_eq!((untouched.event_at, untouched.checked), (None, false));
}

async fn depth(pool: &PgPool, id: &str) -> Option<String> {
    db::query_scalar("select card->>'depth' from public.actions where id::text = $1").bind(id).fetch_one(pool).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn cards_backfill_once_then_fill_with_each_weekly_review() {
    let w = world(json!({"summary": "On track, keep the plan.", "size": "small", "changes": []})).await;
    let goal = w.goal(Autonomy::Ask).await;
    let first = w.action(goal, "2026-11-23T07:00:00Z").await;
    w.tick().await;
    assert_eq!(depth(&w.pool, &first.id).await.as_deref(), Some("full"), "the one-time backfill wrote the card");
    assert_eq!(w.count("select count(*) from public.adaptations where user_id = $1 and window_key = 'fill:backfill'").await, 1);
    let second = w.action(goal, "2026-11-25T07:00:00Z").await;
    w.later(31_000);
    w.tick().await;
    assert_eq!(depth(&w.pool, &second.id).await, None, "the backfill runs once per goal");
    w.engine
        .set_review(w.user, goal, SetReview { weekday: 0, at: "13:10".into(), enabled: true, timezone: "Europe/Berlin".into() })
        .await
        .unwrap();
    w.later(11 * 60_000);
    w.tick().await;
    assert_eq!(depth(&w.pool, &second.id).await.as_deref(), Some("full"), "the weekly review fills the next two weeks");
    assert_eq!(w.count("select count(*) from public.adaptations where user_id = $1 and window_key = 'fill:2026-11-22'").await, 1);
    assert_eq!(w.count("select count(*) from public.runs where user_id = $1 and workflow = 'fill-cards' and state = 'done'").await, 2);
    let writer = w.seen.lock().unwrap().iter().filter(|b| b.contains("You are Card writer")).count();
    assert_eq!(writer, 2);
    let shown = insights::list(&w.pool, w.user, Some(goal)).await.unwrap();
    assert!(shown.iter().all(|i| !i.body.contains("cards")), "card fills never reach the Inbox");
    assert_eq!(shown.len(), 1, "the weekly review only");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn backfill_gives_a_goal_without_a_review_its_weekly_review() {
    let w = world(json!({"summary": "s", "size": "small", "changes": []})).await;
    let goal = w.goal(Autonomy::Ask).await;
    db::query("delete from public.review_schedules where goal_id = $1").bind(goal).execute(&w.pool).await.unwrap();
    w.action(goal, "2026-11-23T07:00:00Z").await;
    w.tick().await;
    assert_eq!(w.count("select count(*) from public.review_schedules where user_id = $1 and enabled and weekday = 0 and at = '18:00'").await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_replan_that_writes_new_actions_fills_their_cards_once_right_away() {
    let w = world(json!({"summary": "On track.", "size": "small", "changes": []})).await;
    let goal = w.goal(Autonomy::Balanced).await;
    w.action(goal, "2026-11-23T07:00:00Z").await;
    w.tick().await;
    assert_eq!(w.count("select count(*) from public.adaptations where user_id = $1 and window_key = 'fill:backfill'").await, 1);
    *w.analyst.lock().unwrap() = json!({"summary": "Add an evening session.", "size": "small",
        "changes": [{"op": "create", "action_id": null, "title": "Core workout", "start": "2026-11-24T18:00:00", "minutes": 30}]});
    let run = w.engine.start_replan(w.user, goal, StartReplan { reason: "Run again.".into(), timezone: "Europe/Berlin".into() }).await.unwrap().run_id;
    w.tick().await;
    let created: String = db::query_scalar("select id::text from public.actions where user_id = $1 and start_at = '2026-11-24T17:00:00Z' and not deleted")
        .bind(w.user)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(depth(&w.pool, &created).await.as_deref(), Some("full"), "the new plan has full cards without waiting for the weekly fill");
    assert_eq!(w.count(&format!("select count(*) from public.adaptations where user_id = $1 and window_key = 'fill:plan:{run}'")).await, 1);
    w.later(31_000);
    w.tick().await;
    assert_eq!(w.count("select count(*) from public.adaptations where user_id = $1 and window_key like 'fill:plan:%'").await, 1, "one fill per plan version");
}
