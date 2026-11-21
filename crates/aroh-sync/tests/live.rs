use aroh_sync::auth::password_session;
use aroh_sync::config::{validate_url, Config};
use aroh_sync::remote::sync_all;
use aroh_sync::store::TABLES;
use aroh_sync::remote::Moved;
use rusqlite::{Connection, OptionalExtension};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

async fn sync(conn: &mut Connection, config: &Config, token: &str) -> Moved {
    sync_all(conn, config, token, &mut |_| {}).await.unwrap()
}

fn now_millis() -> u64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
    u64::try_from(since.as_millis()).unwrap()
}

fn hlc(millis: u64, node: u64) -> String {
    format!("{millis:013}-0000-{node:016x}")
}

fn device() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    for table in &TABLES {
        let sql = format!(
            "CREATE TABLE {} ({}, PRIMARY KEY ({}))",
            table.name,
            table.columns.join(", "),
            table.pk
        );
        conn.execute_batch(&sql).unwrap();
    }
    conn
}

fn goal(conn: &Connection, id: &str) -> Option<(String, i64, String)> {
    conn.query_row("SELECT title, deleted, updated_at FROM goals WHERE id = ?1", [id], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    })
    .optional()
    .unwrap()
}

fn action_title(conn: &Connection, id: &str) -> Option<String> {
    conn.query_row("SELECT title FROM actions WHERE id = ?1", [id], |r| r.get(0))
        .optional()
        .unwrap()
}

/// Two devices signed in as one Supabase user converge through the live
/// project: insert, newer edit, tombstone, and an older edit that must not
/// resurrect the tombstone. Run with `live.sh`, never in the default suite.
#[tokio::test]
#[ignore = "needs a live Supabase project; run crates/aroh-sync/live.sh"]
async fn two_devices_converge_through_supabase() {
    let config = Config { url: validate_url(&env("AROH_SUPABASE_URL")).unwrap(), anon_key: env("AROH_SUPABASE_ANON_KEY") };
    let session = password_session(&config, &env("AROH_TEST_EMAIL"), &env("AROH_TEST_PASSWORD")).await.unwrap();
    let token = session.access_token.as_str();
    let (mut a, mut b) = (device(), device());
    let (node_a, node_b) = (0xa, 0xb);
    let run = now_millis();
    let goal_id = format!("live-goal-{run}");
    let action_id = format!("live-action-{run}");
    let t1 = hlc(run, node_a);

    a.execute(
        "INSERT INTO goals (id, title, kind, status, created_at, updated_at, deleted) VALUES (?1, 'Live goal', 'custom', 'active', '2026-11-20T00:00:00Z', ?2, 0)",
        [&goal_id, &t1],
    )
    .unwrap();
    a.execute(
        "INSERT INTO actions (id, goal_id, title, status, est_minutes, seq, updated_at, deleted) VALUES (?1, ?2, 'Read chapter 1', 'planned', 30, 1, ?3, 0)",
        [&action_id, &goal_id, &t1],
    )
    .unwrap();
    assert!(sync(&mut a, &config, token).await.pushed >= 2);
    sync(&mut b, &config, token).await;
    assert_eq!(goal(&b, &goal_id).map(|g| g.0).as_deref(), Some("Live goal"));
    assert_eq!(action_title(&b, &action_id).as_deref(), Some("Read chapter 1"));

    let t2 = hlc(run + 1, node_b);
    b.execute("UPDATE actions SET title = 'Read chapter 1 and 2', updated_at = ?2 WHERE id = ?1", [&action_id, &t2]).unwrap();
    sync(&mut b, &config, token).await;
    sync(&mut a, &config, token).await;
    assert_eq!(action_title(&a, &action_id).as_deref(), Some("Read chapter 1 and 2"));

    let t3 = hlc(run + 3, node_a);
    a.execute("UPDATE goals SET deleted = 1, updated_at = ?2 WHERE id = ?1", [&goal_id, &t3]).unwrap();
    sync(&mut a, &config, token).await;
    sync(&mut b, &config, token).await;
    assert_eq!(goal(&b, &goal_id).map(|g| g.1), Some(1));

    let stale = hlc(run + 2, node_b);
    b.execute("UPDATE goals SET deleted = 0, title = 'resurrected', updated_at = ?2 WHERE id = ?1", [&goal_id, &stale]).unwrap();
    sync(&mut b, &config, token).await;
    sync(&mut a, &config, token).await;
    let mut fresh = device();
    sync(&mut fresh, &config, token).await;
    for conn in [&a, &fresh] {
        assert_eq!(goal(conn, &goal_id), Some(("Live goal".to_string(), 1, t3.clone())));
    }
}
