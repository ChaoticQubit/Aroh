use crate::db;
use crate::error::store_err;
use crate::rows::utc;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crate::db::PgPool;
use std::sync::Arc;
use troupekit_core::clock::Clock;
use troupekit_core::error::CoreError;
use troupekit_core::ids::RunId;
use troupekit_host::{JobQueue, Lease};
use uuid::Uuid;

const MAX_ERROR_CHARS: usize = 1000;

/// `JobQueue` on the `jobs` table: one job per run (`jobs.run_id` unique),
/// claims with `FOR UPDATE SKIP LOCKED` and leases with a sequence token.
/// Times come from the injected clock, never from the database.
pub struct PgQueue {
    pool: PgPool,
    clock: Arc<dyn Clock>,
}

impl PgQueue {
    /// Builds a queue over `pool` reading time from `clock`.
    /// Share the clock with the engine's run io.
    /// Returns the queue.
    pub fn new(pool: PgPool, clock: Arc<dyn Clock>) -> PgQueue {
        PgQueue { pool, clock }
    }

    /// Returns the status of `run_id`'s job (`queued`, `running`, `done` or
    /// `failed`) and its stored error, for status views and tests.
    /// `None` when the run was never enqueued.
    pub async fn status(&self, run_id: RunId) -> Result<Option<(String, Option<String>)>, CoreError> {
        db::query_as("select status, error from public.jobs where run_id = $1")
            .bind(run_id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(store_err)
    }

    async fn stale(&self, lease: &Lease) -> CoreError {
        let token: Result<Option<Option<i64>>, db::Error> =
            db::query_scalar("select lease_token from public.jobs where run_id = $1")
                .bind(lease.run_id.0)
                .fetch_optional(&self.pool)
                .await;
        match token {
            Ok(None) => CoreError::NotFound(format!("job for run {}", lease.run_id)),
            Ok(Some(t)) => CoreError::Conflict {
                expected: t.and_then(|t| u64::try_from(t).ok()).unwrap_or(0),
                actual: lease.token,
            },
            Err(e) => store_err(e),
        }
    }

    async fn finish(&self, lease: &Lease, done: Result<db::PgQueryResult, db::Error>) -> Result<(), CoreError> {
        let done = done.map_err(store_err)?;
        assert!(done.rows_affected() <= 1, "one job per run");
        if done.rows_affected() == 0 {
            return Err(self.stale(lease).await);
        }
        Ok(())
    }
}

fn token(lease: &Lease) -> i64 {
    i64::try_from(lease.token).unwrap_or(-1)
}

const END: &str = "lease_token = null, locked_until = null, attempts = 0, updated_at = now()";

#[async_trait]
impl JobQueue for PgQueue {
    async fn enqueue(&self, run_id: RunId, due_ms: i64) -> Result<(), CoreError> {
        let done = db::query(
            "insert into public.jobs (user_id, kind, run_id, status, run_after)
             select r.user_id, 'graph', r.id, 'queued', $2 from public.runs r where r.id = $1
             on conflict (run_id) do update set
               run_after = case when jobs.status = 'queued' then least(jobs.run_after, excluded.run_after)
                                when jobs.status = 'running' then jobs.run_after
                                else excluded.run_after end,
               next_run_after = case when jobs.status = 'running'
                                     then least(coalesce(jobs.next_run_after, excluded.run_after), excluded.run_after)
                                     else null end,
               error = case when jobs.status = 'running' then jobs.error else null end,
               status = case when jobs.status = 'running' then 'running' else 'queued' end,
               updated_at = now()",
        )
        .bind(run_id.0)
        .bind(utc(due_ms))
        .execute(&self.pool)
        .await
        .map_err(store_err)?;
        if done.rows_affected() == 0 {
            return Err(CoreError::NotFound(format!("run {run_id}")));
        }
        Ok(())
    }

    async fn claim_due(&self, limit: usize, lease_ms: i64) -> Result<Vec<Lease>, CoreError> {
        assert!(lease_ms > 0, "a lease lasts some time");
        if limit == 0 {
            return Ok(Vec::new());
        }
        let now = self.clock.now_ms();
        let rows: Vec<(Uuid, i64, i32, DateTime<Utc>)> = db::query_as(
            "with due as (
               select id, case when status = 'queued' then run_after else locked_until end as due_at
               from public.jobs
               where run_id is not null
                 and ((status = 'queued' and run_after <= $1) or (status = 'running' and locked_until <= $1))
               order by due_at, id
               limit $2
               for update skip locked)
             update public.jobs j set status = 'running', lease_token = nextval('public.aroh_lease_seq'),
               locked_until = $3, attempts = j.attempts + 1, updated_at = now()
             from due where j.id = due.id
             returning j.run_id, j.lease_token, j.attempts, due.due_at",
        )
        .bind(utc(now))
        .bind(i64::try_from(limit).expect("limit fits"))
        .bind(utc(now + lease_ms))
        .fetch_all(&self.pool)
        .await
        .map_err(store_err)?;
        let mut rows = rows;
        rows.sort_by_key(|r| (r.3, r.0));
        assert!(rows.len() <= limit);
        Ok(rows
            .into_iter()
            .map(|(run_id, token, attempts, _)| Lease {
                run_id: RunId(run_id),
                token: u64::try_from(token).expect("lease tokens are positive"),
                attempts: u32::try_from(attempts).expect("attempts are non-negative"),
            })
            .collect())
    }

    async fn complete(&self, lease: &Lease) -> Result<(), CoreError> {
        let sql = format!(
            "update public.jobs set status = case when next_run_after is null then 'done' else 'queued' end,
               run_after = coalesce(next_run_after, run_after), next_run_after = null, {END}
             where run_id = $1 and status = 'running' and lease_token = $2"
        );
        let done = db::query(&sql).bind(lease.run_id.0).bind(token(lease)).execute(&self.pool).await;
        self.finish(lease, done).await
    }

    async fn fail(&self, lease: &Lease, error: &str) -> Result<(), CoreError> {
        let sql = format!(
            "update public.jobs set status = 'failed', error = $3, next_run_after = null, {END}
             where run_id = $1 and status = 'running' and lease_token = $2"
        );
        let error: String = error.chars().take(MAX_ERROR_CHARS).collect();
        let done = db::query(&sql).bind(lease.run_id.0).bind(token(lease)).bind(error).execute(&self.pool).await;
        self.finish(lease, done).await
    }

    async fn reschedule(&self, lease: &Lease, due_ms: i64) -> Result<(), CoreError> {
        let sql = format!(
            "update public.jobs set status = 'queued', run_after = least(coalesce(next_run_after, $3), $3),
               next_run_after = null, {END}
             where run_id = $1 and status = 'running' and lease_token = $2"
        );
        let done = db::query(&sql).bind(lease.run_id.0).bind(token(lease)).bind(utc(due_ms)).execute(&self.pool).await;
        self.finish(lease, done).await
    }
}
