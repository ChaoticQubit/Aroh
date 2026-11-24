mod common;

use aroh_server::db::{self, PgPool};
use common::{pg, user};
use std::time::Duration;
use uuid::Uuid;

const PULL: &str = "select id from public.preferences where user_id = $1 and server_seq > $2 order by server_seq";

async fn write<'e>(conn: impl db::Executor<'e>, owner: Uuid, id: Uuid, stamp: &str) {
    db::query("insert into public.preferences (id, user_id, fact, source, noted_at, confirmed, updated_at) values ($1, $2, 'f', 'owner', now(), false, $3)")
        .bind(id)
        .bind(owner)
        .bind(stamp)
        .execute(conn)
        .await
        .expect("row written");
}

async fn pull(pool: &PgPool, owner: Uuid, after: i64) -> Vec<(Uuid, i64)> {
    db::query_as::<(Uuid, i64)>(&PULL.replace("select id", "select id, server_seq")).bind(owner).bind(after).fetch_all(pool).await.expect("pull")
}

/// B77 lead hypothesis: a row whose transaction took a lower `server_seq` but commits
/// after a higher one was pulled would be skipped by the device cursor. A writer of
/// the same user holds one transaction open while a second writes; a device pulls in
/// between and again after both commit. Every row must reach the device.
#[tokio::test]
#[ignore = "needs Postgres; run crates/aroh-server/tests/run.sh"]
async fn b77_a_pull_never_skips_a_row_that_commits_after_a_later_seq() {
    let pool = pg().await;
    let owner = user(&pool).await;
    let (slow, fast) = (Uuid::now_v7(), Uuid::now_v7());
    let mut open = pool.begin().await.unwrap();
    write(&mut *open, owner, slow, "1795000000000-0000-00000000000000d1").await;
    let racer = {
        let pool = pool.clone();
        tokio::spawn(async move {
            write(&pool, owner, fast, "1795000000001-0000-00000000000000e5").await;
        })
    };
    tokio::time::sleep(Duration::from_millis(500)).await;
    let first = pull(&pool, owner, 0).await;
    let cursor = first.iter().map(|r| r.1).max().unwrap_or(0);
    open.commit().await.unwrap();
    racer.await.unwrap();
    let second = pull(&pool, owner, cursor).await;
    let seen: Vec<Uuid> = first.iter().chain(second.iter()).map(|r| r.0).collect();
    assert!(seen.contains(&slow), "the row of the transaction that committed last was skipped: first {first:?} second {second:?}");
    assert!(seen.contains(&fast));
    assert_eq!(seen.len(), 2);
}
