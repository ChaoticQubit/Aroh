mod common;

use aroh_domain::rows::{Action, ActionStatus, Autonomy, Goal, GoalStatus, Proposal, ProposalSize, ProposalStatus};
use aroh_server::db::{self, PgPool};
use aroh_server::fake::{json_reply, FnTransport};
use aroh_server::push::{self, ApnsConfig, ApnsEnv, Platform, Pusher, RegisterDevice, Signer};
use aroh_server::rows::{put_row, utc, ServerClock};
use base64::Engine as _;
use chrono::{DateTime, Duration, Utc};
use common::{pg, user, T0};
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, ECDSA_P256_SHA256_FIXED_SIGNING};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use troupekit_core::clock::{Clock, FixedClock};
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
    let pusher = Pusher::new(pool.clone(), apns.clone(), signer(dyn_clock.clone()), dyn_clock);
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
