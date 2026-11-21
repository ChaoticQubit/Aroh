//! Generic repositories for every synced row type plus the recurrence operations.
//! Every local write stamps `updated_at` with the device HLC.

use crate::codec::{row_from_sql, row_to_sql, WireRow};
use crate::{bad_row, Error, Result};
use aroh_domain::hlc::Hlc;
use aroh_domain::recur;
use aroh_domain::rows::{Action, ActionSeries, ActionStatus, Record};
use aroh_domain::schema::TableDef;
use chrono::{DateTime, Utc};
use rusqlite::{params_from_iter, Connection, OptionalExtension};
use std::collections::BTreeSet;

fn not_found(id: &str) -> Error {
    Error::NotFound(id.chars().take(64).collect())
}

fn to_wire<R: Record>(row: &R) -> Result<WireRow> {
    match serde_json::to_value(row).map_err(|e| bad_row(e.to_string()))? {
        serde_json::Value::Object(map) => Ok(map),
        _ => Err(bad_row("row did not serialize to an object")),
    }
}

fn from_wire<R: Record>(row: WireRow) -> Result<R> {
    serde_json::from_value(serde_json::Value::Object(row)).map_err(|e| bad_row(format!("{}: {e}", R::TABLE.name)))
}

/// Writes every column of `row` into `table`, inserting or replacing by `id`.
/// With `keep_existing` an existing row is left untouched. Returns rows changed.
pub(crate) fn upsert(conn: &Connection, table: &TableDef, row: &WireRow, keep_existing: bool) -> Result<usize> {
    let names = table.names();
    let marks: Vec<String> = (1..=names.len()).map(|i| format!("?{i}")).collect();
    let action = if keep_existing {
        "NOTHING".to_string()
    } else {
        let sets: Vec<String> = names.iter().filter(|c| **c != "id").map(|c| format!("{c} = excluded.{c}")).collect();
        format!("UPDATE SET {}", sets.join(", "))
    };
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT(id) DO {action}",
        table.name,
        names.join(", "),
        marks.join(", ")
    );
    let changed = conn.execute(&sql, params_from_iter(row_to_sql(table, row)?))?;
    assert!(changed <= 1, "upsert by primary key changed more than one row");
    Ok(changed)
}

/// Returns a stamp newer than `previous` (when it is a stamp) and than every stamp
/// `clock` issued, so a local write always wins over the version it replaces even
/// when that version came from a device whose clock ran ahead.
fn stamp_after(clock: &mut Hlc, previous: &str) -> Result<String> {
    let stamp = match aroh_domain::hlc::parse(previous) {
        Ok(_) => clock.receive(previous)?,
        Err(_) => clock.tick(),
    };
    assert!(stamp.as_str() > previous, "a local write must carry a newer stamp");
    Ok(stamp)
}

/// Stamps `row` with an HLC newer than its current stamp, validates it and inserts
/// or replaces it. Errors with `Domain` when the row is invalid; nothing is written then.
pub fn put<R: Record>(conn: &Connection, clock: &mut Hlc, row: &mut R) -> Result<()> {
    let stamp = stamp_after(clock, row.updated_at())?;
    row.set_updated_at(stamp);
    row.validate()?;
    upsert(conn, R::TABLE, &to_wire(row)?, false)?;
    Ok(())
}

/// Inserts a derived row (seed or materialized occurrence) with its own stamp,
/// leaving any existing row of the same id untouched, tombstones included.
/// Output: true when the row was new.
pub fn insert_derived<R: Record>(conn: &Connection, row: &R) -> Result<bool> {
    row.validate()?;
    Ok(upsert(conn, R::TABLE, &to_wire(row)?, true)? == 1)
}

/// Returns the live row `id` of `R`. Errors with `NotFound` if missing or deleted.
pub fn get<R: Record>(conn: &Connection, id: &str) -> Result<R> {
    let table = R::TABLE;
    let sql = format!("SELECT {} FROM {} WHERE id = ?1 AND deleted = 0", table.names().join(", "), table.name);
    let row = conn
        .query_row(&sql, [id], |r| Ok(row_from_sql(table, r)))
        .optional()?
        .ok_or_else(|| not_found(id))??;
    from_wire(row)
}

