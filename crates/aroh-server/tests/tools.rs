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
        ordered: false,
        run_limit_minutes: None,
        updated_at: String::new(),
        deleted: false,
    };
    put_row(pool, &hlc, user, &mut g).await.unwrap();
    ToolEnv { pool: pool.clone(), hlc, clock: Arc::new(FixedClock::new(T0)), user, goal: Some(goal), sealed: false, diary_key: None }
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
        card: None,
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
async fn b74_run_again_ends_future_missed_actions_and_keeps_past_and_finished_ones() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let goal = env.goal.unwrap();
    action(&env, "Missed past", -24, ActionStatus::Missed, false).await;
    action(&env, "Missed future", 48, ActionStatus::Missed, false).await;
    action(&env, "Missed future fixed", 50, ActionStatus::Missed, true).await;
    action(&env, "Skipped future", 52, ActionStatus::Skipped, false).await;
    action(&env, "Done future", 54, ActionStatus::Done, false).await;
    let new = json!({"plan": {"summary": "new", "timezone": "UTC", "trackers": [],
        "items": [{"title": "New run", "start": "2026-11-25T08:00:00", "minutes": 30, "rrule": null, "notes": null}]}});
    tool(&env, "replace_plan").call(new, &ToolContext::for_call(RunId::new(), 7, 0)).await.unwrap();
    assert_eq!(
        live(&pool, "actions", goal).await,
        ["Done future", "Missed future fixed", "Missed past", "New run", "Skipped future"],
        "a future missed action does not survive next to the new plan"
    );
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
        card: None,
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

fn card_tool(env: &ToolEnv, name: &str) -> Arc<dyn Tool> {
    aroh_server::cards::card_tools(env.clone()).into_iter().find(|t| t.spec().name == name).unwrap()
}

fn leg_card(exercise: &str) -> Value {
    json!({"why": "Legs.", "blocks": [{"kind": "workout", "title": null, "items": null,
        "exercises": [{"name": exercise, "unit": "kg", "rest_s": 120, "cue": null, "sets": [{"reps": 8, "weight": 60}]}]}]})
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn cards_fill_the_slots_once_and_chat_edits_apply_with_undo() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let goal = env.goal.unwrap();
    let ctx = ToolContext::for_call(RunId::new(), 1, 0);
    let plan = json!({"plan": {"summary": "s", "timezone": "Europe/Berlin", "trackers": [],
        "items": [{"title": "Legs", "start": "2026-11-23T07:00:00", "minutes": 60, "rrule": "FREQ=DAILY;COUNT=3", "notes": "Squat focus"}]}});
    tool(&env, "write_plan").call(plan, &ctx).await.unwrap();
    assert_eq!(count(&pool, "review_schedules", goal).await, 1, "approving a plan creates the weekly review");
    let work: Value = serde_json::from_str(&card_tool(&env, "card_slots").call(json!({"goal": {"timezone": "Europe/Berlin"}}), &ctx).await.unwrap().content).unwrap();
    let slots = work["slots"].as_array().unwrap().clone();
    assert_eq!(slots.len(), 3);
    assert_eq!(slots[0]["start"], "2026-11-23T07:00:00");
    let series = slots[0]["series_id"].as_str().unwrap().to_string();
    let foreign = json!({"card_work": work, "cards": {"cards": [{"action_id": Uuid::now_v7().to_string(), "card": leg_card("Squat")}], "outlines": []}});
    assert!(card_tool(&env, "write_cards").call(foreign, &ctx).await.is_err());
    let cards: Vec<Value> = slots.iter().map(|s| json!({"action_id": s["action_id"], "card": leg_card("Back squat")})).collect();
    let args = json!({"card_work": work, "cards": {"cards": cards, "outlines": [{"series_id": series, "outline": "Weeks 3 to 4: squat 5 x 5"}]}});
    let written: Value = serde_json::from_str(&card_tool(&env, "write_cards").call(args.clone(), &ctx).await.unwrap().content).unwrap();
    assert_eq!(written, json!({"written": 3, "kept": 0, "missing": 0}));
    let again: Value = serde_json::from_str(&card_tool(&env, "write_cards").call(args, &ctx).await.unwrap().content).unwrap();
    assert_eq!(again["kept"], 3, "a replayed write never overwrites a full card");
    let outline: String = db::query_scalar("select card->>'outline' from public.action_series where id::text = $1").bind(&series).fetch_one(&pool).await.unwrap();
    assert_eq!(outline, "Weeks 3 to 4: squat 5 x 5");
    let empty: Value = serde_json::from_str(&card_tool(&env, "card_slots").call(json!({"goal": {"timezone": "UTC"}}), &ctx).await.unwrap().content).unwrap();
    assert_eq!(empty["slots"], json!([]));

    let first = slots[0]["action_id"].as_str().unwrap();
    let edit = json!({"request": {"action_id": first, "scope": "this", "text": "split squats, 45 min"},
        "change": {"reply": "Swapped to split squats.", "minutes": 45, "card": leg_card("Split squat")}});
    let done: Value = serde_json::from_str(&card_tool(&env, "edit_card").call(edit.clone(), &ctx).await.unwrap().content).unwrap();
    assert_eq!(done["changed"], 1);
    let replay: Value = serde_json::from_str(&card_tool(&env, "edit_card").call(edit, &ctx).await.unwrap().content).unwrap();
    assert_eq!(replay, done, "a replayed edit returns its record");
    let (name, minutes): (String, f64) = db::query_as(
        "select card->'blocks'->0->'exercises'->0->>'name', (extract(epoch from end_at - start_at) / 60)::float8 from public.actions where id::text = $1",
    )
    .bind(first)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((name.as_str(), minutes), ("Split squat", 45.0));
    let (status, undo): (String, Value) = db::query_as("select status, undo from public.proposals where goal_id = $1").bind(goal).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "applied");
    assert_eq!(undo["rows"][0]["card"]["blocks"][0]["exercises"][0]["name"], "Back squat", "undo holds the card before the edit");
    let bad = json!({"request": {"action_id": first, "scope": "forever", "text": "x"}, "change": {"reply": "x", "minutes": 45, "card": leg_card("Squat")}});
    assert!(card_tool(&env, "edit_card").call(bad, &ctx).await.is_err());
}

