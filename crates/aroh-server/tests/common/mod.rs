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

/// Creates a fresh database cloned from the migrated `aroh_template` over
/// a direct admin connection (advisory lock and `create database` need a
/// session), so each test owns its jobs and rows. Returns its URL through
/// the transaction-mode pooler (`AROH_TEST_POOLER`).
pub async fn pg_url() -> String {
    let base = std::env::var("AROH_TEST_PG").expect("AROH_TEST_PG is set; run crates/aroh-server/tests/run.sh");
    let admin = PgPoolOptions::new().max_connections(1).connect(&format!("{base}/postgres")).await.expect("admin connects");
    let name = format!("t_{}", Uuid::now_v7().simple());
    db::query("select pg_advisory_lock(4242)").execute(&admin).await.expect("lock");
    db::query(&format!("create database {name} template aroh_template")).execute(&admin).await.expect("clone template");
    db::query("select pg_advisory_unlock(4242)").execute(&admin).await.expect("unlock");
    admin.close().await;
    format!("{}/{name}", pooler())
}

/// Opens a pool of 8 connections on a fresh database from [`pg_url`].
pub async fn pg() -> PgPool {
    PgPoolOptions::new().max_connections(8).connect(&pg_url().await).await.expect("test db connects")
}

/// Base URL of the transaction-mode pooler every test pool connects through.
pub fn pooler() -> String {
    std::env::var("AROH_TEST_POOLER").expect("AROH_TEST_POOLER is set; run crates/aroh-server/tests/run.sh")
}

/// Inserts a new `auth.users` row that has allowed AI use, and returns its id.
/// Every synced row references one; runs need the consent.
/// The email is unique per call.
pub async fn user(pool: &PgPool) -> Uuid {
    let id = Uuid::now_v7();
    db::query("insert into auth.users (id, email) values ($1, $2)")
        .bind(id)
        .bind(format!("{id}@test.local"))
        .execute(pool)
        .await
        .expect("user inserted");
    db::query("insert into public.ai_consents (user_id, granted_at) values ($1, now())").bind(id).execute(pool).await.expect("consent inserted");
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

/// The fake card writer's answer to a model request `body`: one full leg-day card
/// for every action id the request's `card_work.slots` names, no outlines.
/// Returns the output JSON as text.
pub fn card_writer_reply(body: &str) -> String {
    let mut ids: Vec<String> = Vec::new();
    for (i, _) in body.match_indices("action_id") {
        let tail: String = body[i + 9..].chars().take(64).collect();
        let start = tail.find(|c: char| c.is_ascii_hexdigit()).unwrap_or(0);
        let id: String = tail[start..].chars().take(36).collect();
        if Uuid::parse_str(&id).is_ok() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    let card = json!({"why": "Legs carry the target.", "blocks": [{"kind": "workout", "title": null, "items": null,
        "exercises": [{"name": "Back squat", "unit": "kg", "rest_s": 120, "cue": null, "sets": [{"reps": 8, "weight": 60}, {"reps": 8, "weight": 60}]}]}]});
    let cards: Vec<_> = ids.iter().map(|id| json!({"action_id": id, "card": card})).collect();
    json!({"cards": cards, "outlines": []}).to_string()
}
