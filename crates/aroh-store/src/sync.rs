//! Last-write-wins row sync of every synced table, salvaged from the MVP
//! `aroh-sync` and driven through a `Remote` (PostgREST in production).

use crate::codec::{normalize, row_from_sql, WireRow};
use crate::repo::upsert;
use crate::{bad_row, Error, Result};
use aroh_domain::hlc::Hlc;
use aroh_domain::lww::{remote_wins, Version};
use aroh_domain::schema::{TableDef, SYNCED};
use rusqlite::{Connection, OptionalExtension};
use std::future::Future;
use std::sync::Mutex;

/// Rows per push batch and per pull page.
pub const PAGE: usize = 500;

/// Byte offset of the node id inside an HLC stamp (`<13 ms>-<4 counter>-<node>`).
const NODE_OFFSET: usize = 13 + 1 + 4 + 1;

/// The server side of sync. `push` upserts rows (the server keeps the newer stamp);
/// `pull` returns at most `limit` rows with `server_seq` above `after`, in
/// `server_seq` order, each carrying its integer `server_seq` column.
pub trait Remote {
    fn push(&mut self, table: &'static TableDef, rows: &[WireRow]) -> impl Future<Output = Result<()>> + Send;
    fn pull(&mut self, table: &'static TableDef, after: i64, limit: usize) -> impl Future<Output = Result<Vec<WireRow>>> + Send;
}

/// Counts of rows one sync moved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Moved {
    pub pushed: usize,
    pub pulled: usize,
}

/// Reads one `sync_state` value, such as `pull:<user>:goals`.
pub fn get_state(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn.query_row("SELECT value FROM sync_state WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

/// Writes one `sync_state` value, replacing the old one.
pub fn set_state(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO sync_state (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

fn key(direction: &str, user_id: &str, table: &TableDef) -> String {
    format!("{direction}:{user_id}:{}", table.name)
}

/// The highest `server_seq` of `table` this device has pulled for `user_id`, 0 before the
/// first pull. Every remote row at or below it is in the local store.
pub fn pulled_seq(conn: &Connection, user_id: &str, table: &TableDef) -> Result<i64> {
    let cursor_key = key("pull", user_id, table);
    match get_state(conn, &cursor_key)? {
        None => Ok(0),
        Some(v) => v.parse().map_err(|_| bad_row(format!("bad cursor {cursor_key}"))),
    }
}

/// Rows of `user_id` this device wrote (HLC node `node`) with `updated_at` above
/// `after`, oldest first, at most `limit`. Pulled and derived rows carry another
/// node and are never pushed back.
pub fn pending(conn: &Connection, table: &TableDef, user_id: &str, node: &str, after: &str, limit: usize) -> Result<Vec<WireRow>> {
    assert!(limit > 0 && limit <= PAGE, "push batch size out of range");
    let sql = format!(
        "SELECT {} FROM {} WHERE user_id = ?1 AND updated_at > ?2 AND substr(updated_at, {}) = ?3
            ORDER BY updated_at LIMIT ?4",
        table.names().join(", "),
        table.name,
        NODE_OFFSET + 1
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params![user_id, after, node, limit as i64])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        out.push(row_from_sql(table, r)?);
    }
    assert!(out.len() <= limit);
    Ok(out)
}

fn version(row: &WireRow) -> Version {
    let updated_at = row["updated_at"].as_str().expect("normalized row has a text updated_at").to_string();
    let deleted = row["deleted"].as_bool().expect("normalized row has a bool deleted");
    Version { updated_at, deleted }
}

/// Merges one pulled page of `table` for `user_id` and stores the pull cursor in the
/// same transaction. Rows are normalized and validated, rows of another user are
/// refused, and every remote stamp is fed to `clock`. On error nothing is committed.
/// Output: the number of rows written locally.
pub fn apply_page(conn: &mut Connection, table: &'static TableDef, user_id: &str, rows: &[WireRow], clock: &mut Hlc) -> Result<usize> {
    let tx = conn.transaction()?;
    let cursor_key = key("pull", user_id, table);
    let mut cursor = pulled_seq(&tx, user_id, table)?;
    let local_sql = format!("SELECT updated_at, deleted FROM {} WHERE id = ?1", table.name);
    let mut written = 0;
    for row in rows {
        let seq = row.get("server_seq").and_then(serde_json::Value::as_i64).ok_or_else(|| bad_row("row without server_seq"))?;
        if seq <= cursor {
            return Err(bad_row("server_seq did not advance"));
        }
        cursor = seq;
        let row = normalize(table, row)?;
        if row["user_id"].as_str() != Some(user_id) {
            return Err(bad_row(format!("{} row of another user", table.name)));
        }
        let remote = version(&row);
        clock.receive(&remote.updated_at)?;
        let id = row["id"].as_str().expect("normalized row has a text id");
        let local = tx
            .query_row(&local_sql, [id], |r| Ok(Version { updated_at: r.get(0)?, deleted: r.get::<_, i64>(1)? != 0 }))
            .optional()?;
        if remote_wins(local.as_ref(), &remote) {
            crate::notebook::before_replace(&tx, table, &row, local.as_ref(), clock)?;
            upsert(&tx, table, &row, false)?;
            written += 1;
        }
    }
    set_state(&tx, &cursor_key, &cursor.to_string())?;
    tx.commit()?;
    assert!(written <= rows.len());
    Ok(written)
}

/// The local side of sync: runs one short step on the store and clock. Callers that
/// share the store behind a lock take it for the step only, so no lock is ever held
/// across a `Remote` call. Output: the step's result.
pub trait Local {
    fn with<T>(&mut self, f: impl FnOnce(&mut Connection, &mut Hlc) -> Result<T>) -> Result<T>;
}

/// A store this caller owns outright, used by `sync_all`.
struct Owned<'a> {
    conn: &'a mut Connection,
    clock: &'a mut Hlc,
}

impl Local for Owned<'_> {
    fn with<T>(&mut self, f: impl FnOnce(&mut Connection, &mut Hlc) -> Result<T>) -> Result<T> {
        f(self.conn, self.clock)
    }
}

/// The parts of a shared store a sync needs: its connection, clock and the local user id
/// the store belongs to now. Output: all three, borrowed from the locked store.
pub trait Parts {
    fn parts(&mut self) -> (&mut Connection, &mut Hlc, &str);
}

/// A `Local` over a store behind a mutex: each step locks it for that step only and
/// stops with `SyncStopped` once the store's local user is no longer `user`, so a
/// sign-in or wipe made during a sync ends it before it writes the old account's rows.
pub struct Locked<'a, S> {
    store: &'a Mutex<S>,
    user: String,
}

impl<'a, S> Locked<'a, S> {
    /// Wraps `store` for one sync of `user`, the store's local user when the sync starts.
    pub fn new(store: &'a Mutex<S>, user: impl Into<String>) -> Self {
        let user = user.into();
        assert!(!user.is_empty(), "sync needs a local user");
        Locked { store, user }
    }
}

impl<S: Parts> Local for Locked<'_, S> {
    fn with<T>(&mut self, f: impl FnOnce(&mut Connection, &mut Hlc) -> Result<T>) -> Result<T> {
        let mut guard = self.store.lock().map_err(|_| Error::SyncStopped("database lock poisoned"))?;
        let (conn, clock, user) = guard.parts();
        if user != self.user {
            return Err(Error::SyncStopped("the local account changed"));
        }
        f(conn, clock)
    }
}

