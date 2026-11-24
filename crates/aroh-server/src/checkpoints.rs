use crate::db;
use crate::error::store_err;
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
pub struct PgCheckpoints {
    pool: PgPool,
    events: Arc<EventBuffer>,
}

impl PgCheckpoints {
    /// Builds a store over `pool` that writes `events` with each save.
    /// Give each lease its own buffer.
    /// Returns the store.
    pub fn new(pool: PgPool, events: Arc<EventBuffer>) -> PgCheckpoints {
        PgCheckpoints { pool, events }
    }
}

#[async_trait]
impl CheckpointStore for PgCheckpoints {
    async fn save(&self, cp: Checkpoint) -> Result<(), CoreError> {
        let (next_seq, status) = checkpoint_header(&cp.state)?;
        let mut tx = self.pool.begin().await.map_err(store_err)?;
        let user: Option<Uuid> = db::query_scalar("select user_id from public.runs where id = $1 for update")
            .bind(cp.run_id.0)
            .fetch_optional(&mut *tx)
            .await
            .map_err(store_err)?;
        let Some(user) = user else {
            return Err(CoreError::NotFound(format!("run {}", cp.run_id)));
        };
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
            .bind(&cp.state)
            .execute(&mut *tx)
            .await
            .map_err(store_err)?;
        for event in self.events.take() {
            assert_eq!(event.run_id, cp.run_id, "a lease buffers one run's events");
            assert!(event.seq < next_seq, "a step's events precede the run's next seq");
            let payload = serde_json::to_value(&event).expect("events serialize");
            let kind = payload["body"]["type"].as_str().expect("event bodies are tagged").to_string();
            db::query(
                "insert into public.run_events (user_id, run_id, seq, kind, payload) values ($1, $2, $3, $4, $5)
                 on conflict (run_id, seq) do update set kind = excluded.kind, payload = excluded.payload, created_at = now()",
            )
            .bind(user)
            .bind(cp.run_id.0)
            .bind(db_int(event.seq))
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
        let row: Option<(i64, Value)> = db::query_as(
            "select step, state from public.run_checkpoints where run_id = $1 order by step desc limit 1",
        )
        .bind(run_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_err)?;
        Ok(row.map(|(step, state)| Checkpoint {
            run_id,
            step: u64::try_from(step).expect("steps are non-negative"),
            state,
        }))
    }
}

/// Reads the stored events of `run_id` with `seq > after`, in `seq` order.
/// Callers check run ownership first.
/// Returns the events as stored.
pub async fn events_after(pool: &PgPool, run_id: RunId, after: Option<u64>) -> Result<Vec<Event>, CoreError> {
    let floor = after.map_or(-1, db_int);
    let rows: Vec<Value> =
        db::query_scalar("select payload from public.run_events where run_id = $1 and seq > $2 order by seq")
            .bind(run_id.0)
            .bind(floor)
            .fetch_all(pool)
            .await
            .map_err(store_err)?;
    rows.into_iter()
        .map(|v| serde_json::from_value(v).map_err(|e| CoreError::Store(format!("run_events: {e}"))))
        .collect()
}
