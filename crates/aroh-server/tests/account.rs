mod common;

use aroh_domain::rows::{Autonomy, Goal, GoalStatus};
use aroh_server::account::StorageApi;
use aroh_server::api::{router, AppState};
use aroh_server::auth::Verifier;
use aroh_server::db::{self, PgPool};
use aroh_server::engine::Engine;
use aroh_server::error::ServerError;
use aroh_server::fake::{body_text, json_reply, FnTransport};
use aroh_server::rows::{put_row, utc, ServerClock};
use common::{deps, offline, pg, user, T0};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};
use std::sync::Arc;
use troupekit_core::clock::FixedClock;
use troupekit_core::http::{read_limited, HttpRequest, Method, Transport};
use troupekit_providers::reqwest_transport::ReqwestTransport;
use uuid::Uuid;

const SECRET: &str = "account-test-jwt-secret-0123456789abcdef";
const STORAGE_URL: &str = "https://storage.test";
const SECRET_KEY: &str = "sb_secret_account-test";
const DAY_MS: i64 = 86_400_000;

struct Api {
    base: String,
    http: ReqwestTransport,
    pool: PgPool,
    storage: Arc<FnTransport>,
    clock: Arc<FixedClock>,
}

fn service_token() -> String {
    let claims = json!({"role": "service_role", "exp": 4_000_000_000u64});
    encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(SECRET.as_bytes())).unwrap()
}

fn user_token(user: Uuid) -> String {
    let claims = json!({"sub": user, "role": "authenticated", "aud": "authenticated", "exp": 4_000_000_000u64});
    encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(SECRET.as_bytes())).unwrap()
}

async fn api(with_storage: bool) -> Api {
    let pool = pg().await;
    let (deps, clock) = deps(&pool, offline(), offline());
    let storage = FnTransport::new(|_| json_reply(200, json!([])));
    let mut engine = Engine::new(deps);
    if with_storage {
        let transport: Arc<dyn Transport> = storage.clone();
        engine = engine.with_storage(StorageApi::new(STORAGE_URL, SECRET_KEY, transport));
    }
    let verifier = Verifier::new(Some(SECRET), None, "authenticated", clock.clone()).with_disabled_check(pool.clone());
    let state = AppState { engine: Arc::new(engine), verifier: Arc::new(verifier) };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    Api { base, http: ReqwestTransport::new().unwrap(), pool, storage, clock }
}

