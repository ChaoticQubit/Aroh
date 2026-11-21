use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Map, Value as Json};

use crate::merge::{remote_wins, Version};
use crate::Error;

/// One synced table: its name, primary key column and every column the
/// contract defines, so remote JSON keys outside this list are never used as
/// SQL identifiers.
#[derive(Debug)]
pub struct Table {
    pub name: &'static str,
    pub pk: &'static str,
    pub columns: &'static [&'static str],
}

impl Table {
    /// Whether the table carries a `deleted` tombstone column.
    pub fn has_tombstone(&self) -> bool {
        self.columns.contains(&"deleted")
    }
}

/// Every synced table, in push and pull order, with columns matching the MVP
/// contract exactly.
pub const TABLES: [Table; 7] = [
    Table {
        name: "goals",
        pk: "id",
        columns: &[
            "id", "title", "kind", "source_url", "target_date", "status", "answers_json",
            "created_at", "updated_at", "deleted",
        ],
    },
    Table {
        name: "actions",
        pk: "id",
        columns: &[
            "id", "goal_id", "title", "notes", "start_at", "end_at", "est_minutes", "status",
            "seq", "done_at", "updated_at", "deleted",
        ],
    },
    Table {
        name: "plans",
        pk: "id",
        columns: &["id", "goal_id", "version", "status", "summary", "updated_at", "deleted"],
    },
    Table {
        name: "replans",
        pk: "id",
        columns: &[
            "id", "trigger_action_id", "diff_json", "size", "status", "created_at", "updated_at",
            "deleted",
        ],
    },
    Table {
        name: "tracker_entries",
        pk: "id",
        columns: &[
            "id", "kind", "amount", "unit", "note", "raw_text", "logged_at", "updated_at",
            "deleted",
        ],
    },
    Table {
        name: "plugins",
        pk: "id",
        columns: &["id", "name", "yaml", "enabled", "updated_at", "deleted"],
    },
    Table {
        name: "settings",
        pk: "key",
        columns: &["key", "value", "updated_at"],
    },
];

/// A row as JSON, keyed by column name.
pub type Row = Map<String, Json>;

/// Creates the local, non-synced `sync_state` table when it is missing.
pub fn ensure_state(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS sync_state (key TEXT PRIMARY KEY, value TEXT)")?;
    Ok(())
}

/// Reads one `sync_state` value, such as `pull:goals`.
pub fn get_state(conn: &Connection, key: &str) -> Result<Option<String>, Error> {
    let value = conn
        .query_row("SELECT value FROM sync_state WHERE key = ?1", [key], |r| r.get(0))
        .optional()?;
    Ok(value)
}

