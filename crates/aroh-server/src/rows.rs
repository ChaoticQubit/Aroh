use crate::db;
use crate::error::ServerError;
use aroh_domain::hlc::Hlc;
use aroh_domain::recur;
use aroh_domain::rows::{Action, ActionSeries, Record};
use aroh_domain::schema::TableDef;
use aroh_domain::seed;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use crate::db::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use troupekit_core::ids::RunId;
use uuid::Uuid;

/// HLC node of every row the server engine writes. No device has it, so
/// devices never push server-written rows back as their own.
/// 16 lowercase hex characters, as the domain requires.
pub const SERVER_NODE: &str = "00000000000000e5";

/// The server engine's hybrid logical clock, shared by every writer.
/// One clock per process keeps server stamps strictly increasing.
/// Safe to share across threads.
#[derive(Debug)]
pub struct ServerClock(Mutex<Hlc>);

impl Default for ServerClock {
    fn default() -> Self {
        ServerClock(Mutex::new(Hlc::new(SERVER_NODE)))
    }
}

impl ServerClock {
    /// Returns a stamp newer than `previous` and every stamp issued so far.
    /// `previous` may be empty for a new row.
    /// Errors only on a malformed remote stamp.
    pub fn stamp_after(&self, previous: &str) -> Result<String, ServerError> {
        let mut clock = self.0.lock().unwrap_or_else(|p| p.into_inner());
        Ok(clock.stamp_after(previous)?)
    }
}

/// Converts Unix milliseconds from a `Clock` to a UTC instant.
/// Panics on a value outside chrono's range, which no clock produces.
/// Returns the instant.
pub fn utc(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().expect("clock time is a valid instant")
}

fn upsert_sql(table: &TableDef, keep_existing: bool) -> String {
    let names = table.names();
    let cols = names.join(", ");
    let action = if keep_existing {
        "nothing".to_string()
    } else {
        let sets: Vec<String> =
            names.iter().filter(|c| !matches!(**c, "id" | "user_id")).map(|c| format!("{c} = excluded.{c}")).collect();
        format!("update set {}", sets.join(", "))
    };
    format!(
        "insert into public.{t} ({cols}) select {cols} from jsonb_populate_record(null::public.{t}, $1) on conflict (user_id, id) do {action}",
        t = table.name
    )
}

/// Stamps `row` with a server HLC newer than its current stamp, validates it
/// with the domain rules and upserts it by `(user_id, id)`; the LWW trigger
/// drops it when a newer version exists. Returns `Domain` for an invalid row.
pub async fn put_row<R: Record>(pool: &PgPool, clock: &ServerClock, user: Uuid, row: &mut R) -> Result<(), ServerError> {
    assert_eq!(row.user_id(), user.to_string(), "server writes stay inside the run's user");
    let stamp = clock.stamp_after(row.updated_at())?;
    row.set_updated_at(stamp);
    row.validate()?;
    let json = serde_json::to_value(&*row).expect("rows serialize");
    db::query(&upsert_sql(R::TABLE, false)).bind(json).execute(pool).await?;
    Ok(())
}

/// Inserts a derived row (seed or materialized occurrence) with its own stamp,
/// leaving an existing row of the same id untouched, tombstones included.
/// Returns true when the row was new.
pub async fn insert_derived<R: Record>(pool: &PgPool, user: Uuid, row: &R) -> Result<bool, ServerError> {
    assert_eq!(row.user_id(), user.to_string(), "server writes stay inside the run's user");
    row.validate()?;
    let json = serde_json::to_value(row).expect("rows serialize");
    let done = db::query(&upsert_sql(R::TABLE, true)).bind(json).execute(pool).await?;
    Ok(done.rows_affected() == 1)
}

/// Lists rows of `R` owned by `user`, filtered by equality on registry
/// columns (compared as text). Tombstones are left out unless
/// `with_deleted`. Panics on a column the table does not have.
pub async fn list_rows<R: Record>(
    pool: &PgPool,
    user: Uuid,
    filters: &[(&str, String)],
    with_deleted: bool,
) -> Result<Vec<R>, ServerError> {
    let mut sql = format!(
        "select to_jsonb(t) - 'server_seq' from public.{} t where t.user_id = $1",
        R::TABLE.name
    );
    if !with_deleted {
        sql.push_str(" and not t.deleted");
    }
    for (i, (col, _)) in filters.iter().enumerate() {
        assert!(R::TABLE.column(col).is_some(), "filter on a registry column");
        sql.push_str(&format!(" and t.{col}::text = ${}", i + 2));
    }
    sql.push_str(" order by t.id");
    let mut q = db::query_scalar::<_, Value>(&sql).bind(user);
    for (_, value) in filters {
        q = q.bind(value);
    }
    let rows = q.fetch_all(pool).await?;
    rows.into_iter().map(parse_row::<R>).collect()
}

/// Parses one `to_jsonb` row of `R`'s table, keeping registry columns only,
/// so a column added by a later migration never breaks a running server.
/// Input: the row object. Output: the row, or `Db` naming the table.
pub fn parse_row<R: Record>(value: Value) -> Result<R, ServerError> {
    let Value::Object(mut row) = value else {
        return Err(ServerError::Db(format!("{}: row is not an object", R::TABLE.name)));
    };
    row.retain(|name, _| R::TABLE.column(name).is_some());
    assert!(row.len() <= R::TABLE.columns.len());
    serde_json::from_value(Value::Object(row)).map_err(|e| ServerError::Db(format!("{}: {e}", R::TABLE.name)))
}

/// Returns the live row `id` of `R` owned by `user`.
/// `None` when it is missing, deleted or another user's.
/// Reads one row by its key.
pub async fn get_row<R: Record>(pool: &PgPool, user: Uuid, id: &str) -> Result<Option<R>, ServerError> {
    Ok(list_rows::<R>(pool, user, &[("id", id.to_string())], false).await?.pop())
}

