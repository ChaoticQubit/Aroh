//! SQLite open and migrations, versioned by `PRAGMA user_version`.

use crate::{Error, Result};
use rusqlite::Connection;
use std::path::Path;

const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE goals (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('course','habit','custom')),
    source_url TEXT,
    target_date TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft','active','done','archived')),
    answers_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0,1))
);
CREATE TABLE actions (
    id TEXT PRIMARY KEY NOT NULL,
    goal_id TEXT NOT NULL,
    title TEXT NOT NULL,
    notes TEXT,
    start_at TEXT NOT NULL,
    end_at TEXT NOT NULL,
    est_minutes INTEGER NOT NULL CHECK (est_minutes >= 0),
    status TEXT NOT NULL CHECK (status IN ('planned','done','missed','skipped')),
    seq INTEGER NOT NULL,
    done_at TEXT,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0,1))
);
CREATE INDEX actions_goal ON actions(goal_id, seq);
CREATE INDEX actions_start ON actions(start_at);
CREATE TABLE plans (
    id TEXT PRIMARY KEY NOT NULL,
    goal_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('proposed','approved','rejected')),
    summary TEXT,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0,1))
);
CREATE TABLE replans (
    id TEXT PRIMARY KEY NOT NULL,
    trigger_action_id TEXT,
    diff_json TEXT NOT NULL,
    size TEXT NOT NULL CHECK (size IN ('small','big')),
    status TEXT NOT NULL CHECK (status IN ('proposed','applied','rejected','undone')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0,1))
);
CREATE TABLE tracker_entries (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('food','water','workout','other')),
    amount REAL,
    unit TEXT,
    note TEXT,
    raw_text TEXT,
    logged_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0,1))
);
CREATE INDEX tracker_logged ON tracker_entries(logged_at);
CREATE TABLE plugins (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    yaml TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0,1))
);
CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"#];

/// Schema version after every migration ran.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Opens (or creates) the database file at `path` and migrates it.
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Opens a migrated in-memory database.
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    migrate(&conn)?;
    Ok(conn)
}

/// Applies every migration newer than the stored `user_version`, each in its own transaction.
pub fn migrate(conn: &Connection) -> Result<()> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if !(0..=SCHEMA_VERSION).contains(&current) {
        return Err(Error::SchemaTooNew(current));
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let next = index as i64 + 1;
        conn.execute_batch(&format!("BEGIN;\n{sql}\nPRAGMA user_version = {next};\nCOMMIT;"))?;
    }
    Ok(())
}