async fn push<L: Local, R: Remote>(local: &mut L, remote: &mut R, table: &'static TableDef, user_id: &str, node: &str) -> Result<usize> {
    let cursor_key = key("push", user_id, table);
    let mut pushed = 0;
    loop {
        let (cursor, rows) = local.with(|conn, _| {
            let cursor = get_state(conn, &cursor_key)?.unwrap_or_default();
            let rows = pending(conn, table, user_id, node, &cursor, PAGE)?;
            Ok((cursor, rows))
        })?;
        let Some(last) = rows.last().and_then(|r| r["updated_at"].as_str()).map(str::to_string) else {
            return Ok(pushed);
        };
        assert!(last > cursor, "push cursor must advance");
        remote.push(table, &rows).await?;
        local.with(|conn, _| set_state(conn, &cursor_key, &last))?;
        pushed += rows.len();
        if rows.len() < PAGE {
            return Ok(pushed);
        }
    }
}

async fn pull<L: Local, R: Remote>(local: &mut L, remote: &mut R, table: &'static TableDef, user_id: &str) -> Result<usize> {
    let mut pulled = 0;
    loop {
        let cursor = local.with(|conn, _| pulled_seq(conn, user_id, table))?;
        let rows = remote.pull(table, cursor, PAGE).await?;
        if rows.len() > PAGE {
            return Err(bad_row("page larger than requested"));
        }
        if rows.is_empty() {
            return Ok(pulled);
        }
        pulled += local.with(|conn, clock| apply_page(conn, table, user_id, &rows, clock))?;
        if rows.len() < PAGE {
            return Ok(pulled);
        }
    }
}

/// Pushes this device's changes, then pulls remote ones, for every synced table of
/// `user_id`, parents before children. Remote stamps advance `clock`.
/// Output: rows pushed and rows written locally.
pub async fn sync_all<R: Remote>(conn: &mut Connection, remote: &mut R, user_id: &str, clock: &mut Hlc) -> Result<Moved> {
    sync_with(&mut Owned { conn, clock }, remote, user_id).await
}

/// `sync_all` over a `Local`: every local step (read cursor and pending page, set push
/// cursor, read pull cursor, apply page) is one `with` call and every `Remote` call runs
/// outside it. A write made between reading a page and setting its cursor carries a
/// later stamp, so it stays pending. Output: rows pushed and rows written locally.
pub async fn sync_with<L: Local, R: Remote>(local: &mut L, remote: &mut R, user_id: &str) -> Result<Moved> {
    let node = local.with(|_, clock| Ok(clock.node().to_string()))?;
    let mut moved = Moved::default();
    for table in SYNCED.iter().copied() {
        moved.pushed += push(local, remote, table, user_id, &node).await?;
    }
    for table in SYNCED.iter().copied() {
        moved.pulled += pull(local, remote, table, user_id).await?;
    }
    Ok(moved)
}