/// Lists live rows of `R` whose columns equal the given text values, in id order
/// (creation order for v7 ids). Filter columns must be registry columns of `R`.
pub fn list<R: Record>(conn: &Connection, filter: &[(&str, &str)]) -> Result<Vec<R>> {
    let table = R::TABLE;
    let mut clauses = vec!["deleted = 0".to_string()];
    for (i, (column, _)) in filter.iter().enumerate() {
        assert!(table.column(column).is_some(), "filter on unknown column {column}");
        clauses.push(format!("{column} = ?{}", i + 1));
    }
    let sql = format!(
        "SELECT {} FROM {} WHERE {} ORDER BY id",
        table.names().join(", "),
        table.name,
        clauses.join(" AND ")
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(filter.iter().map(|(_, v)| *v)))?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        out.push(from_wire(row_from_sql(table, r)?)?);
    }
    Ok(out)
}

/// Tombstones the live row `id` of `R` with a stamp newer than its current one.
/// Errors with `NotFound` when the row is missing or already deleted.
pub fn delete<R: Record>(conn: &Connection, clock: &mut Hlc, id: &str) -> Result<()> {
    let table = R::TABLE.name;
    let current: String = conn
        .query_row(&format!("SELECT updated_at FROM {table} WHERE id = ?1 AND deleted = 0"), [id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| not_found(id))?;
    let stamp = stamp_after(clock, &current)?;
    let changed =
        conn.execute(&format!("UPDATE {table} SET deleted = 1, updated_at = ?2 WHERE id = ?1"), rusqlite::params![id, stamp])?;
    assert_eq!(changed, 1, "the row read a moment ago must update");
    Ok(())
}

fn occurrences(conn: &Connection, series_id: &str) -> Result<BTreeSet<DateTime<Utc>>> {
    let mut stmt = conn.prepare("SELECT occurrence FROM actions WHERE series_id = ?1 AND occurrence IS NOT NULL")?;
    let texts = stmt.query_map([series_id], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    texts
        .iter()
        .map(|t| DateTime::parse_from_rfc3339(t).map(|d| d.with_timezone(&Utc)).map_err(|e| bad_row(e.to_string())))
        .collect()
}

fn store_occurrences(conn: &Connection, series: &ActionSeries, from: DateTime<Utc>, days: u32) -> Result<Vec<Action>> {
    let actions = recur::materialize(series, from, days, &occurrences(conn, &series.id)?)?;
    for action in &actions {
        let inserted = insert_derived(conn, action)?;
        assert!(inserted, "a missing occurrence must insert");
    }
    Ok(actions)
}

/// Stores the missing occurrences of live series `series_id` in `[from, from + days)`.
/// Occurrences already stored, including tombstoned ones, are never re-created.
/// Output: the actions inserted, in start order.
pub fn materialize(conn: &Connection, series_id: &str, from: DateTime<Utc>, days: u32) -> Result<Vec<Action>> {
    let tx = conn.unchecked_transaction()?;
    let series: ActionSeries = get(&tx, series_id)?;
    let actions = store_occurrences(&tx, &series, from, days)?;
    tx.commit()?;
    Ok(actions)
}

/// Applies a "this and following" edit at occurrence `at` of series `series_id` in one
/// transaction: bounds (or tombstones) the old series, stores the new one, tombstones
/// the old series' planned, unlocked occurrences from `at` on, moves the other live ones
/// (locked, done, missed, skipped) to the new series unchanged otherwise, so no instant is
/// booked twice, and materializes the new series for `[from, from + days)`.
pub fn split_series(
    conn: &Connection,
    clock: &mut Hlc,
    series_id: &str,
    at: DateTime<Utc>,
    from: DateTime<Utc>,
    days: u32,
) -> Result<(ActionSeries, ActionSeries)> {
    let tx = conn.unchecked_transaction()?;
    let series: ActionSeries = get(&tx, series_id)?;
    let (mut old, mut new) = recur::split(&series, at, aroh_domain::new_id())?;
    put(&tx, clock, &mut old)?;
    put(&tx, clock, &mut new)?;
    let later: Vec<Action> = list::<Action>(&tx, &[("series_id", series_id)])?
        .into_iter()
        .filter(|a| a.occurrence.is_some_and(|o| o >= at))
        .collect();
    for mut action in later {
        if !action.locked && action.status == ActionStatus::Planned {
            delete::<Action>(&tx, clock, &action.id)?;
        } else {
            action.series_id = Some(new.id.clone());
            put(&tx, clock, &mut action)?;
        }
    }
    let left: Vec<Action> = list(&tx, &[("series_id", series_id)])?;
    assert!(left.iter().all(|a| a.occurrence.is_some_and(|o| o < at)), "the old series keeps only earlier occurrences");
    store_occurrences(&tx, &new, from, days)?;
    tx.commit()?;
    Ok((old, new))
}
