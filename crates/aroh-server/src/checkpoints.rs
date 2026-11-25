use crate::db;
use crate::error::store_err;
use crate::vault::Vault;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use async_trait::async_trait;
use serde_json::Value;
use crate::db::PgPool;
use std::sync::{Arc, Mutex};
use troupekit_agent::EventSink;
use troupekit_core::error::CoreError;
use troupekit_core::event::Event;
use troupekit_core::ids::RunId;
use troupekit_core::store::{Checkpoint, CheckpointStore};
use uuid::Uuid;

/// Collects the events of one step in memory. `emit` is synchronous, so a
/// step's events wait here and `PgCheckpoints::save` writes them with the
/// checkpoint; a step that is lost or fails never reaches the table.
#[derive(Debug, Default)]
pub struct EventBuffer(Mutex<Vec<Event>>);

impl EventBuffer {
    /// Removes and returns every buffered event, oldest first.
    /// The buffer is empty afterwards.
    /// Returns an empty list when nothing was emitted.
    pub fn take(&self) -> Vec<Event> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

impl EventSink for EventBuffer {
    fn emit(&self, event: Event) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).push(event);
    }
}

/// Reads the next event `seq` and the lifecycle status a graph checkpoint
/// holds (`state.seq`, `state.status.status`). Returns `InvalidMessage`
/// when the state is not a graph run of the expected shape.
pub fn checkpoint_header(state: &Value) -> Result<(u64, &str), CoreError> {
    let seq = state.get("seq").and_then(Value::as_u64);
    let status = state.get("status").and_then(|s| s.get("status")).and_then(Value::as_str);
    match (seq, status) {
        (Some(seq), Some(status)) => Ok((seq, status)),
        _ => Err(CoreError::InvalidMessage("checkpoint state has no seq or status".into())),
    }
}

fn db_int(n: u64) -> i64 {
    i64::try_from(n).expect("step and seq fit in bigint")
}

/// `CheckpointStore` on `run_checkpoints`, one instance per lease. `save`
/// runs one transaction: the step + 1 rule under a lock on the `runs` row,
/// the buffered events upserted on `(run_id, seq)`, the stale tail dropped.
/// A run marked `sealed` (it read the diary) stores its state and event
/// payloads sealed by the user's data key; `latest` and `events_after` open them.
pub struct PgCheckpoints {
    pool: PgPool,
    vault: Vault,
    events: Arc<EventBuffer>,
}

impl PgCheckpoints {
    /// Builds a store over `pool` that writes `events` with each save and
    /// seals and opens diary runs with `vault`. Give each lease its own buffer.
    /// Returns the store.
    pub fn new(pool: PgPool, vault: Vault, events: Arc<EventBuffer>) -> PgCheckpoints {
        PgCheckpoints { pool, vault, events }
    }
}

fn checkpoint_aad(user: Uuid, run: Uuid, step: u64) -> Vec<u8> {
    format!("aroh/checkpoint/v1/{user}/{run}/{step}").into_bytes()
}

fn event_aad(user: Uuid, run: Uuid, seq: u64) -> Vec<u8> {
    format!("aroh/run-event/v1/{user}/{run}/{seq}").into_bytes()
}

fn crypto(e: crate::error::ServerError) -> CoreError {
    CoreError::Store(format!("sealed run: {e}"))
}

/// Whether run `run` is sealed. False for a missing run.
pub async fn is_sealed(pool: &PgPool, run: RunId) -> Result<bool, crate::error::ServerError> {
    let sealed: Option<bool> = db::query_scalar("select sealed from public.runs where id = $1").bind(run.0).fetch_optional(pool).await?;
    Ok(sealed.unwrap_or(false))
}

/// Seals `value` for `user` with `aad` into `{sealed, nonce}` base64 fields.
async fn seal_value(vault: &Vault, user: Uuid, aad: &[u8], value: &Value) -> Result<serde_json::Map<String, Value>, CoreError> {
    let plain = zeroize::Zeroizing::new(serde_json::to_vec(value).expect("json values serialize"));
    let (ct, nonce) = vault.seal_for(user, aad, &plain).await.map_err(crypto)?;
    let mut out = serde_json::Map::new();
    out.insert("sealed".into(), Value::String(B64.encode(ct)));
    out.insert("nonce".into(), Value::String(B64.encode(nonce)));
    Ok(out)
}

/// Opens `stored` when it holds `{sealed, nonce}`, else returns it as is.
async fn open_value(vault: &Vault, user: Uuid, aad: &[u8], stored: Value) -> Result<Value, CoreError> {
    let (Some(sealed), Some(nonce)) = (stored.get("sealed").and_then(Value::as_str), stored.get("nonce").and_then(Value::as_str)) else {
        return Ok(stored);
    };
    let ct = B64.decode(sealed).map_err(|_| CoreError::Store("sealed run: bad base64".into()))?;
    let nonce = B64.decode(nonce).map_err(|_| CoreError::Store("sealed run: bad base64".into()))?;
    let plain = vault.open_for(user, aad, &nonce, &ct).await.map_err(crypto)?;
    serde_json::from_slice(&plain).map_err(|_| CoreError::Store("sealed run: not json".into()))
}

