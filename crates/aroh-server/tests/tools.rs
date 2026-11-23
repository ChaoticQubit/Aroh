mod common;

use aroh_domain::rows::{Autonomy, Goal, GoalStatus};
use aroh_server::db::{self, PgPool};
use aroh_server::rows::{put_row, utc, ServerClock};
use aroh_server::tools::{aroh_toolkit, ToolEnv};
use common::{pg, user, T0};
use serde_json::{json, Value};
use std::sync::Arc;
use troupekit_core::clock::FixedClock;
use troupekit_core::ids::RunId;
use troupekit_tools::{Tool, ToolContext, ToolError};
use uuid::Uuid;

async fn env(pool: &PgPool, user: Uuid) -> ToolEnv {
    let hlc = Arc::new(ServerClock::default());
    let goal = Uuid::now_v7();
    let mut g = Goal {
        id: goal.to_string(),
        user_id: user.to_string(),
        category_id: None,
        title: "Run a 5k".into(),
        context: None,
        deadline: None,
        status: GoalStatus::Draft,
        autonomy: Autonomy::Balanced,
        created_at: utc(T0),
        reminders: true,
        use_calendars: true,
        run_limit_minutes: None,
        updated_at: String::new(),
        deleted: false,
    };
    put_row(pool, &hlc, user, &mut g).await.unwrap();
    ToolEnv { pool: pool.clone(), hlc, clock: Arc::new(FixedClock::new(T0)), user, goal: Some(goal) }
}

fn tool(env: &ToolEnv, name: &str) -> Arc<dyn Tool> {
    aroh_toolkit(env.clone()).tools().iter().find(|t| t.spec().name == name).unwrap().clone()
}