/// The replan `calendar` state of `env`'s goal as the engine builds it: every
/// live action with id, title, local UTC start, minutes, status and locked flag.
/// Input: the tool env. Output: the JSON list.
async fn calendar(env: &ToolEnv) -> Value {
    let rows: Vec<(String, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, String, bool)> = db::query_as(
        "select id::text, title, start_at, end_at, status, locked from public.actions where goal_id = $1 and not deleted order by start_at",
    )
    .bind(env.goal.unwrap())
    .fetch_all(&env.pool)
    .await
    .unwrap();
    Value::Array(
        rows.into_iter()
            .map(|(id, title, start, end, status, locked)| {
                json!({"id": id, "title": title, "start": start.format("%Y-%m-%dT%H:%M:%S").to_string(), "minutes": (end - start).num_minutes(), "status": status, "locked": locked})
            })
            .collect(),
    )
}

async fn plan_sessions(env: &ToolEnv, ordered: bool, items: &[(&str, &str)]) -> Vec<String> {
    let items: Vec<Value> = items.iter().map(|(title, start)| json!({"title": title, "start": start, "minutes": 30, "rrule": null, "notes": "Read"})).collect();
    let plan = json!({"plan": {"summary": "s", "timezone": "UTC", "trackers": [], "items": items, "ordered": ordered}});
    let day_before = ToolEnv { clock: Arc::new(FixedClock::new(T0 - 86_400_000)), ..env.clone() };
    tool(&day_before, "write_plan").call(plan, &ToolContext::for_call(RunId::new(), 1, 0)).await.unwrap();
    let stored: bool = db::query_scalar("select ordered from public.goals where id = $1").bind(env.goal.unwrap()).fetch_one(&env.pool).await.unwrap();
    assert_eq!(stored, ordered, "the planner's order switch lands on the goal");
    db::query_scalar("select id::text from public.actions where goal_id = $1 order by start_at").bind(env.goal.unwrap()).fetch_all(&env.pool).await.unwrap()
}

async fn mark_missed(env: &ToolEnv, ids: &[String]) {
    for id in ids {
        let mut a = get_row::<Action>(&env.pool, env.user, id).await.unwrap().unwrap();
        a.status = ActionStatus::Missed;
        put_row(&env.pool, &env.hlc, env.user, &mut a).await.unwrap();
    }
}

fn moved(id: &str, title: &str, start: &str) -> Value {
    json!({"op": "move", "action_id": id, "title": title, "start": start, "minutes": 30})
}