/// Whether `user`'s goal `goal` is a tombstone (Q68). A goal the server has not seen
/// is not deleted, so a new goal still starts. Runs and tools of a deleted goal stop.
pub async fn goal_deleted(pool: &PgPool, user: Uuid, goal: Uuid) -> Result<bool, ServerError> {
    let deleted: Option<bool> = db::query_scalar("select deleted from public.goals where user_id = $1 and id = $2")
        .bind(user)
        .bind(goal)
        .fetch_optional(pool)
        .await?;
    Ok(deleted == Some(true))
}

/// Readies a user's rows before anything reads them: inserts the seeded
/// categories and materializes every live series for `days` from `now`,
/// both insert-or-ignore, so the server sees what the app engine derives.
pub async fn prepare_reads(pool: &PgPool, user: Uuid, now: DateTime<Utc>, days: u32) -> Result<(), ServerError> {
    for category in seed::categories(&user.to_string()) {
        insert_derived(pool, user, &category).await?;
    }
    for series in list_rows::<ActionSeries>(pool, user, &[], false).await? {
        materialize(pool, user, &series, now, days).await?;
    }
    Ok(())
}

/// Inserts the occurrences of `series` in `[now, now + days)` that are not
/// stored yet, tombstoned ones included. Returns how many were new.
/// Uses the domain materializer, so ids match the app engine's.
pub async fn materialize(
    pool: &PgPool,
    user: Uuid,
    series: &ActionSeries,
    now: DateTime<Utc>,
    days: u32,
) -> Result<usize, ServerError> {
    let stored = list_rows::<Action>(pool, user, &[("series_id", series.id.clone())], true).await?;
    let existing: BTreeSet<DateTime<Utc>> = stored.iter().filter_map(|a| a.occurrence).collect();
    let mut added = 0;
    for action in recur::materialize(series, now, days, &existing)? {
        if insert_derived(pool, user, &action).await? {
            added += 1;
        }
    }
    Ok(added)
}

/// One `runs` row as the engine reads it: owner, goal, workflow name and
/// the persona snapshot (name to TOML text) taken at start.
/// Every store derives a run's `user_id` from this row.
#[derive(Debug, Clone, PartialEq)]
pub struct RunRow {
    pub id: RunId,
    pub user_id: Uuid,
    pub goal_id: Option<Uuid>,
    pub workflow: String,
    pub personas: BTreeMap<String, String>,
}

/// Inserts the `runs` row of a new run in state `queued`.
/// Call it before `Runner::enqueue`, which needs the row.
/// Returns the database error, if any.
pub async fn insert_run(pool: &PgPool, run: &RunRow) -> Result<(), ServerError> {
    let personas = serde_json::to_value(&run.personas).expect("persona map serializes");
    db::query(
        "insert into public.runs (id, user_id, goal_id, workflow, state, personas) values ($1, $2, $3, $4, 'queued', $5)",
    )
    .bind(run.id.0)
    .bind(run.user_id)
    .bind(run.goal_id)
    .bind(&run.workflow)
    .bind(personas)
    .execute(pool)
    .await?;
    Ok(())
}

/// Reads the `runs` row `id`, of any user, for the engine's own work.
/// Requests from users go through `run_row_for` instead.
/// `None` when there is no such run.
pub async fn run_row(pool: &PgPool, id: RunId) -> Result<Option<RunRow>, ServerError> {
    let row: Option<(Uuid, Uuid, Option<Uuid>, String, Value)> =
        db::query_as("select id, user_id, goal_id, workflow, personas from public.runs where id = $1")
            .bind(id.0)
            .fetch_optional(pool)
            .await?;
    row.map(|(id, user_id, goal_id, workflow, personas)| {
        let personas = serde_json::from_value(personas).map_err(|e| ServerError::Db(format!("runs.personas: {e}")))?;
        Ok(RunRow { id: RunId(id), user_id, goal_id, workflow, personas })
    })
    .transpose()
}

/// Reads the `runs` row `id` only when `user` owns it.
/// Returns `NotFound` for a missing run and for another user's run alike,
/// so a caller learns nothing about runs it does not own.
pub async fn run_row_for(pool: &PgPool, user: Uuid, id: RunId) -> Result<RunRow, ServerError> {
    match run_row(pool, id).await? {
        Some(row) if row.user_id == user => Ok(row),
        _ => Err(ServerError::NotFound(format!("run {id}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aroh_domain::rows::Goal;

    fn goal() -> Value {
        serde_json::json!({
            "id": "0190f2a0-0000-7000-8000-000000000091", "user_id": "0190f2a0-0000-7000-8000-00000000000a",
            "category_id": null, "title": "Read Dune", "context": null, "deadline": "2027-01-31",
            "status": "active", "autonomy": "ask", "created_at": "2026-11-23T08:00:00Z",
            "reminders": true, "use_calendars": true, "run_limit_minutes": null,
            "updated_at": "1763683200000-0000-00000000000000aa", "deleted": false
        })
    }

    #[test]
    fn columns_outside_the_registry_are_ignored() {
        let mut row = goal();
        row["added_later"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Goal>(row.clone()).is_err());
        let parsed = parse_row::<Goal>(row).unwrap();
        assert_eq!(parsed, serde_json::from_value::<Goal>(goal()).unwrap());
        parsed.validate().unwrap();
        let mut missing = goal();
        missing.as_object_mut().unwrap().remove("title");
        assert!(parse_row::<Goal>(missing).is_err());
        assert!(parse_row::<Goal>(serde_json::json!([1])).is_err());
    }
}