impl Api {
    async fn call(&self, method: Method, path: &str, token: &str, body: Option<Value>) -> (u16, Value) {
        let req = HttpRequest {
            method,
            url: format!("{}{path}", self.base),
            headers: vec![("content-type".into(), "application/json".into()), ("authorization".into(), format!("Bearer {token}"))],
            body: body.map(|b| b.to_string().into_bytes()),
            accept: vec![],
        };
        let mut res = self.http.send(req).await.unwrap();
        let (bytes, _) = read_limited(res.body.as_mut(), 1 << 20).await.unwrap();
        (res.status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn with_key(&self, u: Uuid) -> String {
        let t = user_token(u);
        let body = json!({"vendor": "anthropic", "key": "sk-ant-test-ACCOUNT-1234", "model": "claude-sonnet-5-5", "label": "Main", "is_default": true});
        let (status, info) = self.call(Method::Put, &format!("/v1/keys/{}", Uuid::now_v7()), &t, Some(body)).await;
        assert_eq!(status, 200, "{info}");
        t
    }

    async fn log(&self, t: &str) -> (u16, Value) {
        self.call(Method::Post, "/v1/logs", t, Some(json!({"text": "ran 5 km", "timezone": "UTC"}))).await
    }

    async fn tick(&self) -> Value {
        let (status, report) = self.call(Method::Post, "/v1/tick", &service_token(), None).await;
        assert_eq!(status, 200, "{report}");
        report
    }

    async fn job(&self, u: Uuid) -> (String, chrono::DateTime<chrono::Utc>) {
        db::query_as("select status, run_after from public.jobs where user_id = $1").bind(u).fetch_one(&self.pool).await.unwrap()
    }

    async fn count(&self, sql: &str, u: Uuid) -> i64 {
        db::query_scalar(sql).bind(u).fetch_one(&self.pool).await.unwrap()
    }
}

fn goal(u: Uuid, status: GoalStatus) -> Goal {
    Goal {
        id: Uuid::now_v7().to_string(),
        user_id: u.to_string(),
        category_id: None,
        title: "Read more".into(),
        context: None,
        deadline: None,
        status,
        autonomy: Autonomy::Balanced,
        created_at: utc(T0),
        reminders: true,
        use_calendars: true,
        run_limit_minutes: None,
        updated_at: String::new(),
        deleted: false,
    }
}

async fn stranger(pool: &PgPool) -> Uuid {
    let id = Uuid::now_v7();
    db::query("insert into auth.users (id, email) values ($1, $2)").bind(id).bind(format!("{id}@test.local")).execute(pool).await.unwrap();
    id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn runs_need_consent_and_only_owner_runs_stop_at_ten_a_day() {
    let api = api(false).await;
    let u = stranger(&api.pool).await;
    let t = api.with_key(u).await;
    let (status, view) = api.call(Method::Get, "/v1/account", &t, None).await;
    assert_eq!((status, &view["ai_consent_at"]), (200, &Value::Null), "{view}");
    let (status, err) = api.call(Method::Post, "/v1/goals", &t, Some(json!({"title": "Run a 5k", "timezone": "UTC"}))).await;
    assert_eq!((status, err["error"]["code"].as_str()), (403, Some("consent_required")), "{err}");
    assert_eq!(api.count("select count(*) from public.goals where user_id = $1", u).await, 0, "a refused start writes no goal");
    assert_eq!(api.log(&t).await.1["error"]["code"], "consent_required", "system runs need consent too");
    let (status, view) = api.call(Method::Put, "/v1/account/consent", &t, Some(json!({"ai": true}))).await;
    assert!(status == 200 && view["ai_consent_at"].is_string(), "{view}");

    for i in 0..12 {
        assert_eq!(api.log(&t).await.0, 202, "system run {i} is never capped");
    }
    let (_, view) = api.call(Method::Get, "/v1/account", &t, None).await;
    assert_eq!(view["runs_today"].as_i64(), Some(0), "system runs never count: {view}");
    for (i, workflow) in ["new-goal", "run-again", "card-chat"].iter().cycle().take(9).enumerate() {
        db::query("insert into public.runs (id, user_id, workflow, state, personas) values ($1, $2, $3, 'done', '{}')")
            .bind(Uuid::now_v7())
            .bind(u)
            .bind(workflow)
            .execute(&api.pool)
            .await
            .unwrap_or_else(|e| panic!("run {i}: {e}"));
    }
    let (status, body) = api.call(Method::Post, "/v1/goals", &t, Some(json!({"title": "Run a 5k", "timezone": "UTC"}))).await;
    assert_eq!(status, 202, "the tenth owner run starts: {body}");
    let (status, err) = api.call(Method::Post, "/v1/goals", &t, Some(json!({"title": "Swim", "timezone": "UTC"}))).await;
    assert_eq!((status, err["error"]["code"].as_str()), (429, Some("daily_run_limit")), "{err}");
    assert!(err["error"]["message"].as_str().unwrap().contains("10 agent runs"), "{err}");
    let (_, view) = api.call(Method::Get, "/v1/account", &t, None).await;
    assert_eq!((view["runs_today"].as_i64(), view["max_runs_per_day"].as_i64()), (Some(10), Some(10)), "{view}");
    assert_eq!(api.log(&t).await.0, 202, "a capped owner still gets system runs");
    db::query("update public.runs set created_at = now() - interval '25 hours' where user_id = $1").bind(u).execute(&api.pool).await.unwrap();
    let (status, body) = api.call(Method::Post, "/v1/goals", &t, Some(json!({"title": "Swim", "timezone": "UTC"}))).await;
    assert_eq!(status, 202, "runs older than 24 hours no longer count: {body}");
    let (status, _) = api.call(Method::Put, "/v1/account/consent", &t, Some(json!({"ai": false}))).await;
    assert_eq!(status, 200);
    assert_eq!(api.log(&t).await.1["error"]["code"], "consent_required");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn five_open_goals_at_most() {
    let api = api(false).await;
    let u = user(&api.pool).await;
    let t = api.with_key(u).await;
    let hlc = ServerClock::default();
    for status in [GoalStatus::Draft, GoalStatus::Active, GoalStatus::Active, GoalStatus::Paused, GoalStatus::Active, GoalStatus::Archived, GoalStatus::Done] {
        put_row(&api.pool, &hlc, u, &mut goal(u, status)).await.unwrap();
    }
    let (status, err) = api.call(Method::Post, "/v1/goals", &t, Some(json!({"title": "Learn Spanish", "timezone": "UTC"}))).await;
    assert_eq!((status, err["error"]["code"].as_str()), (409, Some("goal_limit")), "{err}");
    assert!(err["error"]["message"].as_str().unwrap().contains("Archive one to add another"), "{err}");
    let direct = put_row(&api.pool, &hlc, u, &mut goal(u, GoalStatus::Active)).await.unwrap_err();
    assert!(matches!(&direct, ServerError::Refused { code: "goal_limit", .. }), "{direct:?}");
    assert_eq!(api.count("select count(*) from public.goals where user_id = $1", u).await, 7);
    let (_, view) = api.call(Method::Get, "/v1/account", &t, None).await;
    assert_eq!((view["open_goals"].as_i64(), view["max_open_goals"].as_i64()), (Some(5), Some(5)), "{view}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn deletion_disables_at_once_and_signing_in_cancels_it() {
    let api = api(false).await;
    let u = user(&api.pool).await;
    let t = api.with_key(u).await;
    put_row(&api.pool, &ServerClock::default(), u, &mut goal(u, GoalStatus::Active)).await.unwrap();
    let device = json!({"platform": "ios", "token": "ab".repeat(32), "environment": "sandbox", "zone": "UTC", "synced_seq": 0, "scheduled_until": "2026-11-23T00:00:00Z"});
    let (status, body) = api.call(Method::Put, &format!("/v1/devices/{}", Uuid::now_v7()), &t, Some(device)).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(api.log(&t).await.0, 202);

    let (status, first) = api.call(Method::Delete, "/v1/account", &t, None).await;
    assert_eq!((status, first["purge_after"].as_str()), (202, Some("2026-12-07T12:00:00Z")), "{first}");
    api.clock.advance(DAY_MS);
    let (status, again) = api.call(Method::Delete, "/v1/account", &t, None).await;
    assert_eq!((status, &again), (202, &first), "a repeat keeps the first date");
    api.tick().await;
    let parked = utc(T0 + DAY_MS + 900_000);
    assert_eq!(api.job(u).await, ("queued".to_string(), parked), "a disabled account's run waits");
    for (method, path) in [(Method::Get, "/v1/account"), (Method::Get, "/v1/runs"), (Method::Get, "/v1/keys")] {
        let (status, err) = api.call(method, path, &t, None).await;
        assert_eq!((status, err["error"]["code"].as_str()), (403, Some("account_disabled")), "{path}: {err}");
    }
    let refused = aroh_server::account::check_run(&api.pool, u, "replan").await.unwrap_err();
    assert!(matches!(refused, ServerError::Refused { code: "account_disabled", .. }), "triggers start no run: {refused:?}");
    assert_eq!(api.count("select count(*) from public.devices where user_id = $1", u).await, 0, "devices are signed out");

    db::query("update auth.users set last_sign_in_at = now() where id = $1").bind(u).execute(&api.pool).await.unwrap();
    assert_eq!(api.call(Method::Get, "/v1/keys", &t, None).await.0, 200, "signing back in restores the account");
    assert_eq!(api.count("select count(*) from public.account_deletions where user_id = $1", u).await, 0);
    api.clock.advance(20 * DAY_MS);
    api.tick().await;
    assert_ne!(api.job(u).await, ("queued".to_string(), parked), "the run goes on after the sign-in");
    assert_eq!(api.count("select count(*) from public.goals where user_id = $1", u).await, 1, "a cancelled deletion never purges");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn deletion_purges_everything_after_fifteen_days() {
    let api = api(true).await;
    let u = user(&api.pool).await;
    let other = user(&api.pool).await;
    let t = api.with_key(u).await;
    api.with_key(other).await;
    put_row(&api.pool, &ServerClock::default(), u, &mut goal(u, GoalStatus::Active)).await.unwrap();
    assert_eq!(api.log(&t).await.0, 202);
    for name in [format!("{u}/goals/g/a.pdf"), format!("{u}/goals/g/b.pdf"), format!("{other}/goals/g/c.pdf")] {
        db::query("insert into storage.objects (bucket_id, name) values ('goal-files', $1)").bind(name).execute(&api.pool).await.unwrap();
    }
    assert_eq!(api.call(Method::Delete, "/v1/account", &t, None).await.0, 202);

    api.clock.advance(15 * DAY_MS - 1);
    api.tick().await;
    assert!(api.storage.sent().is_empty(), "nothing is purged inside the grace period");
    assert_eq!(api.count("select count(*) from auth.users where id = $1", u).await, 1);

    api.clock.advance(1);
    let report = api.tick().await;
    assert!(report["errors"].to_string().contains("files_remain"), "the fake storage removed nothing: {report}");
    let sent = api.storage.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!((sent[0].method, sent[0].url.as_str()), (Method::Delete, "https://storage.test/storage/v1/object/goal-files"));
    assert!(sent[0].headers.contains(&("apikey".into(), SECRET_KEY.into())));
    assert!(!sent[0].headers.iter().any(|(k, _)| k == "authorization"), "a secret key goes on apikey only");
    let prefixes: Value = serde_json::from_str(&body_text(&sent[0])).unwrap();
    assert_eq!(prefixes, json!({"prefixes": [format!("{u}/goals/g/a.pdf"), format!("{u}/goals/g/b.pdf")]}));
    assert_eq!(api.count("select count(*) from auth.users where id = $1", u).await, 1, "nothing is deleted while files remain");

    db::query("delete from storage.objects where name like $1 || '/%'").bind(u.to_string()).execute(&api.pool).await.unwrap();
    let report = api.tick().await;
    assert_eq!(report["errors"], json!([]), "{report}");
    let tables: Vec<String> = db::query_scalar(
        "select c.table_name::text from information_schema.columns c join information_schema.tables tb
           on tb.table_schema = c.table_schema and tb.table_name = c.table_name
         where c.table_schema = 'public' and c.column_name = 'user_id' and tb.table_type = 'BASE TABLE'",
    )
    .fetch_all(&api.pool)
    .await
    .unwrap();
    assert!(tables.len() > 30, "{tables:?}");
    for table in &tables {
        let left = api.count(&format!("select count(*) from public.{table} where user_id = $1"), u).await;
        assert_eq!(left, 0, "{table} still holds the deleted account's rows");
    }
    assert_eq!(api.count("select count(*) from auth.users where id = $1", u).await, 0);
    assert_eq!(api.count("select count(*) from public.user_keys where user_id = $1", other).await, 1, "another account is untouched");
    assert_eq!(api.count("select count(*) from storage.objects where name like $1::text || '/%'", other).await, 1);
    assert_eq!(api.tick().await["errors"], json!([]), "a purged account is not purged again");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn files_without_a_storage_key_keep_the_purge_pending() {
    let api = api(false).await;
    let u = user(&api.pool).await;
    let t = user_token(u);
    db::query("insert into storage.objects (bucket_id, name) values ('goal-files', $1)").bind(format!("{u}/goals/g/a.pdf")).execute(&api.pool).await.unwrap();
    assert_eq!(api.call(Method::Delete, "/v1/account", &t, None).await.0, 202);
    api.clock.advance(16 * DAY_MS);
    let report = api.tick().await;
    assert!(report["errors"].to_string().contains("AROH_SUPABASE_SECRET_KEY"), "{report}");
    assert_eq!(api.count("select count(*) from auth.users where id = $1", u).await, 1);
}