async fn live_sessions(env: &ToolEnv) -> Vec<(String, String)> {
    db::query_as("select title, status from public.actions where goal_id = $1 and not deleted order by start_at")
        .bind(env.goal.unwrap())
        .fetch_all(&env.pool)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn b63_ordered_replan_keeps_chapter_order_and_plans_missed_sessions() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let ids = plan_sessions(
        &env,
        true,
        &[
            ("Chapters 3-6", "2026-11-22T08:00:00"),
            ("Chapters 7-9", "2026-11-22T09:00:00"),
            ("Chapters 10-12", "2026-11-22T10:00:00"),
            ("Chapters 12-14", "2026-11-23T20:00:00"),
            ("Chapters 15-17", "2026-11-24T20:00:00"),
        ],
    )
    .await;
    mark_missed(&env, &ids[..3]).await;
    let ctx = ToolContext::for_call(RunId::new(), 2, 0);
    let record = |changes: Value| json!({"goal": {"timezone": "UTC"}, "proposal": {"summary": "Catch up", "size": "big", "changes": changes}, "ordered": true, "daily_minutes": 30});

    let owner_b63 = json!([
        {"op": "create", "action_id": null, "title": "Catch up: Chapters 3-6", "start": "2026-11-23T07:00:00", "minutes": 30},
        moved(&ids[1], "Chapters 7-9", "2026-11-26T20:00:00"),
        moved(&ids[2], "Chapters 10-12", "2026-11-28T20:00:00")
    ]);
    let mut args = record(owner_b63);
    args["calendar"] = calendar(&env).await;
    let err = tool(&env, "record_proposal").call(args, &ctx).await.unwrap_err();
    assert!(matches!(&err, ToolError::InvalidArgs(m) if m.contains("repeats the missed session")), "{err}");
    let out_of_order = json!([
        moved(&ids[0], "Chapters 3-6", "2026-11-22T20:00:00"),
        moved(&ids[1], "Chapters 7-9", "2026-11-26T20:00:00"),
        moved(&ids[2], "Chapters 10-12", "2026-11-28T20:00:00")
    ]);
    let mut args = record(out_of_order);
    args["calendar"] = calendar(&env).await;
    let err = tool(&env, "record_proposal").call(args, &ctx).await.unwrap_err();
    assert!(matches!(&err, ToolError::InvalidArgs(m) if m.contains("Keep the sequence")), "{err}");
    assert_eq!(count(&pool, "proposals", env.goal.unwrap()).await, 0, "a rejected plan records nothing");

    let shifted = json!([
        moved(&ids[0], "Chapters 3-6", "2026-11-22T20:00:00"),
        moved(&ids[1], "Chapters 7-9", "2026-11-23T20:00:00"),
        moved(&ids[2], "Chapters 10-12", "2026-11-24T20:00:00"),
        moved(&ids[3], "Chapters 12-14", "2026-11-25T20:00:00"),
        moved(&ids[4], "Chapters 15-17", "2026-11-26T20:00:00")
    ]);
    let mut args = record(shifted);
    args["calendar"] = calendar(&env).await;
    let recorded: Value = serde_json::from_str(&tool(&env, "record_proposal").call(args, &ctx).await.unwrap().content).unwrap();
    let outcome: Value = serde_json::from_str(&tool(&env, "apply_proposal").call(json!({"proposal_record": recorded}), &ctx).await.unwrap().content).unwrap();
    assert_eq!(outcome, json!({"status": "applied", "applied": 5, "skipped": 0}));
    let planned = |t: &str| (t.to_string(), "planned".to_string());
    assert_eq!(
        live_sessions(&env).await,
        vec![planned("Chapters 3-6"), planned("Chapters 7-9"), planned("Chapters 10-12"), planned("Chapters 12-14"), planned("Chapters 15-17")],
        "order kept, moved sessions planned, no duplicate"
    );
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn unordered_replan_may_move_a_missed_workout_after_later_ones() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let ids = plan_sessions(&env, false, &[("Leg day", "2026-11-22T07:00:00"), ("Upper body", "2026-11-23T07:00:00"), ("Core", "2026-11-24T07:00:00")]).await;
    mark_missed(&env, &ids[..1]).await;
    let ctx = ToolContext::for_call(RunId::new(), 2, 0);
    let args = json!({"goal": {"timezone": "UTC"}, "calendar": calendar(&env).await, "ordered": false, "daily_minutes": 45,
        "proposal": {"summary": "Leg day after Core", "size": "small", "changes": [moved(&ids[0], "Leg day", "2026-11-25T07:00:00")]}});
    let recorded: Value = serde_json::from_str(&tool(&env, "record_proposal").call(args, &ctx).await.unwrap().content).unwrap();
    tool(&env, "apply_proposal").call(json!({"proposal_record": recorded}), &ctx).await.unwrap();
    let planned = |t: &str| (t.to_string(), "planned".to_string());
    assert_eq!(live_sessions(&env).await, vec![planned("Upper body"), planned("Core"), planned("Leg day")]);
}

fn one_off(title: &str, start: &str) -> Value {
    json!({"title": title, "start": start, "minutes": 30, "rrule": null, "notes": null})
}

