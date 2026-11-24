use crate::db;
use crate::error::store_err;
use crate::rows::{put_row, utc, ServerClock};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, TimeZone, Utc};
use crate::db::PgPool;
use std::sync::Arc;
use troupekit_agent::UsageLedger;
use troupekit_core::clock::Clock;
use troupekit_core::error::{CoreError, ProviderError, ProviderErrorKind};
use troupekit_core::event::Usage;
use troupekit_core::ids::RunId;
use troupekit_providers::{CompletionRequest, CompletionResponse, Delta, Model};
use uuid::Uuid;

/// Ledger key of a goal's monthly spend, used as the run's `scope` limit.
/// Returns `goal:<uuid>`.
/// `PgLedger` parses it back.
pub fn goal_key(goal: Uuid) -> String {
    format!("goal:{goal}")
}

/// Ledger key of a user's monthly spend, used as the run's `user` limit.
/// Returns `user:<uuid>`.
/// `PgLedger` parses it back.
pub fn user_key(user: Uuid) -> String {
    format!("user:{user}")
}

/// Returns the first instant of the UTC calendar month holding `now` and of
/// the next month: the period every ledger total covers.
/// Pure; the clock supplies `now`.
pub fn month_bounds(now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0).single().expect("first of month exists");
    let (y, m) = if now.month() == 12 { (now.year() + 1, 1) } else { (now.year(), now.month() + 1) };
    let end = Utc.with_ymd_and_hms(y, m, 1, 0, 0, 0).single().expect("first of month exists");
    assert!(start <= now && now < end);
    (start, end)
}

/// `UsageLedger` over the `usage` table for one user. `period_total` sums
/// the calendar month (UTC) by goal or by user. `record` writes nothing:
/// `UsageModel` already wrote the call's single row with vendor and model.
pub struct PgLedger {
    pool: PgPool,
    user: Uuid,
    clock: Arc<dyn Clock>,
}

impl PgLedger {
    /// Builds the ledger of `user` over `pool`, months from `clock`.
    /// One per run io.
    /// Returns the ledger.
    pub fn new(pool: PgPool, user: Uuid, clock: Arc<dyn Clock>) -> PgLedger {
        PgLedger { pool, user, clock }
    }
}

#[async_trait]
impl UsageLedger for PgLedger {
    async fn period_total(&self, key: &str) -> Result<u64, CoreError> {
        let bad = || CoreError::InvalidMessage(format!("unknown ledger key `{key}`"));
        let (kind, id) = key.split_once(':').ok_or_else(bad)?;
        let id = Uuid::parse_str(id).map_err(|_| bad())?;
        let (start, end) = month_bounds(utc(self.clock.now_ms()));
        let base = "select coalesce(sum(cost_usd_micros), 0)::bigint from public.usage
                    where user_id = $1 and not deleted and occurred_at >= $2 and occurred_at < $3";
        let total: i64 = match kind {
            "user" if id == self.user => db::query_scalar(base).bind(self.user).bind(start).bind(end).fetch_one(&self.pool).await,
            "goal" => db::query_scalar(&format!("{base} and goal_id = $4"))
                .bind(self.user)
                .bind(start)
                .bind(end)
                .bind(id)
                .fetch_one(&self.pool)
                .await,
            _ => return Err(bad()),
        }
        .map_err(store_err)?;
        Ok(u64::try_from(total).expect("usage sums are non-negative"))
    }

    async fn record(&self, _key: &str, _usage: Usage) -> Result<(), CoreError> {
        Ok(())
    }
}

/// Where `UsageModel` writes: the user, goal and run a call is charged to,
/// plus the pool, the server HLC and the clock for `occurred_at`.
/// Built once per run io and shared by its models.
pub struct UsageWriter {
    pub pool: PgPool,
    pub hlc: Arc<ServerClock>,
    pub clock: Arc<dyn Clock>,
    pub user: Uuid,
    pub goal: Option<Uuid>,
    pub run: Option<RunId>,
}

impl UsageWriter {
    async fn write(&self, vendor: &str, model: &str, usage: Usage) -> Result<(), ProviderError> {
        let int = |n: u64| i64::try_from(n).expect("token counts fit in bigint");
        let mut row = aroh_domain::rows::Usage {
            id: aroh_domain::new_id(),
            user_id: self.user.to_string(),
            goal_id: self.goal.map(|g| g.to_string()),
            run_id: self.run.map(|r| r.to_string()),
            vendor: vendor.to_string(),
            model: model.to_string(),
            input_tokens: int(usage.input_tokens),
            output_tokens: int(usage.output_tokens),
            cost_usd_micros: int(usage.cost_usd_micros),
            occurred_at: utc(self.clock.now_ms()),
            updated_at: String::new(),
            deleted: false,
        };
        put_row(&self.pool, &self.hlc, self.user, &mut row)
            .await
            .map_err(|e| ProviderError::new(ProviderErrorKind::Server, format!("usage ledger: {e}")))
    }
}

/// A model that writes one `usage` row per successful call (vendor, model,
/// tokens, cost), after the cost meter priced the reply. A failed write is
/// a retryable error, so spend is never silently dropped.
pub struct UsageModel {
    inner: Box<dyn Model>,
    vendor: String,
    model: String,
    writer: Arc<UsageWriter>,
}

impl UsageModel {
    /// Wraps the metered `inner` serving `vendor` / `model`, writing through `writer`.
    /// The id stays the inner model's.
    /// Returns the model.
    pub fn new(inner: Box<dyn Model>, vendor: &str, model: &str, writer: Arc<UsageWriter>) -> UsageModel {
        UsageModel { inner, vendor: vendor.to_string(), model: model.to_string(), writer }
    }
}

#[async_trait]
impl Model for UsageModel {
    fn id(&self) -> &str {
        self.inner.id()
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let reply = self.inner.complete(req).await?;
        self.writer.write(&self.vendor, &self.model, reply.usage).await?;
        Ok(reply)
    }

    async fn stream(
        &self,
        req: &CompletionRequest,
        on_delta: &mut (dyn for<'d> FnMut(&'d Delta) + Send),
    ) -> Result<CompletionResponse, ProviderError> {
        let reply = self.inner.stream(req, on_delta).await?;
        self.writer.write(&self.vendor, &self.model, reply.usage).await?;
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_bounds_cover_the_calendar_month() {
        let at = Utc.with_ymd_and_hms(2026, 12, 31, 23, 59, 59).unwrap();
        let (s, e) = month_bounds(at);
        assert_eq!(s, Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap());
        assert_eq!(e, Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap());
        let (s, e) = month_bounds(Utc.with_ymd_and_hms(2027, 2, 1, 0, 0, 0).unwrap());
        assert_eq!((s.month(), e.month()), (2, 3));
    }
}
