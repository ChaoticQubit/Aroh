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