/// Writes one `sync_state` value, replacing the old one.
pub fn set_state(conn: &Connection, key: &str, value: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO sync_state (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

/// Local rows with `updated_at` above `after`, oldest first, at most `limit`.
pub fn pending(conn: &Connection, table: &Table, after: &str, limit: usize) -> Result<Vec<Row>, Error> {
    assert!(limit > 0);
    let sql = format!(
        "SELECT {} FROM {} WHERE updated_at > ?1 ORDER BY updated_at LIMIT ?2",
        table.columns.join(", "),
        table.name
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params![after, limit as i64])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        let mut row = Row::new();
        for (i, column) in table.columns.iter().enumerate() {
            row.insert((*column).to_string(), to_json(r.get(i)?)?);
        }
        out.push(row);
    }
    assert!(out.len() <= limit);
    Ok(out)
}

/// Merges one pulled page and stores the pull cursor in the same transaction.
/// Input: remote rows (unknown keys such as `user_id` are ignored), the page's
/// last `server_seq`, and a callback receiving each remote HLC.
/// Output: the number of rows written locally. On error nothing is committed.
pub fn apply_page(
    conn: &mut Connection,
    table: &Table,
    rows: &[Row],
    cursor: i64,
    on_remote: &mut dyn FnMut(&str),
) -> Result<usize, Error> {
    let tx = conn.transaction()?;
    let local_sql = format!(
        "SELECT updated_at, {} FROM {} WHERE {} = ?1",
        if table.has_tombstone() { "deleted" } else { "0" },
        table.name,
        table.pk
    );
    let mut written = 0;
    for row in rows {
        let key = row
            .get(table.pk)
            .and_then(Json::as_str)
            .ok_or_else(|| Error::InvalidRow(format!("{} row without {}", table.name, table.pk)))?;
        let remote = version(row)?;
        on_remote(&remote.updated_at);
        let local = tx
            .query_row(&local_sql, [key], |r| {
                Ok(Version { updated_at: r.get(0)?, deleted: r.get::<_, i64>(1)? != 0 })
            })
            .optional()?;
        if remote_wins(local.as_ref(), &remote) {
            upsert(&tx, table, row)?;
            written += 1;
        }
    }
    set_state(&tx, &format!("pull:{}", table.name), &cursor.to_string())?;
    tx.commit()?;
    assert!(written <= rows.len());
    Ok(written)
}

fn version(row: &Row) -> Result<Version, Error> {
    let updated_at = row
        .get("updated_at")
        .and_then(Json::as_str)
        .filter(|s| is_hlc(s))
        .ok_or_else(|| Error::InvalidRow("bad updated_at".into()))?;
    let deleted = match row.get("deleted") {
        None | Some(Json::Null) => false,
        Some(d) => match d.as_i64() {
            Some(0) => false,
            Some(1) => true,
            _ => return Err(Error::InvalidRow("bad deleted".into())),
        },
    };
    Ok(Version { updated_at: updated_at.to_string(), deleted })
}

/// Whether `s` has the fixed HLC shape `<13 digits>-<4 hex>-<16 hex>`.
pub fn is_hlc(s: &str) -> bool {
    let b = s.as_bytes();
    let hex = |r: std::ops::Range<usize>| b[r].iter().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c));
    b.len() == 35
        && b[..13].iter().all(u8::is_ascii_digit)
        && b[13] == b'-'
        && hex(14..18)
        && b[18] == b'-'
        && hex(19..35)
}

fn upsert(conn: &Connection, table: &Table, row: &Row) -> Result<(), Error> {
    let columns: Vec<&str> = table.columns.iter().copied().filter(|c| row.contains_key(*c)).collect();
    assert!(columns.contains(&table.pk));
    let params: Vec<String> = (1..=columns.len()).map(|i| format!("?{i}")).collect();
    let updates: Vec<String> = columns
        .iter()
        .filter(|c| **c != table.pk)
        .map(|c| format!("{c} = excluded.{c}"))
        .collect();
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT({}) DO UPDATE SET {}",
        table.name,
        columns.join(", "),
        params.join(", "),
        table.pk,
        updates.join(", ")
    );
    let values = columns.iter().map(|c| to_sql(&row[*c])).collect::<Result<Vec<_>, _>>()?;
    conn.execute(&sql, rusqlite::params_from_iter(values))?;
    Ok(())
}

fn to_sql(value: &Json) -> Result<Value, Error> {
    Ok(match value {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Integer(i64::from(*b)),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Real(n.as_f64().ok_or_else(|| Error::InvalidRow("bad number".into()))?),
        },
        Json::String(s) => Value::Text(s.clone()),
        Json::Array(_) | Json::Object(_) => return Err(Error::InvalidRow("nested value".into())),
    })
}