async fn count(pool: &PgPool, table: &str, goal: Uuid) -> i64 {
    db::query_scalar(&format!("select count(*) from public.{table} where goal_id = $1")).bind(goal).fetch_one(pool).await.unwrap()
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn replayed_write_plan_writes_each_row_once() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let plan = json!({"plan": {"summary": "s", "timezone": "Europe/Berlin", "trackers": [],
        "items": [{"title": "Run", "start": "2026-11-23T07:00:00", "minutes": 30, "rrule": "FREQ=DAILY;COUNT=3", "notes": null},
                  {"title": "Shoes", "start": "2026-11-24T18:00:00", "minutes": 30, "rrule": null, "notes": null}]}});
    let ctx = ToolContext::for_call(RunId::new(), 4, 0);
    let write = tool(&env, "write_plan");
    let first: Value = serde_json::from_str(&write.call(plan.clone(), &ctx).await.unwrap().content).unwrap();
    assert_eq!(first, json!({"actions": 1, "series": 1, "occurrences": 3, "trackers": 0}));
    write.call(plan, &ctx).await.unwrap();
    let goal = env.goal.unwrap();
    assert_eq!(count(&pool, "actions", goal).await, 4);
    assert_eq!(count(&pool, "action_series", goal).await, 1);
    let status: String = db::query_scalar("select status from public.goals where id = $1").bind(goal).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "active");
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn tools_refuse_rows_outside_the_run() {
    let pool = pg().await;
    let (a, b) = (user(&pool).await, user(&pool).await);
    let (env_a, env_b) = (env(&pool, a).await, env(&pool, b).await);
    let ctx = ToolContext::for_call(RunId::new(), 1, 0);
    let plan = json!({"plan": {"summary": "s", "timezone": "UTC", "trackers": [],
        "items": [{"title": "Run", "start": "2026-11-23T07:00:00", "minutes": 30, "rrule": null, "notes": null}]}});
    tool(&env_b, "write_plan").call(plan, &ctx).await.unwrap();
    let foreign: String = db::query_scalar("select id::text from public.actions where user_id = $1").bind(b).fetch_one(&pool).await.unwrap();
    let record = json!({"goal": {"timezone": "UTC"}, "proposal": {"summary": "x", "size": "small",
        "changes": [{"op": "delete", "action_id": foreign, "title": "Run", "start": null, "minutes": null}]}});
    let err = tool(&env_a, "record_proposal").call(record, &ctx).await.unwrap_err();
    assert!(matches!(err, ToolError::InvalidArgs(_)), "{err}");
    let entries = json!({"log": {"text": "ran", "at": "2026-11-22T08:00:00Z", "timezone": "UTC"},
        "entries": {"entries": [{"tracker_id": Uuid::now_v7().to_string(), "values": []}]}});
    assert!(tool(&env_a, "write_entries").call(entries, &ctx).await.is_err());
    let bad = json!({"plan": {"summary": "s", "timezone": "UTC", "trackers": [],
        "items": [{"title": "Run", "start": "tomorrow", "minutes": 30, "rrule": null, "notes": null}]}});
    assert!(tool(&env_a, "write_plan").call(bad, &ctx).await.is_err());
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn write_plan_stores_tracker_cadence() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let plan = json!({"plan": {"summary": "s", "timezone": "Europe/Berlin", "items": [],
        "trackers": [{"name": "Weight", "fields": [{"key": "weight", "label": "Weight", "kind": "number", "unit": "kg"}],
                      "cadence": {"freq": "daily", "weekdays": [], "at": "07:30"}},
                     {"name": "Mood", "fields": [{"key": "ok", "label": "Felt good", "kind": "bool", "unit": null}]}]}});
    tool(&env, "write_plan").call(plan, &ToolContext::for_call(RunId::new(), 1, 0)).await.unwrap();
    let rows: Vec<(String, Option<Value>)> = db::query_as("select name, cadence from public.trackers where goal_id = $1 order by name")
        .bind(env.goal.unwrap())
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows[0], ("Mood".to_string(), None));
    assert_eq!(rows[1].1, Some(json!({"freq": "daily", "weekdays": [], "at": "07:30"})));
    let bad = json!({"plan": {"summary": "s", "timezone": "UTC", "items": [],
        "trackers": [{"name": "W", "fields": [{"key": "w", "label": "W", "kind": "number", "unit": null}], "cadence": {"freq": "daily", "weekdays": [], "at": "7am"}}]}});
    assert!(tool(&env, "write_plan").call(bad, &ToolContext::for_call(RunId::new(), 2, 0)).await.is_err());
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn record_proposal_refuses_changes_to_fixed_actions() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let goal = env.goal.unwrap();
    let start = utc(T0) + chrono::Duration::days(1);
    let mut fixed = aroh_domain::rows::Action {
        id: Uuid::now_v7().to_string(),
        user_id: u.to_string(),
        goal_id: goal.to_string(),
        series_id: None,
        occurrence: None,
        title: "Dentist".into(),
        notes: None,
        start_at: start,
        end_at: start + chrono::Duration::minutes(30),
        status: aroh_domain::rows::ActionStatus::Planned,
        locked: true,
        done_at: None,
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&pool, &env.hlc, u, &mut fixed).await.unwrap();
    let ctx = ToolContext::for_call(RunId::new(), 1, 0);
    for (op, start, minutes) in [("move", Some("2026-11-24T18:00:00"), Some(30)), ("resize", Some("2026-11-24T18:00:00"), Some(60)), ("delete", None, None)] {
        let record = json!({"goal": {"timezone": "UTC"}, "proposal": {"summary": "x", "size": "small",
            "changes": [{"op": op, "action_id": fixed.id, "title": "Dentist", "start": start, "minutes": minutes}]}});
        let err = tool(&env, "record_proposal").call(record, &ctx).await.unwrap_err();
        assert!(matches!(&err, ToolError::InvalidArgs(m) if m.contains("fixed")), "{err}");
    }
    assert_eq!(count(&pool, "proposals", goal).await, 0, "nothing on a fixed action reaches a proposal");
}