fn new_york(items: Vec<Value>) -> Value {
    json!({"plan": {"summary": "s", "timezone": "America/New_York", "trackers": [], "items": items}})
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn b80_write_plan_refuses_a_one_off_before_now_and_writes_nothing() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let goal = env.goal.unwrap();
    let plan = new_york(vec![one_off("Shoes", "2026-11-24T18:00:00"), one_off("Core workout", "2026-11-22T06:59:00")]);
    let err = tool(&env, "write_plan").call(plan, &ToolContext::for_call(RunId::new(), 1, 0)).await.unwrap_err();
    let ToolError::InvalidArgs(message) = &err else { panic!("{err}") };
    assert_eq!(message, "`Core workout` starts at 2026-11-22T06:59, before now (2026-11-22T07:00 local); plan it at or after now");
    assert_eq!(count(&pool, "actions", goal).await, 0, "a refused plan writes no action, not even the valid one");
    let status: String = db::query_scalar("select status from public.goals where id = $1").bind(goal).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "draft", "a refused plan leaves the goal a draft");
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn b80_a_refused_replace_leaves_the_old_plan_in_place() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let goal = env.goal.unwrap();
    tool(&env, "write_plan").call(new_york(vec![one_off("Old run", "2026-11-23T08:00:00")]), &ToolContext::for_call(RunId::new(), 1, 0)).await.unwrap();
    let late = new_york(vec![one_off("New run", "2026-11-25T08:00:00"), one_off("Core workout", "2026-11-22T06:00:00")]);
    let err = tool(&env, "replace_plan").call(late, &ToolContext::for_call(RunId::new(), 2, 0)).await.unwrap_err();
    assert!(matches!(&err, ToolError::InvalidArgs(m) if m.contains("`Core workout`")), "{err}");
    assert_eq!(live(&pool, "actions", goal).await, ["Old run"], "the old plan is not cleared by a refused replace");
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn b80_write_plan_accepts_one_offs_at_and_after_now() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let goal = env.goal.unwrap();
    let plan = new_york(vec![one_off("At now", "2026-11-22T07:00:00"), one_off("A minute later", "2026-11-22T07:01")]);
    let out: Value = serde_json::from_str(&tool(&env, "write_plan").call(plan, &ToolContext::for_call(RunId::new(), 1, 0)).await.unwrap().content).unwrap();
    assert_eq!(out["actions"], 2, "{out}");
    assert_eq!(live(&pool, "actions", goal).await, ["A minute later", "At now"]);
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn b80_a_series_with_a_past_start_writes_no_occurrence_before_now() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = env(&pool, u).await;
    let goal = env.goal.unwrap();
    let plan = json!({"plan": {"summary": "s", "timezone": "UTC", "trackers": [],
        "items": [{"title": "Plank", "start": "2026-11-20T08:00:00", "minutes": 10, "rrule": "FREQ=DAILY;COUNT=5", "notes": null}]}});
    let out: Value = serde_json::from_str(&tool(&env, "write_plan").call(plan, &ToolContext::for_call(RunId::new(), 1, 0)).await.unwrap().content).unwrap();
    assert_eq!(out["series"], 1, "a series with a past start is not refused: {out}");
    let starts: Vec<chrono::DateTime<chrono::Utc>> =
        db::query_scalar("select start_at from public.actions where goal_id = $1 order by start_at").bind(goal).fetch_all(&pool).await.unwrap();
    assert_eq!(starts.iter().map(|s| s.to_rfc3339()).collect::<Vec<_>>(), ["2026-11-23T08:00:00+00:00", "2026-11-24T08:00:00+00:00"], "the same-day 08:00 before now is skipped");
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn a_diary_run_keeps_no_profile_facts() {
    let pool = pg().await;
    let u = user(&pool).await;
    let env = ToolEnv { sealed: true, ..env(&pool, u).await };
    let intake: aroh_server::profile::FactIntake =
        serde_json::from_value(json!({"questions": [{"id": "q1", "lasting": {"topic": "mood", "label": "Mood"}}]})).unwrap();
    let answers = vec![aroh_server::profile::FactAnswer { id: "q1".into(), answer: Some("DIARY-SENTINEL tired".into()) }];
    let kept = aroh_server::profile::save_facts(&env, aroh_server::profile::SaveFactsArgs { intake, answers }).await.unwrap();
    assert_eq!(kept["kept"], 0);
    let args: aroh_server::learning::ProposeArgs =
        serde_json::from_value(json!({"pattern": {"key": "k1", "text": "moved runs"}, "preference": {"fact": "DIARY-SENTINEL fact"}})).unwrap();
    aroh_server::learning::propose_preference(&env, args).await.unwrap();
    let facts: i64 = db::query_scalar("select count(*) from public.preferences where user_id = $1").bind(u).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, 0, "a run that read the diary writes no profile fact");
    let open = ToolEnv { sealed: false, ..env.clone() };
    let args: aroh_server::learning::ProposeArgs =
        serde_json::from_value(json!({"pattern": {"key": "k1", "text": "moved runs"}, "preference": {"fact": "Runs in the evening"}})).unwrap();
    aroh_server::learning::propose_preference(&open, args).await.unwrap();
    let facts: i64 = db::query_scalar("select count(*) from public.preferences where user_id = $1").bind(u).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, 1, "an ordinary run still proposes");
}
