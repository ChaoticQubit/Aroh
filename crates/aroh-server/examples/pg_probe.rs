use aroh_server::db::{self as pg, PgPool, PgPoolOptions};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;

const COUNT: &str = "select count(*) from public.goals where status = $1";

/// Read-only driver probe against `DATABASE_URL` (prod transaction pooler).
/// Runs SELECTs only and prints "ok <check>" or the failing check and its
/// error; never prints the URL or any row data. Exits non-zero on failure.
#[tokio::main]
async fn main() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL is set");
    let pool = PgPoolOptions::new().max_connections(5).acquire_timeout(Duration::from_secs(20)).connect(&url).await;
    let pool = check("build lazy pool", pool);
    check("count twice on the pool", twice(&pool).await);
    check("count inside begin/commit", in_tx(&pool).await);
    check("uuid/timestamptz/jsonb/text[] row", row_types(&pool).await);
    check("5 clients at once", concurrent(&pool).await);
    pool.close().await;
}

fn check<T>(name: &str, result: Result<T, pg::Error>) -> T {
    match result {
        Ok(v) => {
            println!("ok {name}");
            v
        }
        Err(e) => {
            println!("FAIL {name}: {e}");
            std::process::exit(1);
        }
    }
}

async fn twice(pool: &PgPool) -> Result<(), pg::Error> {
    for _ in 0..2 {
        let _: i64 = pg::query_scalar(COUNT).bind("active").fetch_one(pool).await?;
    }
    Ok(())
}

async fn in_tx(pool: &PgPool) -> Result<(), pg::Error> {
    let mut tx = pool.begin().await?;
    let _: i64 = pg::query_scalar(COUNT).bind("active").fetch_one(&mut *tx).await?;
    let _: i64 = pg::query_scalar(COUNT).bind("done").fetch_one(&mut *tx).await?;
    tx.commit().await
}

async fn row_types(pool: &PgPool) -> Result<(), pg::Error> {
    let row: Option<(Uuid, DateTime<Utc>, Value, Vec<String>)> = pg::query_as(
        "select r.id, r.created_at, c.state, array[r.state, r.workflow] from public.runs r join public.run_checkpoints c on c.run_id = r.id limit 1",
    )
    .fetch_optional(pool)
    .await?;
    println!("  decoded a row: {}", row.is_some());
    let bound: (bool, bool, bool, bool) = pg::query_as("select $1 is not null, $2 is not null, $3 is not null, $4 is not null")
        .bind(Uuid::nil())
        .bind(Utc::now())
        .bind(serde_json::json!({"probe": true}))
        .bind(vec!["a".to_string()])
        .fetch_one(pool)
        .await?;
    assert_eq!(bound, (true, true, true, true), "every bound type arrived");
    Ok(())
}

async fn concurrent(pool: &PgPool) -> Result<(), pg::Error> {
    let tasks: Vec<_> = (0..5)
        .map(|_| {
            let pool = pool.clone();
            tokio::spawn(async move { pg::query_scalar::<i64>(COUNT).bind("active").fetch_one(&pool).await })
        })
        .collect();
    for task in tasks {
        task.await.expect("probe task runs")?;
    }
    Ok(())
}
