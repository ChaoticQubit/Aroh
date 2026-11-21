#![allow(dead_code)]

use aroh_server::db::{self, PgPool, PgPoolOptions};
use aroh_server::io::{Deps, CATALOG};
use aroh_server::rows::ServerClock;
use aroh_server::vault::{MasterKey, Vault};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use troupekit_core::clock::{Clock, FixedClock};
use troupekit_core::http::{ChunkBody, HttpError, HttpRequest, HttpResponse, Transport};
use troupekit_providers::Catalog;
use uuid::Uuid;

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

/// What a fake transport answers: status, content type and body.
/// Built by the `sse_*` and `json_reply` helpers.
/// The body is sent as one chunk.
pub struct Reply {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

type Handler = dyn Fn(&HttpRequest) -> Reply + Send + Sync;

/// A Troupekit transport that answers every request with a pure function
/// of it and records each request, so tests can check headers (the key
/// reached `x-api-key`) without a network.
pub struct FnTransport {
    handler: Box<Handler>,
    sent: Mutex<Vec<HttpRequest>>,
}

impl FnTransport {
    /// Wraps `handler` as a transport.
    /// Requests are recorded in order.
    /// Returns it in an `Arc`, ready for `Deps`.
    pub fn new(handler: impl Fn(&HttpRequest) -> Reply + Send + Sync + 'static) -> Arc<FnTransport> {
        Arc::new(FnTransport { handler: Box::new(handler), sent: Mutex::new(Vec::new()) })
    }

    /// Returns every request sent so far, oldest first.
    /// Bodies included.
    /// Cloned out of the lock.
    pub fn sent(&self) -> Vec<HttpRequest> {
        self.sent.lock().unwrap().clone()
    }
}

#[async_trait]
impl Transport for FnTransport {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        let reply = (self.handler)(&req);
        self.sent.lock().unwrap().push(req.clone());
        Ok(HttpResponse {
            status: reply.status,
            url: req.url.clone(),
            headers: vec![("content-type".into(), reply.content_type.into())],
            body: Box::new(ChunkBody::new(vec![reply.body.into_bytes()])),
            truncated: false,
        })
    }
}

fn sse(events: Vec<Value>) -> String {
    events
        .into_iter()
        .map(|e| format!("event: {}\ndata: {}\n\n", e["type"].as_str().expect("typed event"), e))
        .collect()
}

fn start_and_stop(blocks: Vec<Value>, stop: &str) -> Reply {
    let mut events = vec![json!({"type": "message_start", "message": {"id": "msg_1", "type": "message", "role": "assistant", "content": [], "usage": {"input_tokens": 100, "output_tokens": 1}}})];
    events.extend(blocks);
    events.push(json!({"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": null}, "usage": {"output_tokens": 50}}));
    events.push(json!({"type": "message_stop"}));
    Reply { status: 200, content_type: "text/event-stream", body: sse(events) }
}

/// An Anthropic streaming reply whose text is `text`: the final answer
/// of an agent turn (structured output is JSON text).
/// Usage is 100 input and 50 output tokens.
pub fn sse_text(text: &str) -> Reply {
    start_and_stop(
        vec![
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": text}}),
            json!({"type": "content_block_stop", "index": 0}),
        ],
        "end_turn",
    )
}

/// An Anthropic streaming reply that calls tool `name` with `input`.
/// The call id is `toolu_1`.
/// Stops with `tool_use`.
pub fn sse_tool(name: &str, input: Value) -> Reply {
    start_and_stop(
        vec![
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": "toolu_1", "name": name, "input": {}}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": input.to_string()}}),
            json!({"type": "content_block_stop", "index": 0}),
        ],
        "tool_use",
    )
}

/// A plain JSON reply with `status`.
/// Used for Anthropic non-streaming calls and Brave search.
/// Content type `application/json`.
pub fn json_reply(status: u16, body: Value) -> Reply {
    Reply { status, content_type: "application/json", body: body.to_string() }
}

/// Returns the request body as text, empty for a bodiless request.
/// Lossy for non-UTF-8 bodies.
/// Used to route fake replies by persona.
pub fn body_text(req: &HttpRequest) -> String {
    req.body.as_deref().map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default()
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
