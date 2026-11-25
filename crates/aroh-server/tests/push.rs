mod common;

use aroh_domain::rows::{Action, ActionStatus, Autonomy, Goal, GoalStatus, Proposal, ProposalSize, ProposalStatus};
use aroh_server::checkpoints::{EventBuffer, PgCheckpoints};
use aroh_server::db::{self, PgPool};
use aroh_server::fake::{json_reply, FnTransport};
use aroh_server::push::{self, ApnsConfig, ApnsEnv, Platform, Pusher, RegisterDevice, Signer};
use aroh_server::rows::{insert_run, put_row, utc, RunRow, ServerClock};
use base64::Engine as _;
use chrono::{DateTime, Duration, Utc};
use common::{pg, user, MASTER, T0};
use aroh_server::vault::{MasterKey, Vault};
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, ECDSA_P256_SHA256_FIXED_SIGNING};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use troupekit_agent::{PersonaLoader, SimpleIo};
use troupekit_core::clock::{Clock, FixedClock};
use troupekit_core::ids::RunId;
use troupekit_core::store::CheckpointStore;
use troupekit_graph::{GraphOutcome, GraphRun, SimpleGraphIo, Stop, Workflow};
use troupekit_tools::ToolRegistry;
use uuid::Uuid;

fn signer(clock: Arc<dyn Clock>) -> Signer {
    let der = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new()).unwrap();
    let pem = format!("-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n", base64::engine::general_purpose::STANDARD.encode(der.as_ref()));
    let vars = BTreeMap::from([
        ("AROH_APNS_KEY".to_string(), pem),
        ("AROH_APNS_KEY_ID".to_string(), "ABC123DEFG".to_string()),
        ("AROH_APNS_TEAM_ID".to_string(), "TEAM456789".to_string()),
        ("AROH_APNS_TOPIC".to_string(), "io.aroh.app".to_string()),
    ]);
    Signer::new(ApnsConfig::from_vars(&vars).unwrap().unwrap(), clock)
}

fn token(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}

fn device(token: String, synced_seq: i64, scheduled_until: DateTime<Utc>) -> RegisterDevice {
    RegisterDevice { platform: Platform::Ios, token, environment: ApnsEnv::Sandbox, zone: "Europe/Berlin".into(), synced_seq, scheduled_until }
}

fn now() -> DateTime<Utc> {
    utc(T0)
}

async fn goal(pool: &PgPool, hlc: &ServerClock, user: Uuid) -> Goal {
    let mut g = Goal {
        id: Uuid::now_v7().to_string(),
        user_id: user.to_string(),
        category_id: None,
        title: "Run a 10k".into(),
        context: None,
        deadline: None,
        status: GoalStatus::Active,
        autonomy: Autonomy::Balanced,
        created_at: now(),
        reminders: true,
        use_calendars: true,
        ordered: false,
        run_limit_minutes: None,
        updated_at: String::new(),
        deleted: false,
    };
    put_row(pool, hlc, user, &mut g).await.unwrap();
    g
}

async fn action(pool: &PgPool, hlc: &ServerClock, user: Uuid, goal: &Goal, start: DateTime<Utc>) -> Action {
    let mut a = Action {
        id: Uuid::now_v7().to_string(),
        user_id: user.to_string(),
        goal_id: goal.id.clone(),
        series_id: None,
        occurrence: None,
        title: "Tempo run".into(),
        notes: None,
        start_at: start,
        end_at: start + Duration::minutes(40),
        status: ActionStatus::Planned,
        locked: false,
        done_at: None,
        card: None,
        updated_at: String::new(),
        deleted: false,
    };
    put_row(pool, hlc, user, &mut a).await.unwrap();
    a
}

async fn seq(pool: &PgPool, id: &str) -> i64 {
    db::query_scalar("select server_seq from public.actions where id::text = $1").bind(id).fetch_one(pool).await.unwrap()
}

struct Fixture {
    pool: PgPool,
    hlc: ServerClock,
    clock: Arc<FixedClock>,
    apns: Arc<FnTransport>,
    status: Arc<Mutex<(u16, Value)>>,
    pusher: Pusher,
}

