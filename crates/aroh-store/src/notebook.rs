//! Notebook side effects of a pulled row (Q96): pages whose rows change are marked so
//! their local links index is rebuilt, and a block version this device wrote that a newer
//! remote version replaces is kept, still sealed, in `page_block_history`.

use crate::codec::WireRow;
use crate::{bad_row, repo, Result};
use aroh_domain::hlc::{self, Hlc};
use aroh_domain::lww::Version;
use aroh_domain::rows::{PageBlock, PageBlockHistory};
use aroh_domain::schema::TableDef;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};

/// Block versions kept per block in `page_block_history`; older ones are tombstoned.
pub const HISTORY_KEEP: usize = 20;

/// Largest number of pages one links rebuild takes from the dirty list.
pub const DIRTY_MAX: usize = 10_000;

fn text<'a>(row: &'a WireRow, column: &str) -> Result<&'a str> {
    row.get(column).and_then(serde_json::Value::as_str).ok_or_else(|| bad_row(format!("row without {column}")))
}

/// Marks page `page_id` so its links are rebuilt from its blocks on the next notebook read.
pub fn mark_dirty(conn: &Connection, page_id: &str) -> Result<()> {
    conn.execute("INSERT OR IGNORE INTO page_links_dirty (page_id) VALUES (?1)", [page_id])?;
    Ok(())
}

/// The pages marked dirty, at most `DIRTY_MAX`, in id order.
pub fn dirty(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT page_id FROM page_links_dirty ORDER BY page_id LIMIT ?1")?;
    let ids = stmt.query_map([DIRTY_MAX as i64], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
    assert!(ids.len() <= DIRTY_MAX);
    Ok(ids)
}

/// Clears the dirty mark of `page_id` once its links are rebuilt.
pub fn clear_dirty(conn: &Connection, page_id: &str) -> Result<()> {
    conn.execute("DELETE FROM page_links_dirty WHERE page_id = ?1", [page_id])?;
    Ok(())
}

/// Runs inside `apply_page`'s transaction just before `remote` replaces the local row of
/// `table` (whose version is `local`, if any). Marks the touched pages dirty and, for a
/// live block this device's node wrote, keeps the losing version as history, unless the
/// block is tombstoned with its page (a page delete keeps no history).
pub(crate) fn before_replace(tx: &Connection, table: &TableDef, remote: &WireRow, local: Option<&Version>, clock: &mut Hlc) -> Result<()> {
    match table.name {
        "pages" => mark_dirty(tx, text(remote, "id")?),
        "page_blocks" => {
            mark_dirty(tx, text(remote, "page_id")?)?;
            let Some(local) = local else { return Ok(()) };
            if local.deleted || local.updated_at == text(remote, "updated_at")? || !local.updated_at.ends_with(clock.node()) {
                return Ok(());
            }
            let old: PageBlock = repo::get(tx, text(remote, "id")?)?;
            mark_dirty(tx, &old.page_id)?;
            let page_gone: Option<i64> = tx.query_row("SELECT deleted FROM pages WHERE id = ?1", [&old.page_id], |r| r.get(0)).optional()?;
            if remote.get("deleted").and_then(serde_json::Value::as_bool) == Some(true) && page_gone == Some(1) {
                return Ok(());
            }
            keep_history(tx, clock, &old)
        }
        _ => Ok(()),
    }
}

/// Stores `old` as a history row of its block, sealed bytes verbatim, with an id derived
/// from the block and its stamp so a version is kept once, then tombstones all but the
/// newest `HISTORY_KEEP` versions of the block.
pub fn keep_history(conn: &Connection, clock: &mut Hlc, old: &PageBlock) -> Result<()> {
    let (millis, _) = hlc::parse(&old.updated_at)?;
    let id = aroh_domain::derived_id(&old.user_id, &format!("page_block_history/{}/{}", old.id, old.updated_at), millis);
    let known: Option<i64> = conn.query_row("SELECT 1 FROM page_block_history WHERE id = ?1", [&id], |r| r.get(0)).optional()?;
    if known.is_none() {
        let created_at = DateTime::<Utc>::from_timestamp_millis(i64::try_from(millis).map_err(|_| bad_row("stamp out of range"))?).ok_or_else(|| bad_row("stamp out of range"))?;
        let mut row = PageBlockHistory {
            id,
            user_id: old.user_id.clone(),
            block_id: old.id.clone(),
            page_id: old.page_id.clone(),
            created_at,
            key_version: old.key_version,
            nonce: old.nonce.clone(),
            ciphertext: old.ciphertext.clone(),
            updated_at: String::new(),
            deleted: false,
        };
        repo::put(conn, clock, &mut row)?;
    }
    let mut stmt = conn.prepare("SELECT id FROM page_block_history WHERE block_id = ?1 AND deleted = 0 ORDER BY created_at DESC, id DESC LIMIT -1 OFFSET ?2")?;
    let extra = stmt.query_map(rusqlite::params![old.id, HISTORY_KEEP as i64], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
    for id in extra {
        repo::delete::<PageBlockHistory>(conn, clock, &id)?;
    }
    Ok(())
}
