use aroh_server::boot::{lazy_pool, MAX_DB_CONNECTIONS};
use aroh_server::db;
use std::time::{Duration, Instant};

#[tokio::test]
async fn b68_a_cold_start_opens_no_connection_even_when_the_database_is_unreachable() {
    let started = Instant::now();
    let pool = lazy_pool("postgres://nobody:nothing@127.0.0.1:1/none").expect("a lazy pool never connects at start");
    assert!(started.elapsed() < Duration::from_secs(1), "building the pool does no network work");
    assert_eq!(MAX_DB_CONNECTIONS, 4);
    let first = db::query("select 1").execute(&pool).await.unwrap_err();
    assert!(!first.to_string().contains("nothing"), "the error never echoes the URL: {first}");
}
