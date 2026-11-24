mod common;

use aroh_server::api::router;
use aroh_server::boot;
use aroh_server::config::Config;
use aroh_server::db::{self, PgPool};
use common::{pg, user, MASTER};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

const SECRET: &str = "deploy-test-jwt-secret-0123456789abcdef";

fn token(claims: Value) -> String {
    encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(SECRET.as_bytes())).unwrap()
}

/// Serves the router that `boot::app` builds from production-style vars
/// over the database of `pool`. Returns the base URL of the bound server.
/// Same constructor as the `serve` bin and the Vercel entry.
async fn serve(pool: &PgPool) -> String {
    let name: String = db::query_scalar("select current_database()").fetch_one(pool).await.unwrap();
    let base = common::pooler();
    let vars: BTreeMap<String, String> = [
        ("DATABASE_URL", format!("{base}/{name}")),
        ("AROH_MASTER_KEY", MASTER.to_string()),
        ("AROH_JWT_SECRET", SECRET.to_string()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    let state = boot::app(Config::from_vars(&vars).unwrap()).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    url
}

async fn status(method: &str, url: &str, auth: Option<&str>) -> u16 {
    let client = reqwest::Client::new();
    let mut req = if method == "POST" { client.post(url).json(&json!({})) } else { client.get(url) };
    if let Some(t) = auth {
        req = req.bearer_auth(t);
    }
    req.send().await.unwrap().status().as_u16()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn booted_engine_serves_health_and_step_mode_routes() {
    let pool = pg().await;
    let base = serve(&pool).await;
    let someone = user(&pool).await;
    let user_jwt = token(json!({"sub": someone, "role": "authenticated", "aud": "authenticated", "exp": 4_000_000_000u64}));
    let service_jwt = token(json!({"role": "service_role", "exp": 4_000_000_000u64}));
    assert_eq!(status("GET", &format!("{base}/healthz"), None).await, 200);
    assert_eq!(status("POST", &format!("{base}/v1/tick"), None).await, 401);
    assert_eq!(status("POST", &format!("{base}/v1/tick"), Some(&user_jwt)).await, 403);
    assert_eq!(status("POST", &format!("{base}/v1/tick"), Some(&service_jwt)).await, 200);
    assert_eq!(status("POST", &format!("{base}/v1/push/tick"), Some(&service_jwt)).await, 200);
    assert_eq!(status("POST", &format!("{base}/v1/hooks/enqueue"), Some(&service_jwt)).await, 200);
    assert_eq!(status("GET", &format!("{base}/v1/keys"), Some(&user_jwt)).await, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn wake_trigger_is_installed_and_never_blocks_job_writes() {
    let pool = pg().await;
    let installed: i64 = db::query_scalar("select count(*) from pg_trigger where tgname = 'jobs_wake' and tgrelid = 'public.jobs'::regclass")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(installed, 1);
    let owner = user(&pool).await;
    let job = Uuid::now_v7();
    db::query("insert into public.jobs (id, user_id, kind, status) values ($1, $2, 'run', 'queued')")
        .bind(job)
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    db::query("update public.jobs set status = 'done' where id = $1").bind(job).execute(&pool).await.unwrap();
    db::query("update public.jobs set status = 'queued' where id = $1").bind(job).execute(&pool).await.unwrap();
    let state: String = db::query_scalar("select status from public.jobs where id = $1").bind(job).fetch_one(&pool).await.unwrap();
    assert_eq!(state, "queued");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn wake_trigger_posts_the_hook_only_when_a_job_becomes_queued() {
    let pool = pg().await;
    for sql in [
        "create schema net",
        "create table net.calls (url text, headers jsonb)",
        "create function net.http_post(url text, headers jsonb, body jsonb, timeout_milliseconds integer) returns bigint language sql as $$ insert into net.calls values (url, headers) returning 1::bigint $$",
        "create schema vault",
        "create table vault.decrypted_secrets (name text, decrypted_secret text)",
        "insert into vault.decrypted_secrets values ('aroh_engine_url', 'https://engine.test'), ('aroh_service_jwt', 'svc-token')",
    ] {
        db::query(sql).execute(&pool).await.unwrap();
    }
    let calls = || async { db::query_scalar::<i64>("select count(*) from net.calls").fetch_one(&pool).await.unwrap() };
    let owner = user(&pool).await;
    let job = Uuid::now_v7();
    db::query("insert into public.jobs (id, user_id, kind, status) values ($1, $2, 'run', 'queued')").bind(job).bind(owner).execute(&pool).await.unwrap();
    assert_eq!(calls().await, 1);
    for (status, want) in [("running", 1), ("queued", 1), ("running", 1), ("done", 1), ("queued", 2), ("failed", 2), ("queued", 3)] {
        db::query("update public.jobs set status = $2 where id = $1").bind(job).bind(status).execute(&pool).await.unwrap();
        assert_eq!(calls().await, want, "after {status}");
    }
    let (url, auth): (String, String) = db::query_as("select url, headers->>'Authorization' from net.calls limit 1").fetch_one(&pool).await.unwrap();
    assert_eq!((url.as_str(), auth.as_str()), ("https://engine.test/v1/hooks/enqueue", "Bearer svc-token"));
}
