mod common;

use aroh_domain::rows::{Action, ActionStatus, Autonomy, Goal, GoalStatus};
use aroh_server::db::{self, PgPool};
use aroh_server::rows::{get_row, put_row, utc, ServerClock};
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

async fn live(pool: &PgPool, table: &str, goal: Uuid) -> Vec<String> {
    db::query_scalar(&format!("select title from public.{table} where goal_id = $1 and not deleted order by title, start_at"))
        .bind(goal)
        .fetch_all(pool)
        .await
        .unwrap()
}

/// Marks each action of `ids` done through `put_row`, as the owner's device would.
/// A raw update keeps the HLC stamp, which the LWW trigger drops.
/// Input: the tool env and the action ids.
async fn mark_done(env: &ToolEnv, ids: Vec<String>) {
    for id in ids {
        let mut a = get_row::<Action>(&env.pool, env.user, &id).await.unwrap().unwrap();
        a.status = ActionStatus::Done;
        a.done_at = Some(a.start_at);
        put_row(&env.pool, &env.hlc, env.user, &mut a).await.unwrap();
    }
}

async fn action(env: &ToolEnv, title: &str, hours_from_t0: i64, status: ActionStatus, locked: bool) -> Action {
    let start = utc(T0) + chrono::Duration::hours(hours_from_t0);
    let mut a = Action {
        id: Uuid::now_v7().to_string(),
        user_id: env.user.to_string(),
        goal_id: env.goal.unwrap().to_string(),
        series_id: None,
        occurrence: None,
        title: title.into(),
        notes: None,
        start_at: start,
        end_at: start + chrono::Duration::minutes(30),
        status,
        locked,
        done_at: (status == ActionStatus::Done).then_some(start),
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&env.pool, &env.hlc, env.user, &mut a).await.unwrap();
    a
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn a_replayed_write_plan_keeps_what_the_owner_changed_in_between() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let plan = json!({"plan": {"summary": "s", "timezone": "UTC",
        "trackers": [{"name": "Weight", "fields": [{"key": "kg", "label": "Weight", "kind": "number", "unit": "kg"}]}],
        "items": [{"title": "Shoes", "start": "2026-11-24T18:00:00", "minutes": 30, "rrule": null, "notes": null}]}});
    let ctx = ToolContext::for_call(RunId::new(), 9, 0);
    let write = tool(&env, "write_plan");
    write.call(plan.clone(), &ctx).await.unwrap();
    let goal = env.goal.unwrap();
    let ids: Vec<String> = db::query_scalar("select id::text from public.actions where goal_id = $1").bind(goal).fetch_all(&pool).await.unwrap();
    mark_done(&env, ids).await;
    write.call(plan, &ctx).await.unwrap();
    let statuses: Vec<String> = db::query_scalar("select status from public.actions where goal_id = $1").bind(goal).fetch_all(&pool).await.unwrap();
    assert_eq!(statuses, ["done"], "the replay neither duplicates nor resets the action");
    assert_eq!(count(&pool, "trackers", goal).await, 1);
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn replace_plan_swaps_future_planned_actions_and_keeps_the_rest() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let goal = env.goal.unwrap();
    let old = json!({"plan": {"summary": "old", "timezone": "UTC",
        "trackers": [{"name": "Weight", "fields": [{"key": "kg", "label": "Weight", "kind": "number", "unit": "kg"}]}],
        "items": [{"title": "Old daily", "start": "2026-11-22T13:00:00", "minutes": 20, "rrule": "FREQ=DAILY;COUNT=5", "notes": null}]}});
    tool(&env, "write_plan").call(old, &ToolContext::for_call(RunId::new(), 3, 0)).await.unwrap();
    action(&env, "Done before", -24, ActionStatus::Done, false).await;
    action(&env, "Missed before", -48, ActionStatus::Planned, false).await;
    action(&env, "Fixed later", 30, ActionStatus::Planned, true).await;
    action(&env, "Old later", 40, ActionStatus::Planned, false).await;
    let early: Vec<String> = db::query_scalar("select id::text from public.actions where goal_id = $1 and title = 'Old daily' and start_at < $2")
        .bind(goal)
        .bind(utc(T0) + chrono::Duration::hours(2))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(early.len(), 1, "the first occurrence is done early");
    mark_done(&env, early).await;

    let new = json!({"plan": {"summary": "new", "timezone": "UTC",
        "trackers": [{"name": "Weight", "fields": [{"key": "kg", "label": "Weight", "kind": "number", "unit": "kg"}]},
                     {"name": "Sleep", "fields": [{"key": "h", "label": "Hours", "kind": "number", "unit": "h"}]}],
        "items": [{"title": "New run", "start": "2026-11-25T08:00:00", "minutes": 30, "rrule": null, "notes": null},
                  {"title": "New weekly", "start": "2026-11-23T09:00:00", "minutes": 30, "rrule": "FREQ=WEEKLY;COUNT=2", "notes": null}]}});
    let ctx = ToolContext::for_call(RunId::new(), 7, 0);
    let replace = tool(&env, "replace_plan");
    replace.call(new.clone(), &ctx).await.unwrap();
    let want = ["Done before", "Fixed later", "Missed before", "New run", "New weekly", "New weekly", "Old daily"];
    assert_eq!(live(&pool, "actions", goal).await, want, "done, past and fixed actions stay; future planned ones are replaced");
    let series: Vec<String> = db::query_scalar("select title from public.action_series where goal_id = $1 and not deleted").bind(goal).fetch_all(&pool).await.unwrap();
    assert_eq!(series, ["New weekly"], "the old series ends");
    let trackers: Vec<String> = db::query_scalar("select name from public.trackers where goal_id = $1 and not deleted order by name").bind(goal).fetch_all(&pool).await.unwrap();
    assert_eq!(trackers, ["Sleep", "Weight"], "a tracker of the same name is kept, not added again");

    replace.call(new, &ctx).await.unwrap();
    assert_eq!(live(&pool, "actions", goal).await, want, "a replayed replace keeps its own rows");
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn moving_the_date_on_the_limits_card_sets_the_deadline() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let args = json!({"plan": {"summary": "s", "timezone": "UTC", "trackers": [],
        "items": [{"title": "Read", "start": "2026-11-24T20:00:00", "minutes": 20, "rrule": null, "notes": null}]},
        "limits": {"fits": true, "tries": 0, "problems": [], "questions": [], "card": null, "move_to": "2026-12-15"}});
    tool(&env, "write_plan").call(args, &ToolContext::for_call(RunId::new(), 4, 0)).await.unwrap();
    let deadline: Option<String> = db::query_scalar("select deadline::text from public.goals where id = $1").bind(env.goal.unwrap()).fetch_one(&pool).await.unwrap();
    assert_eq!(deadline.as_deref(), Some("2026-12-15"));
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