async fn fixture() -> Fixture {
    let pool = pg().await;
    let clock = Arc::new(FixedClock::new(T0));
    let status = Arc::new(Mutex::new((200u16, json!({}))));
    let answer = status.clone();
    let apns = FnTransport::new(move |_| {
        let (code, body) = answer.lock().unwrap().clone();
        json_reply(code, body)
    });
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let pusher = Pusher::new(pool.clone(), vault(&pool), apns.clone(), signer(dyn_clock.clone()), dyn_clock);
    Fixture { pool, hlc: ServerClock::default(), clock, apns, status, pusher }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn devices_register_move_between_accounts_and_delete() {
    let pool = pg().await;
    let (a, b) = (user(&pool).await, user(&pool).await);
    let (da, db_id) = (Uuid::now_v7(), Uuid::now_v7());
    let info = push::register(&pool, a, da, device(token(1), 5, now()), now()).await.unwrap();
    assert_eq!((info.device_id, info.synced_seq), (da, 5));
    push::register(&pool, a, da, device(token(1), 9, now()), now()).await.unwrap();
    let rows: Vec<(Uuid, i64)> = db::query_as("select user_id, synced_seq from public.devices").fetch_all(&pool).await.unwrap();
    assert_eq!(rows, vec![(a, 9)]);
    assert!(matches!(push::register(&pool, b, da, device(token(2), 0, now()), now()).await, Err(aroh_server::error::ServerError::Conflict(_))));
    push::register(&pool, b, db_id, device(token(1), 0, now()), now()).await.unwrap();
    let rows: Vec<(Uuid, Uuid)> = db::query_as("select id, user_id from public.devices").fetch_all(&pool).await.unwrap();
    assert_eq!(rows, vec![(db_id, b)]);
    assert!(push::register(&pool, b, db_id, device("zz".repeat(32), 0, now()), now()).await.is_err());
    assert!(!push::unregister(&pool, a, db_id).await.unwrap());
    assert!(push::unregister(&pool, b, db_id).await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn unseen_reminders_are_pushed_once_and_covered_ones_are_skipped() {
    let f = fixture().await;
    let u = user(&f.pool).await;
    let g = goal(&f.pool, &f.hlc, u).await;
    let known = action(&f.pool, &f.hlc, u, &g, now() + Duration::minutes(10)).await;
    let synced = seq(&f.pool, &known.id).await;
    push::register(&f.pool, u, Uuid::now_v7(), device(token(3), synced, now() + Duration::hours(24)), now()).await.unwrap();
    let unseen = action(&f.pool, &f.hlc, u, &g, now() + Duration::minutes(12)).await;
    assert!(seq(&f.pool, &unseen.id).await > synced);

    f.clock.advance(2 * 60 * 1000);
    let summary = f.pusher.dispatch_once().await.unwrap();
    assert_eq!((summary.sent, summary.failed, summary.removed), (1, 0, 0), "{summary:?}");
    let sent = f.apns.sent();
    assert_eq!(sent.len(), 1);
    let req = &sent[0];
    assert_eq!(req.url, format!("https://api.sandbox.push.apple.com/3/device/{}", token(3)));
    assert!(req.header("authorization").unwrap().starts_with("bearer ey"));
    assert_eq!(req.header("apns-topic"), Some("io.aroh.app"));
    let reminder = aroh_domain::remind::reminder_id(&unseen.id, aroh_domain::remind::ReminderKind::Soon);
    assert_eq!(req.header("apns-collapse-id"), Some(reminder.to_string().as_str()));
    let payload: Value = serde_json::from_slice(req.body.as_ref().unwrap()).unwrap();
    assert_eq!(payload["aps"]["alert"]["title"], "In 10 min: Tempo run");
    assert_eq!(payload["aroh"], json!({"kind": "reminder", "id": unseen.id}));

    let again = f.pusher.dispatch_once().await.unwrap();
    assert_eq!(again.sent, 0);
    assert_eq!(f.apns.sent().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn proposals_are_pushed_and_gone_tokens_remove_the_device() {
    let f = fixture().await;
    let u = user(&f.pool).await;
    let g = goal(&f.pool, &f.hlc, u).await;
    let device_id = Uuid::now_v7();
    push::register(&f.pool, u, device_id, device(token(4), i64::MAX / 2, now() + Duration::hours(24)), now()).await.unwrap();
    let mut proposal = Proposal {
        id: Uuid::now_v7().to_string(),
        user_id: u.to_string(),
        goal_id: g.id.clone(),
        run_id: None,
        summary: "Move the tempo run to Thursday".into(),
        diff: json!([]),
        size: ProposalSize::Big,
        status: ProposalStatus::Proposed,
        undo: None,
        created_at: now() + Duration::seconds(30),
        sealed_summary: None,
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&f.pool, &f.hlc, u, &mut proposal).await.unwrap();
    f.clock.advance(60 * 1000);

    *f.status.lock().unwrap() = (500, json!({"reason": "InternalServerError"}));
    let failed = f.pusher.dispatch_once().await.unwrap();
    assert_eq!((failed.sent, failed.failed), (0, 1));
    let claims: i64 = db::query_scalar("select count(*) from public.push_sent").fetch_one(&f.pool).await.unwrap();
    assert_eq!(claims, 0);

    *f.status.lock().unwrap() = (200, json!({}));
    assert_eq!(f.pusher.dispatch_once().await.unwrap().sent, 1);
    let last = f.apns.sent().pop().unwrap();
    let payload: Value = serde_json::from_slice(last.body.as_ref().unwrap()).unwrap();
    assert_eq!(payload["aps"]["alert"], json!({"title": "Plan change: Run a 10k", "body": "Move the tempo run to Thursday"}));
    assert_eq!(payload["aroh"]["goal_id"], json!(g.id));

    let mut second = Proposal { id: Uuid::now_v7().to_string(), created_at: now() + Duration::seconds(40), updated_at: String::new(), ..proposal };
    put_row(&f.pool, &f.hlc, u, &mut second).await.unwrap();
    *f.status.lock().unwrap() = (410, json!({"reason": "Unregistered"}));
    let gone = f.pusher.dispatch_once().await.unwrap();
    assert_eq!(gone.removed, 1, "{gone:?}");
    let left: i64 = db::query_scalar("select count(*) from public.devices").fetch_one(&f.pool).await.unwrap();
    assert_eq!(left, 0);
}

const WAIT: &str = r#"
name = "wait"
start = "questions"

[state]
intake = "string"
answers = "string"
plan = "{summary: string}"
approved = "bool"
change = "string"

[[nodes]]
id = "questions"
kind = "human"
ask = "question"
prompt = "Answer these."
reads = ["intake"]
writes = "answers"

[[nodes]]
id = "approve"
kind = "human"
ask = "approval"
prompt = "Approve this plan?"
reads = ["plan"]
writes = "approved"

[[nodes]]
id = "decided"
kind = "router"
condition = "flag(approved)"

[[nodes]]
id = "change"
kind = "human"
ask = "question"
prompt = "What should change?"
reads = ["plan"]
writes = "change"

[[nodes]]
id = "done"
kind = "end"

[[edges]]
from = "questions"
to = "approve"

[[edges]]
from = "approve"
to = "decided"

[[edges]]
from = "decided"
to = "done"
when = "true"

[[edges]]
from = "decided"
to = "change"
when = "false"

[[edges]]
from = "change"
to = "approve"
"#;

fn graph_io() -> SimpleGraphIo {
    SimpleGraphIo::new(Arc::new(SimpleIo::new(ToolRegistry::new())), PersonaLoader::new(vec![]))
}

fn vault(pool: &PgPool) -> Vault {
    Vault::new(pool.clone(), Arc::new(MasterKey::from_base64(MASTER).unwrap()))
}

fn store(pool: &PgPool) -> PgCheckpoints {
    PgCheckpoints::new(pool.clone(), vault(pool), Arc::new(EventBuffer::default()))
}

async fn waiting_run(pool: &PgPool, user: Uuid, goal: Option<&Goal>) -> RunId {
    let io = graph_io();
    let mut input = serde_json::Map::new();
    input.insert("intake".into(), json!("How often do you run?"));
    input.insert("plan".into(), json!({"summary": "Three easy runs a week"}));
    let mut run = GraphRun::new(Workflow::parse("wait.toml", WAIT).unwrap(), input, Default::default(), &io).await.unwrap();
    let goal_id = goal.map(|g| Uuid::parse_str(&g.id).unwrap());
    let row = RunRow { id: run.run_id(), user_id: user, goal_id, workflow: "wait".into(), personas: BTreeMap::new(), plugin_id: None, plugin_version: None, sealed: false };
    insert_run(pool, &row).await.unwrap();
    let store = store(pool);
    store.save(run.checkpoint()).await.unwrap();
    assert!(matches!(run.step(&io).await, GraphOutcome::Waiting(_)));
    store.save(run.checkpoint()).await.unwrap();
    assert_eq!(run.pending_human()[0].node, "questions");
    run.run_id()
}

async fn answer(pool: &PgPool, id: RunId, node: &str, value: Value) {
    let io = graph_io();
    let store = store(pool);
    let mut run = GraphRun::from_checkpoint(&store.latest(id).await.unwrap().unwrap()).unwrap();
    run.resume(node, value).unwrap();
    store.save(run.checkpoint()).await.unwrap();
    assert!(matches!(run.step(&io).await, GraphOutcome::Waiting(_)));
    store.save(run.checkpoint()).await.unwrap();
}

async fn waiting_since(pool: &PgPool, id: RunId, at: DateTime<Utc>) {
    let state: String = db::query_scalar("update public.runs set updated_at = $2 where id = $1 returning state").bind(id.0).bind(at).fetch_one(pool).await.unwrap();
    assert_eq!(state, "waiting_human");
}

async fn stop_run(pool: &PgPool, id: RunId, why: Stop, at: DateTime<Utc>) {
    let store = store(pool);
    let mut run = GraphRun::from_checkpoint(&store.latest(id).await.unwrap().unwrap()).unwrap();
    run.stop(why).unwrap();
    store.save(run.checkpoint()).await.unwrap();
    let state: String = db::query_scalar("update public.runs set updated_at = $2 where id = $1 returning state").bind(id.0).bind(at).fetch_one(pool).await.unwrap();
    assert_eq!(state, "failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_failed_run_pushes_once_and_a_cancelled_one_never() {
    let f = fixture().await;
    let u = user(&f.pool).await;
    let g = goal(&f.pool, &f.hlc, u).await;
    push::register(&f.pool, u, Uuid::now_v7(), device(token(9), i64::MAX / 2, now() + Duration::hours(24)), now()).await.unwrap();
    let limited = waiting_run(&f.pool, u, Some(&g)).await;
    stop_run(&f.pool, limited, Stop::TimeLimit { limit_ms: 300_000 }, now() + Duration::seconds(30)).await;
    let cancelled = waiting_run(&f.pool, u, Some(&g)).await;
    stop_run(&f.pool, cancelled, Stop::Cancelled, now() + Duration::seconds(30)).await;
    f.clock.advance(60 * 1000);

    let summary = f.pusher.dispatch_once().await.unwrap();
    assert_eq!((summary.sent, summary.failed), (1, 0), "{summary:?}");
    let (payload, collapse) = last_payload(&f.apns);
    assert_eq!(payload["aroh"], json!({"kind": "run_failed", "id": limited.0.to_string(), "goal_id": g.id}));
    assert_eq!(payload["aps"]["alert"], json!({"title": "Run a 10k: the run stopped", "body": push::FAIL_BODY}));
    assert!(!payload.to_string().contains("time limit"), "the push never carries the error text");
    assert_eq!(collapse, Some(limited.0.to_string()));
    let again = f.pusher.dispatch_once().await.unwrap();
    assert_eq!((again.sent, again.skipped), (0, 1), "{again:?}");
    f.clock.advance(3 * 60 * 60 * 1000);
    assert_eq!(f.pusher.dispatch_once().await.unwrap(), push::PushSummary { enabled: true, ..Default::default() });
    assert_eq!(f.apns.sent().len(), 1);
}

fn last_payload(apns: &FnTransport) -> (Value, Option<String>) {
    let last = apns.sent().pop().unwrap();
    (serde_json::from_slice(last.body.as_ref().unwrap()).unwrap(), last.header("apns-collapse-id").map(str::to_string))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_waiting_run_pushes_once_per_waiting_node() {
    let f = fixture().await;
    let u = user(&f.pool).await;
    let g = goal(&f.pool, &f.hlc, u).await;
    push::register(&f.pool, u, Uuid::now_v7(), device(token(5), i64::MAX / 2, now() + Duration::hours(24)), now()).await.unwrap();
    let run = waiting_run(&f.pool, u, Some(&g)).await;
    waiting_since(&f.pool, run, now() + Duration::seconds(30)).await;
    f.clock.advance(60 * 1000);

    let summary = f.pusher.dispatch_once().await.unwrap();
    assert_eq!((summary.sent, summary.failed), (1, 0), "{summary:?}");
    let (payload, collapse) = last_payload(&f.apns);
    assert_eq!(payload["aps"]["alert"]["title"], "Run a 10k: your answers are needed");
    assert_eq!(payload["aps"]["thread-id"], "run");
    assert_eq!(payload["aroh"], json!({"kind": "run", "id": run.0.to_string(), "goal_id": g.id, "node": "questions"}));
    assert_eq!(collapse, Some(run.0.to_string()));
    let again = f.pusher.dispatch_once().await.unwrap();
    assert_eq!((again.sent, again.skipped), (0, 1), "{again:?}");

    answer(&f.pool, run, "questions", json!("Three times")).await;
    waiting_since(&f.pool, run, now() + Duration::seconds(90)).await;
    f.clock.advance(60 * 1000);
    assert_eq!(f.pusher.dispatch_once().await.unwrap().sent, 1);
    let (payload, _) = last_payload(&f.apns);
    assert_eq!(payload["aps"]["alert"]["title"], "Run a 10k: plan ready to approve");
    assert_eq!(payload["aroh"]["node"], "approve");
    assert_eq!(f.pusher.dispatch_once().await.unwrap().sent, 0);
    assert_eq!(f.apns.sent().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_new_wait_at_the_same_node_pushes_again() {
    let f = fixture().await;
    let u = user(&f.pool).await;
    let g = goal(&f.pool, &f.hlc, u).await;
    push::register(&f.pool, u, Uuid::now_v7(), device(token(7), i64::MAX / 2, now() + Duration::hours(24)), now()).await.unwrap();
    let run = waiting_run(&f.pool, u, Some(&g)).await;
    answer(&f.pool, run, "questions", json!("Three times")).await;
    waiting_since(&f.pool, run, now() + Duration::seconds(30)).await;
    f.clock.advance(60 * 1000);
    assert_eq!(f.pusher.dispatch_once().await.unwrap().sent, 1);
    assert_eq!(last_payload(&f.apns).0["aroh"]["node"], "approve");
    let again = f.pusher.dispatch_once().await.unwrap();
    assert_eq!((again.sent, again.skipped), (0, 1), "{again:?}");

    answer(&f.pool, run, "approve", json!(false)).await;
    answer(&f.pool, run, "change", json!("Fewer runs")).await;
    waiting_since(&f.pool, run, now() + Duration::seconds(90)).await;
    f.clock.advance(60 * 1000);
    let revised = f.pusher.dispatch_once().await.unwrap();
    assert_eq!(revised.sent, 1, "the revised plan waits at approve again: {revised:?}");
    let (payload, collapse) = last_payload(&f.apns);
    assert_eq!(payload["aroh"]["node"], "approve");
    assert_eq!(payload["aps"]["alert"]["title"], "Run a 10k: plan ready to approve");
    assert_eq!(collapse, Some(run.0.to_string()));
    assert_eq!(f.pusher.dispatch_once().await.unwrap().sent, 0);
    assert_eq!(f.apns.sent().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn stale_and_pre_device_waits_are_not_pushed() {
    let f = fixture().await;
    let u = user(&f.pool).await;
    let g = goal(&f.pool, &f.hlc, u).await;
    push::register(&f.pool, u, Uuid::now_v7(), device(token(6), i64::MAX / 2, now() + Duration::hours(24)), now()).await.unwrap();
    let stale = waiting_run(&f.pool, u, Some(&g)).await;
    waiting_since(&f.pool, stale, now() - Duration::hours(7)).await;
    let before = waiting_run(&f.pool, u, Some(&g)).await;
    waiting_since(&f.pool, before, now() - Duration::hours(1)).await;
    let fresh = waiting_run(&f.pool, u, None).await;
    waiting_since(&f.pool, fresh, now() + Duration::seconds(30)).await;
    f.clock.advance(60 * 1000);

    let summary = f.pusher.dispatch_once().await.unwrap();
    assert_eq!(summary.sent, 1, "{summary:?}");
    let (payload, _) = last_payload(&f.apns);
    assert_eq!(payload["aroh"], json!({"kind": "run", "id": fresh.0.to_string(), "goal_id": null, "node": "questions"}));
    assert_eq!(payload["aps"]["alert"]["title"], "Calendar: your answers are needed");

    f.clock.advance(push::RUN_HOURS * 3600 * 1000);
    let later = f.pusher.dispatch_once().await.unwrap();
    assert_eq!((later.sent, later.skipped), (0, 0), "a wait older than RUN_HOURS is not even considered: {later:?}");
}
