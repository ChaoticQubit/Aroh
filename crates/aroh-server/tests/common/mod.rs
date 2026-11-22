#![allow(dead_code)]

use aroh_server::db::{self, PgPool, PgPoolOptions};
use aroh_server::io::{Deps, CATALOG};
use aroh_server::rows::ServerClock;
use aroh_server::vault::{MasterKey, Vault};
use serde_json::json;
use std::sync::Arc;
use troupekit_core::clock::{Clock, FixedClock};
use troupekit_core::http::Transport;
use troupekit_providers::Catalog;
use uuid::Uuid;

use aroh_server::fake::{json_reply, FnTransport};

/// Master key of every test vault.
/// 32 bytes of 0x07, base64.
/// Never a production value.
pub const MASTER: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=";

/// The fixed start time of every DB test clock.
/// 2026-11-22T12:00:00Z, a Sunday.
/// Milliseconds since the Unix epoch.
pub const T0: i64 = 1_795_348_800_000;

/// Opens a fresh database cloned from the migrated `aroh_template`, so
/// each test owns its jobs and rows. Clones are serialized with an
/// advisory lock because Postgres rejects concurrent template copies.
pub async fn pg() -> PgPool {
    let base = std::env::var("AROH_TEST_PG").expect("AROH_TEST_PG is set; run crates/aroh-server/tests/run.sh");
    let admin = PgPoolOptions::new().max_connections(1).connect(&format!("{base}/postgres")).await.expect("admin connects");
    let name = format!("t_{}", Uuid::now_v7().simple());
    db::query("select pg_advisory_lock(4242)").execute(&admin).await.expect("lock");
    db::query(&format!("create database {name} template aroh_template")).execute(&admin).await.expect("clone template");
    db::query("select pg_advisory_unlock(4242)").execute(&admin).await.expect("unlock");
    admin.close().await;
    PgPoolOptions::new().max_connections(8).connect(&format!("{base}/{name}")).await.expect("test db connects")
}

/// Inserts a new `auth.users` row and returns its id.
/// Every synced row references one.
/// The email is unique per call.
pub async fn user(pool: &PgPool) -> Uuid {
    let id = Uuid::now_v7();
    db::query("insert into auth.users (id, email) values ($1, $2)")
        .bind(id)
        .bind(format!("{id}@test.local"))
        .execute(pool)
        .await
        .expect("user inserted");
    id
}

/// Builds engine deps over `pool` with fake `models` and `web`
/// transports, a fixed clock at `T0` and the test master key.
/// Returns the deps and the clock, which tests advance.
pub fn deps(pool: &PgPool, models: Arc<dyn Transport>, web: Arc<dyn Transport>) -> (Arc<Deps>, Arc<FixedClock>) {
    let clock = Arc::new(FixedClock::new(T0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let deps = Deps {
        pool: pool.clone(),
        vault: Vault::new(pool.clone(), Arc::new(MasterKey::from_base64(MASTER).expect("test key"))),
        catalog: Catalog::load(CATALOG).expect("catalog loads"),
        models,
        web,
        clock: dyn_clock,
        hlc: Arc::new(ServerClock::default()),
        allow_custom_base_url: false,
        search_toml: None,
    };
    (Arc::new(deps), clock)
}

/// A transport that fails every request, for paths that must not call out.
/// Answers 599 with an empty JSON body.
/// Records requests like any `FnTransport`.
pub fn offline() -> Arc<FnTransport> {
    FnTransport::new(|_| json_reply(599, json!({})))
}