#[async_trait]
impl CheckpointStore for PgCheckpoints {
    async fn save(&self, cp: Checkpoint) -> Result<(), CoreError> {
        let (next_seq, status) = checkpoint_header(&cp.state)?;
        let owner: Option<(Uuid, bool)> = db::query_as("select user_id, sealed from public.runs where id = $1")
            .bind(cp.run_id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(store_err)?;
        let Some((owner, sealed)) = owner else {
            return Err(CoreError::NotFound(format!("run {}", cp.run_id)));
        };
        let state = if sealed {
            let mut stored = seal_value(&self.vault, owner, &checkpoint_aad(owner, cp.run_id.0, cp.step), &cp.state).await?;
            stored.insert("seq".into(), Value::from(next_seq));
            stored.insert("status".into(), serde_json::json!({ "status": status }));
            Value::Object(stored)
        } else {
            cp.state.clone()
        };
        let mut events = Vec::new();
        for event in self.events.take() {
            assert_eq!(event.run_id, cp.run_id, "a lease buffers one run's events");
            assert!(event.seq < next_seq, "a step's events precede the run's next seq");
            let payload = serde_json::to_value(&event).expect("events serialize");
            let kind = payload["body"]["type"].as_str().expect("event bodies are tagged").to_string();
            let payload = if sealed { Value::Object(seal_value(&self.vault, owner, &event_aad(owner, cp.run_id.0, event.seq), &payload).await?) } else { payload };
            events.push((event.seq, kind, payload));
        }
        let mut tx = self.pool.begin().await.map_err(store_err)?;
        let user: Option<Uuid> = db::query_scalar("select user_id from public.runs where id = $1 for update")
            .bind(cp.run_id.0)
            .fetch_optional(&mut *tx)
            .await
            .map_err(store_err)?;
        let Some(user) = user else {
            return Err(CoreError::NotFound(format!("run {}", cp.run_id)));
        };
        assert_eq!(user, owner, "a run never changes owner");
        let max: Option<i64> = db::query_scalar("select max(step) from public.run_checkpoints where run_id = $1")
            .bind(cp.run_id.0)
            .fetch_one(&mut *tx)
            .await
            .map_err(store_err)?;
        let expected = max.map_or(0, |m| u64::try_from(m).expect("steps are non-negative") + 1);
        if cp.step != expected {
            return Err(CoreError::Conflict { expected, actual: cp.step });
        }
        db::query("insert into public.run_checkpoints (user_id, run_id, step, state) values ($1, $2, $3, $4)")
            .bind(user)
            .bind(cp.run_id.0)
            .bind(db_int(cp.step))
            .bind(&state)
            .execute(&mut *tx)
            .await
            .map_err(store_err)?;
        for (seq, kind, payload) in events {
            db::query(
                "insert into public.run_events (user_id, run_id, seq, kind, payload) values ($1, $2, $3, $4, $5)
                 on conflict (run_id, seq) do update set kind = excluded.kind, payload = excluded.payload, created_at = now()",
            )
            .bind(user)
            .bind(cp.run_id.0)
            .bind(db_int(seq))
            .bind(kind)
            .bind(payload)
            .execute(&mut *tx)
            .await
            .map_err(store_err)?;
        }
        db::query("delete from public.run_events where run_id = $1 and seq >= $2")
            .bind(cp.run_id.0)
            .bind(db_int(next_seq))
            .execute(&mut *tx)
            .await
            .map_err(store_err)?;
        db::query("update public.runs set state = $2, updated_at = now() where id = $1")
            .bind(cp.run_id.0)
            .bind(status)
            .execute(&mut *tx)
            .await
            .map_err(store_err)?;
        tx.commit().await.map_err(store_err)
    }

    async fn latest(&self, run_id: RunId) -> Result<Option<Checkpoint>, CoreError> {
        let row: Option<(Uuid, i64, Value)> = db::query_as(
            "select user_id, step, state from public.run_checkpoints where run_id = $1 order by step desc limit 1",
        )
        .bind(run_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_err)?;
        let Some((user, step, state)) = row else {
            return Ok(None);
        };
        let step = u64::try_from(step).expect("steps are non-negative");
        let state = open_value(&self.vault, user, &checkpoint_aad(user, run_id.0, step), state).await?;
        Ok(Some(Checkpoint { run_id, step, state }))
    }
}

/// Reads the stored events of `run_id` with `seq > after`, in `seq` order,
/// opening sealed payloads with `vault`. Callers check run ownership first.
/// Returns the events.
pub async fn events_after(pool: &PgPool, vault: &Vault, run_id: RunId, after: Option<u64>) -> Result<Vec<Event>, CoreError> {
    let floor = after.map_or(-1, db_int);
    let rows: Vec<(Uuid, i64, Value)> =
        db::query_as("select user_id, seq, payload from public.run_events where run_id = $1 and seq > $2 order by seq")
            .bind(run_id.0)
            .bind(floor)
            .fetch_all(pool)
            .await
            .map_err(store_err)?;
    let mut out = Vec::with_capacity(rows.len());
    for (user, seq, payload) in rows {
        let seq = u64::try_from(seq).expect("seqs are non-negative");
        let payload = open_value(vault, user, &event_aad(user, run_id.0, seq), payload).await?;
        out.push(serde_json::from_value(payload).map_err(|e| CoreError::Store(format!("run_events: {e}")))?);
    }
    Ok(out)
}
