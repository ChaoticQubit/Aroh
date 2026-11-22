mod common;

use aroh_domain::rows::{Goal, Usage};
use aroh_server::checkpoints::{events_after, EventBuffer, PgCheckpoints};
use aroh_server::db::{self, PgPool};
use aroh_server::error::ServerError;
use aroh_server::ledger::{goal_key, user_key, PgLedger};
use aroh_server::queue::PgQueue;
use aroh_server::rows::{insert_run, put_row, utc, RunRow, ServerClock};
use aroh_server::vault::{MasterKey, Vault};
use common::{pg, user, MASTER, T0};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use troupekit_agent::{EventSink, PersonaLoader, SimpleIo, UsageLedger};
use troupekit_core::clock::{Clock, FixedClock};
use troupekit_core::error::CoreError;
use troupekit_core::event::{Event, EventBody};
use troupekit_core::ids::RunId;
use troupekit_core::secret::Secret;
use troupekit_core::store::{Checkpoint, CheckpointStore};
use troupekit_graph::testing::FnTool;
use troupekit_graph::{GraphRun, GraphStatus, SimpleGraphIo, Workflow};
use troupekit_host::{JobQueue, Runner, TickOptions};
use troupekit_tools::{ToolOutput, ToolRegistry};
use uuid::Uuid;

fn key(text: &str) -> Secret {
    Secret::new(text.to_string()).unwrap()
}

fn vault(pool: &PgPool) -> Vault {
    Vault::new(pool.clone(), Arc::new(MasterKey::from_base64(MASTER).unwrap()))
}

async fn run_row(pool: &PgPool, user: Uuid) -> RunId {
    let id = RunId::new();
    let row = RunRow { id, user_id: user, goal_id: None, workflow: "test".into(), personas: BTreeMap::new() };
    insert_run(pool, &row).await.unwrap();
    id
}

fn state(seq: u64, status: &str) -> Value {
    json!({"seq": seq, "status": {"status": status}})
}

