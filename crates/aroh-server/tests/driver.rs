mod common;

use aroh_server::db::{self, PgPool, PgPoolOptions};
use common::{pg_url, pooler};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// A pool of `max` connections to a fresh database through the pooler,
/// with a short acquire timeout so a leaked slot fails fast.
async fn pool(max: u32) -> PgPool {
    PgPoolOptions::new().max_connections(max).acquire_timeout(Duration::from_secs(5)).connect(&pg_url().await).await.unwrap()
}

/// A one-connection pool straight to Postgres (not the pooler), so the
/// backend pid identifies the physical connection; `idle` is its idle timeout.
async fn direct(idle: Duration) -> PgPool {
    let url = pg_url().await.replacen(&pooler(), &std::env::var("AROH_TEST_PG").unwrap(), 1);
    PgPoolOptions::new().max_connections(1).idle_timeout(idle).acquire_timeout(Duration::from_secs(5)).connect(&url).await.unwrap()
}

async fn backend(pool: &PgPool) -> i32 {
    db::query_scalar("select pg_backend_pid()").fetch_one(pool).await.unwrap()
}

async fn users(pool: &PgPool) -> i64 {
    db::query_scalar("select count(*) from auth.users").fetch_one(pool).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_dropped_transaction_rolls_back_and_frees_its_slot() {
    let pool = pool(1).await;
    let before = users(&pool).await;
    {
        let mut tx = pool.begin().await.unwrap();
        db::query("insert into auth.users (id, email) values ($1, $2)")
            .bind(Uuid::now_v7())
            .bind("dropped@test.local")
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    assert_eq!(users(&pool).await, before, "the dropped insert is not visible");
    let mut tx = pool.begin().await.unwrap();
    db::query("insert into auth.users (id, email) values ($1, $2)").bind(Uuid::now_v7()).bind("kept@test.local").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(users(&pool).await, before + 1, "the committed insert is visible on the one slot");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn statements_report_rows_affected_missing_rows_and_server_errors() {
    let pool = pool(2).await;
    let id = Uuid::now_v7();
    let insert = || db::query("insert into auth.users (id, email) values ($1, $2)").bind(id).bind(format!("{id}@test.local"));
    assert_eq!(insert().execute(&pool).await.unwrap().rows_affected(), 1);
    let none = db::query("update auth.users set email = email where id = $1").bind(Uuid::nil()).execute(&pool).await.unwrap();
    assert_eq!(none.rows_affected(), 0);
    let missing = db::query_scalar::<String>("select email from auth.users where id = $1").bind(Uuid::nil()).fetch_one(&pool).await.unwrap_err();
    assert!(matches!(missing, db::Error::RowNotFound), "{missing}");
    let duplicate = insert().execute(&pool).await.unwrap_err();
    let server = duplicate.as_database_error().expect("the server raised it");
    assert_eq!(server.code().code(), "23505", "unique violation");
    assert!(duplicate.to_string().starts_with("error returned from database: duplicate key"), "{duplicate}");
    let many: Vec<(i32, Option<String>)> = db::query_as("select g, nullif('x', 'x') from generate_series(1, 3) g").fetch_all(&pool).await.unwrap();
    assert_eq!(many, vec![(1, None), (2, None), (3, None)]);
    let first: Option<i32> = db::query_scalar("select g from generate_series(1, 3) g").fetch_optional(&pool).await.unwrap();
    assert_eq!(first, Some(1), "like sqlx, more rows than one is not an error");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs AROH_TEST_PG"]
async fn fifty_concurrent_requests_through_the_pooler_all_succeed() {
    let pool = pool(20).await;
    let before = users(&pool).await;
    let started = Instant::now();
    let tasks: Vec<_> = (0..50)
        .map(|i| {
            let pool = pool.clone();
            tokio::spawn(async move {
                let id = Uuid::now_v7();
                let mut tx = pool.begin().await?;
                db::query("insert into auth.users (id, email) values ($1, $2)").bind(id).bind(format!("load{i}@test.local")).execute(&mut *tx).await?;
                let seen: Option<String> = db::query_scalar("select email from auth.users where id = $1").bind(id).fetch_optional(&mut *tx).await?;
                assert_eq!(seen.as_deref(), Some(format!("load{i}@test.local").as_str()), "a transaction reads its own write");
                tx.commit().await?;
                db::query_scalar::<i64>("select count(*) from auth.users where email like $1").bind("load%").fetch_one(&pool).await
            })
        })
        .collect();
    let mut errors = Vec::new();
    for task in tasks {
        if let Err(e) = task.await.expect("load task runs") {
            errors.push(e.to_string());
        }
    }
    let wall = started.elapsed();
    eprintln!("load: 50 requests, {} errors, {} ms wall, client pool 20, pooler server pool 4", errors.len(), wall.as_millis());
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(users(&pool).await, before + 50);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_connection_idle_past_the_timeout_is_replaced() {
    let pool = direct(Duration::from_secs(1)).await;
    let first = backend(&pool).await;
    assert_eq!(backend(&pool).await, first, "a fresh idle connection is reused");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_ne!(backend(&pool).await, first, "an idle-expired connection is closed, not reused");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_dead_pooled_connection_is_replaced_on_checkout() {
    let pool = direct(Duration::from_secs(600)).await;
    let first = backend(&pool).await;
    let admin = PgPoolOptions::new().max_connections(1).connect(&format!("{}/postgres", std::env::var("AROH_TEST_PG").unwrap())).await.unwrap();
    let killed: bool = db::query_scalar("select pg_terminate_backend($1)").bind(first).fetch_one(&admin).await.unwrap();
    assert!(killed);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_ne!(backend(&pool).await, first, "the checkout skipped the dead connection");
    admin.close().await;
}