fn to_json(value: Value) -> Result<Json, Error> {
    Ok(match value {
        Value::Null => Json::Null,
        Value::Integer(i) => Json::from(i),
        Value::Real(f) => serde_json::Number::from_f64(f).map_or(Json::Null, Json::Number),
        Value::Text(s) => Json::String(s),
        Value::Blob(_) => return Err(Error::InvalidRow("blob column".into())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE goals (id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL, kind TEXT NOT NULL,
             source_url TEXT, target_date TEXT, status TEXT NOT NULL, answers_json TEXT,
             created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0);",
        )
        .unwrap();
        ensure_state(&conn).unwrap();
        conn
    }

    fn goal(id: &str, title: &str, updated_at: &str, deleted: i64) -> Row {
        json!({"id": id, "title": title, "kind": "custom", "source_url": null, "target_date": null,
               "status": "active", "answers_json": null, "created_at": "2026-11-20T00:00:00Z",
               "updated_at": updated_at, "deleted": deleted, "user_id": "u", "server_seq": 9})
        .as_object()
        .unwrap()
        .clone()
    }

    fn title(conn: &Connection, id: &str) -> Option<String> {
        conn.query_row("SELECT title FROM goals WHERE id = ?1", [id], |r| r.get(0))
            .optional()
            .unwrap()
    }

    const GOALS: &Table = &TABLES[0];

    #[test]
    fn cursor_roundtrip() {
        let conn = db();
        assert_eq!(get_state(&conn, "pull:goals").unwrap(), None);
        set_state(&conn, "pull:goals", "41").unwrap();
        set_state(&conn, "pull:goals", "42").unwrap();
        set_state(&conn, "push:goals", "1763683200000-0000-aaaaaaaaaaaaaaaa").unwrap();
        assert_eq!(get_state(&conn, "pull:goals").unwrap().as_deref(), Some("42"));
        assert_eq!(
            get_state(&conn, "push:goals").unwrap().as_deref(),
            Some("1763683200000-0000-aaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn page_applies_merge_and_cursor_together() {
        let mut conn = db();
        let mut seen = Vec::new();
        let rows = [goal("a", "A1", "1763683200001-0000-bbbbbbbbbbbbbbbb", 0)];
        let written = apply_page(&mut conn, GOALS, &rows, 7, &mut |h| seen.push(h.to_string())).unwrap();
        assert_eq!(written, 1);
        assert_eq!(title(&conn, "a").as_deref(), Some("A1"));
        assert_eq!(get_state(&conn, "pull:goals").unwrap().as_deref(), Some("7"));
        assert_eq!(seen, ["1763683200001-0000-bbbbbbbbbbbbbbbb"]);

        let older = [goal("a", "stale", "1763683200000-0000-aaaaaaaaaaaaaaaa", 0)];
        assert_eq!(apply_page(&mut conn, GOALS, &older, 8, &mut |_| {}).unwrap(), 0);
        assert_eq!(title(&conn, "a").as_deref(), Some("A1"));
        assert_eq!(get_state(&conn, "pull:goals").unwrap().as_deref(), Some("8"));
    }

    #[test]
    fn failed_page_commits_neither_rows_nor_cursor() {
        let mut conn = db();
        set_state(&conn, "pull:goals", "3").unwrap();
        let mut bad = goal("b", "B", "1763683200001-0000-bbbbbbbbbbbbbbbb", 0);
        bad.remove("title");
        let rows = [goal("a", "A", "1763683200001-0000-bbbbbbbbbbbbbbbb", 0), bad];
        assert!(apply_page(&mut conn, GOALS, &rows, 9, &mut |_| {}).is_err());
        assert_eq!(title(&conn, "a"), None);
        assert_eq!(get_state(&conn, "pull:goals").unwrap().as_deref(), Some("3"));
    }

    #[test]
    fn pending_returns_rows_after_cursor_in_order() {
        let mut conn = db();
        let rows = [
            goal("b", "B", "1763683200002-0000-bbbbbbbbbbbbbbbb", 0),
            goal("a", "A", "1763683200001-0000-bbbbbbbbbbbbbbbb", 1),
        ];
        apply_page(&mut conn, GOALS, &rows, 1, &mut |_| {}).unwrap();
        let out = pending(&conn, GOALS, "1763683200001-0000-bbbbbbbbbbbbbbbb", 500).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["id"], "b");
        assert!(!out[0].contains_key("user_id"));
        assert_eq!(pending(&conn, GOALS, "", 1).unwrap()[0]["deleted"], 1);
    }
}