fn event(run_id: RunId, seq: u64, step: &str) -> Event {
    Event { run_id, seq, at_ms: T0, body: EventBody::StepStarted { step: step.into() } }
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn server_schema_columns_exist() {
    let pool = pg().await;
    let cols: Vec<String> = db::query_scalar(
        "select table_name || '.' || column_name from information_schema.columns
         where table_schema = 'public' and (table_name, column_name) in
         (('jobs','run_id'),('jobs','lease_token'),('jobs','next_run_after'),('jobs','error'),('runs','personas'),
          ('provider_credentials','vendor'),('provider_credentials','last4'),('user_keys','wrapped'))
         order by 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(cols.len(), 8, "{cols:?}");
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn vault_round_trip_per_user() {
    let pool = pg().await;
    let (a, b) = (user(&pool).await, user(&pool).await);
    let v = vault(&pool);
    let setting = Uuid::now_v7();
    let info = v.set(a, setting, "anthropic", &key("sk-ant-alpha-1234")).await.unwrap();
    assert_eq!((info.vendor.as_str(), info.last4.as_str()), ("anthropic", "1234"));
    v.set(b, setting, "anthropic", &key("sk-ant-bravo-5678")).await.unwrap();
    assert_eq!(v.load(a, setting).await.unwrap().unwrap().expose(), "sk-ant-alpha-1234");
    assert_eq!(v.load(b, setting).await.unwrap().unwrap().expose(), "sk-ant-bravo-5678");
    v.set(a, setting, "anthropic", &key("sk-ant-alpha-9999")).await.unwrap();
    assert_eq!(v.load(a, setting).await.unwrap().unwrap().expose(), "sk-ant-alpha-9999");
    assert!(v.load(a, Uuid::now_v7()).await.unwrap().is_none());
    assert!(v.delete(a, setting).await.unwrap());
    assert!(!v.delete(a, setting).await.unwrap());
    assert!(v.load(a, setting).await.unwrap().is_none());
    assert!(v.set(a, setting, "anthropic", &key("short")).await.is_err());
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn other_user_cannot_open_a_moved_key() {
    let pool = pg().await;
    let (a, b) = (user(&pool).await, user(&pool).await);
    let v = vault(&pool);
    let (sa, sb) = (Uuid::now_v7(), Uuid::now_v7());
    v.set(a, sa, "anthropic", &key("sk-ant-alpha-1234")).await.unwrap();
    v.set(b, sb, "anthropic", &key("sk-ant-bravo-5678")).await.unwrap();
    db::query("update public.provider_credentials set user_id = $1 where user_id = $2 and setting_id = $3")
        .bind(b)
        .bind(a)
        .bind(sa)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(v.load(b, sa).await, Err(ServerError::Crypto(_))));
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn tampered_row_fails() {
    let pool = pg().await;
    let a = user(&pool).await;
    let v = vault(&pool);
    for _ in 0..512 {
        let s = Uuid::now_v7();
        v.set(a, s, "openai", &key("sk-proj-abcdefgh")).await.unwrap();
        db::query("update public.provider_credentials set ciphertext = set_byte(ciphertext, 0, get_byte(ciphertext, 0) # 1) where setting_id = $1")
            .bind(s)
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(v.load(a, s).await, Err(ServerError::Crypto(_))));
    }
    let other = vault_with(&pool, "CAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAg=");
    let s2 = Uuid::now_v7();
    v.set(a, s2, "openai", &key("sk-proj-abcdefgh")).await.unwrap();
    assert!(matches!(other.load(a, s2).await, Err(ServerError::Crypto(_))));
}

fn vault_with(pool: &PgPool, master: &str) -> Vault {
    Vault::new(pool.clone(), Arc::new(MasterKey::from_base64(master).unwrap()))
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn list_shows_vendor_and_last4_only_and_ciphertext_hides_the_key() {
    let pool = pg().await;
    let a = user(&pool).await;
    let v = vault(&pool);
    let s = Uuid::now_v7();
    v.set(a, s, "anthropic", &key("sk-ant-secret-KEYTAIL")).await.unwrap();
    let listed = serde_json::to_value(v.list(a).await.unwrap()).unwrap();
    assert_eq!(listed[0]["last4"], "TAIL");
    assert_eq!(listed[0].as_object().unwrap().len(), 4, "{listed}");
    assert!(!listed.to_string().contains("secret"));
    let dump: String = db::query_scalar(
        "select coalesce(string_agg(encode(ciphertext, 'escape') || encode(nonce, 'escape'), ''), '') from public.provider_credentials",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!dump.contains("sk-ant-secret"));
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn checkpoint_save_needs_the_next_step_and_writes_events() {
    let pool = pg().await;
    let u = user(&pool).await;
    let id = run_row(&pool, u).await;
    let buf = Arc::new(EventBuffer::default());
    let store = PgCheckpoints::new(pool.clone(), buf.clone());
    let missing = store.save(Checkpoint { run_id: RunId::new(), step: 0, state: state(0, "queued") }).await;
    assert!(matches!(missing, Err(CoreError::NotFound(_))));
    assert!(matches!(
        store.save(Checkpoint { run_id: id, step: 1, state: state(0, "queued") }).await,
        Err(CoreError::Conflict { expected: 0, actual: 1 })
    ));
    store.save(Checkpoint { run_id: id, step: 0, state: state(0, "queued") }).await.unwrap();
    for seq in 0..3 {
        buf.emit(event(id, seq, "first"));
    }
    store.save(Checkpoint { run_id: id, step: 1, state: state(3, "checkpointed") }).await.unwrap();
    assert_eq!(events_after(&pool, id, None).await.unwrap().len(), 3);
    assert_eq!(events_after(&pool, id, Some(0)).await.unwrap().len(), 2);
    assert!(matches!(
        store.save(Checkpoint { run_id: id, step: 1, state: state(3, "checkpointed") }).await,
        Err(CoreError::Conflict { expected: 2, actual: 1 })
    ));
    buf.emit(event(id, 1, "replayed"));
    store.save(Checkpoint { run_id: id, step: 2, state: state(2, "waiting_human") }).await.unwrap();
    let events = events_after(&pool, id, None).await.unwrap();
    assert_eq!(events.len(), 2, "the stale tail at seq 2 is gone");
    assert_eq!(events[1].body, EventBody::StepStarted { step: "replayed".into() });
    let latest = store.latest(id).await.unwrap().unwrap();
    assert_eq!(latest.step, 2);
    let mirrored: String = db::query_scalar("select state from public.runs where id = $1").bind(id.0).fetch_one(&pool).await.unwrap();
    assert_eq!(mirrored, "waiting_human");
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn queue_claims_leases_and_completes() {
    let pool = pg().await;
    let u = user(&pool).await;
    let clock = Arc::new(FixedClock::new(T0));
    let q = PgQueue::new(pool.clone(), clock.clone());
    assert!(matches!(q.enqueue(RunId::new(), T0).await, Err(CoreError::NotFound(_))));
    let (r1, r2) = (run_row(&pool, u).await, run_row(&pool, u).await);
    q.enqueue(r1, T0).await.unwrap();
    q.enqueue(r2, T0 + 5_000).await.unwrap();
    let leases = q.claim_due(10, 1_000).await.unwrap();
    assert_eq!(leases.len(), 1);
    assert_eq!((leases[0].run_id, leases[0].attempts), (r1, 1));
    assert!(q.claim_due(10, 1_000).await.unwrap().is_empty());
    q.enqueue(r1, T0).await.unwrap();
    q.complete(&leases[0]).await.unwrap();
    assert_eq!(q.status(r1).await.unwrap().unwrap().0, "queued", "an enqueue while running re-queues on complete");
    let again = q.claim_due(10, 1_000).await.unwrap();
    assert_eq!(again.len(), 1);
    q.complete(&again[0]).await.unwrap();
    assert_eq!(q.status(r1).await.unwrap().unwrap().0, "done");
    assert!(matches!(q.complete(&again[0]).await, Err(CoreError::Conflict { .. })));
    clock.advance(5_000);
    let l2 = q.claim_due(10, 1_000).await.unwrap();
    assert_eq!(l2[0].run_id, r2);
    q.fail(&l2[0], "boom").await.unwrap();
    assert_eq!(q.status(r2).await.unwrap().unwrap(), ("failed".to_string(), Some("boom".to_string())));
    q.enqueue(r2, T0).await.unwrap();
    assert_eq!(q.status(r2).await.unwrap().unwrap().0, "queued");
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn expired_lease_is_reclaimed_and_the_stale_token_loses() {
    let pool = pg().await;
    let u = user(&pool).await;
    let clock = Arc::new(FixedClock::new(T0));
    let q = PgQueue::new(pool.clone(), clock.clone());
    let r = run_row(&pool, u).await;
    q.enqueue(r, T0).await.unwrap();
    let stale = q.claim_due(1, 1_000).await.unwrap().remove(0);
    clock.advance(1_000);
    let fresh = q.claim_due(1, 1_000).await.unwrap().remove(0);
    assert_eq!(fresh.attempts, 2);
    assert!(fresh.token > stale.token);
    assert!(matches!(q.complete(&stale).await, Err(CoreError::Conflict { .. })));
    assert!(matches!(q.reschedule(&stale, T0).await, Err(CoreError::Conflict { .. })));
    q.reschedule(&fresh, T0 + 10_000).await.unwrap();
    assert!(q.claim_due(1, 1_000).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs AROH_TEST_PG"]
async fn concurrent_claims_skip_locked_rows() {
    let pool = pg().await;
    let u = user(&pool).await;
    let clock = Arc::new(FixedClock::new(T0));
    let q = Arc::new(PgQueue::new(pool.clone(), clock.clone()));
    for _ in 0..20 {
        let r = run_row(&pool, u).await;
        q.enqueue(r, T0).await.unwrap();
    }
    let claims = futures_util::future::join_all((0..4).map(|_| {
        let q = q.clone();
        tokio::spawn(async move { q.claim_due(5, 10_000).await.unwrap() })
    }))
    .await;
    let mut ids: Vec<RunId> = claims.into_iter().flat_map(|c| c.unwrap()).map(|l| l.run_id).collect();
    let total = ids.len();
    ids.sort_by_key(|r| r.0);
    ids.dedup();
    assert_eq!(ids.len(), total, "no run is leased twice");
    assert_eq!(total, 20);
}

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn ledger_sums_the_calendar_month_by_goal_and_user() {
    let pool = pg().await;
    let (u, other) = (user(&pool).await, user(&pool).await);
    let hlc = ServerClock::default();
    let goal = Uuid::now_v7();
    let mut g = Goal {
        id: goal.to_string(),
        user_id: u.to_string(),
        category_id: None,
        title: "Run".into(),
        context: None,
        deadline: None,
        status: aroh_domain::rows::GoalStatus::Active,
        autonomy: aroh_domain::rows::Autonomy::Balanced,
        created_at: utc(T0),
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&pool, &hlc, u, &mut g).await.unwrap();
    let row = |owner: Uuid, goal: Option<Uuid>, cost: i64, at: i64| Usage {
        id: aroh_domain::new_id(),
        user_id: owner.to_string(),
        goal_id: goal.map(|g| g.to_string()),
        run_id: None,
        vendor: "anthropic".into(),
        model: "claude-sonnet-5-5".into(),
        input_tokens: 1,
        output_tokens: 1,
        cost_usd_micros: cost,
        occurred_at: utc(at),
        updated_at: String::new(),
        deleted: false,
    };
    let day = 86_400_000;
    for (owner, goal, cost, at) in [
        (u, Some(goal), 100, T0),
        (u, Some(goal), 20, T0 - day),
        (u, None, 3, T0),
        (u, Some(goal), 1_000, T0 - 30 * day),
        (other, None, 7_000, T0),
    ] {
        put_row(&pool, &hlc, owner, &mut row(owner, goal, cost, at)).await.unwrap();
    }
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(T0));
    let ledger = PgLedger::new(pool.clone(), u, clock);
    assert_eq!(ledger.period_total(&goal_key(goal)).await.unwrap(), 120);
    assert_eq!(ledger.period_total(&user_key(u)).await.unwrap(), 123);
    assert!(ledger.period_total(&user_key(other)).await.is_err());
    assert!(ledger.period_total("nope").await.is_err());
}

const LINEAR: &str = r#"
name = "linear"
start = "a"

[state]
topic = "string"
a = "string"
b = "string"

[[nodes]]
id = "a"
kind = "tool"
tool = "echo"
reads = ["topic"]
writes = "a"

[[nodes]]
id = "b"
kind = "tool"
tool = "echo"
reads = ["a"]
writes = "b"

[[nodes]]
id = "done"
kind = "end"

[[edges]]
from = "a"
to = "b"

[[edges]]
from = "b"
to = "done"
"#;

fn echo_io(clock: Arc<FixedClock>, keys: Arc<Mutex<Vec<String>>>) -> Arc<SimpleGraphIo> {
    let mut tools = ToolRegistry::new();
    tools
        .register(FnTool::new("echo", move |args, ctx| {
            let keys = keys.clone();
            async move {
                keys.lock().unwrap().push(ctx.idempotency_key.unwrap_or_default());
                Ok(ToolOutput::text(json!(args.to_string()).to_string()))
            }
        }))
        .unwrap();
    let mut agent = SimpleIo::new(tools);
    agent.clock = clock;
    Arc::new(SimpleGraphIo::new(Arc::new(agent), PersonaLoader::new(vec![])))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs AROH_TEST_PG"]
async fn two_runners_on_postgres_never_advance_a_run_twice() {
    let pool = pg().await;
    let u = user(&pool).await;
    let clock = Arc::new(FixedClock::new(T0));
    let keys = Arc::new(Mutex::new(Vec::new()));
    let io = echo_io(clock.clone(), keys.clone());
    let queue: Arc<dyn JobQueue> = Arc::new(PgQueue::new(pool.clone(), clock.clone()));
    let runner = || {
        let cps: Arc<dyn CheckpointStore> = Arc::new(PgCheckpoints::new(pool.clone(), Arc::new(EventBuffer::default())));
        Arc::new(Runner::new(queue.clone(), cps, io.clone()))
    };
    let mut ids = Vec::new();
    for i in 0..6 {
        let wf = Workflow::parse("linear.toml", LINEAR).unwrap();
        let mut input = serde_json::Map::new();
        input.insert("topic".into(), json!(format!("goal {i}")));
        let run = GraphRun::new(wf, input, Default::default(), io.as_ref()).await.unwrap();
        let row = RunRow { id: run.run_id(), user_id: u, goal_id: None, workflow: "linear".into(), personas: BTreeMap::new() };
        insert_run(&pool, &row).await.unwrap();
        runner().enqueue(&run).await.unwrap();
        ids.push(run.run_id());
    }
    let (a, b) = (runner(), runner());
    let opts = TickOptions { max_jobs: 3, ..TickOptions::default() };
    for _ in 0..30 {
        let (ra, rb) = (a.clone(), b.clone());
        let (x, y) = tokio::join!(tokio::spawn(async move { ra.tick(opts).await }), tokio::spawn(async move { rb.tick(opts).await }));
        for report in [x.unwrap().unwrap(), y.unwrap().unwrap()] {
            assert!(report.errors.is_empty(), "{:?}", report.errors);
        }
    }
    let store = PgCheckpoints::new(pool.clone(), Arc::new(EventBuffer::default()));
    let q = PgQueue::new(pool.clone(), clock.clone());
    for id in &ids {
        let status = q.status(*id).await.unwrap().unwrap();
        assert_eq!(status.0, "done", "{status:?}");
        let cp = store.latest(*id).await.unwrap().unwrap();
        assert_eq!(GraphRun::from_checkpoint(&cp).unwrap().status(), &GraphStatus::Done);
    }
    let mut keys = keys.lock().unwrap().clone();
    let n = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), n, "every tool call ran once");
    assert_eq!(n, ids.len() * 2);
}
