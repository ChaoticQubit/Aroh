//! SQLite open and migrations, versioned by `PRAGMA user_version`.

use crate::{Error, Result};
use rusqlite::Connection;
use std::path::Path;

/// Every schema migration in order; migration `n` brings `user_version` from `n` to `n + 1`.
/// Tables are rebuilt rather than altered when a column is added, so column order keeps
/// matching the row registry.
pub const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE categories (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    color TEXT NOT NULL,
    seeded INTEGER NOT NULL CHECK (seeded IN (0, 1)),
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE goals (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    category_id TEXT,
    title TEXT NOT NULL,
    context TEXT,
    deadline TEXT,
    status TEXT NOT NULL,
    autonomy TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE goal_links (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    from_goal_id TEXT NOT NULL,
    to_goal_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    confirmed INTEGER NOT NULL CHECK (confirmed IN (0, 1)),
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE goal_files (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    goal_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    storage_path TEXT,
    url TEXT,
    mime TEXT,
    size_bytes INTEGER NOT NULL,
    ingest_status TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE personas (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    goal_id TEXT,
    name TEXT NOT NULL,
    toml TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE plugins (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    goal_id TEXT,
    name TEXT NOT NULL,
    version INTEGER NOT NULL,
    bundle_toml TEXT NOT NULL,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE trackers (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    goal_id TEXT NOT NULL,
    plugin_id TEXT,
    name TEXT NOT NULL,
    fields TEXT NOT NULL,
    cadence TEXT,
    rules TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE tracker_entries (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    tracker_id TEXT NOT NULL,
    data TEXT NOT NULL,
    source TEXT NOT NULL,
    raw_text TEXT,
    logged_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE INDEX tracker_entries_tracker ON tracker_entries (tracker_id, logged_at);
CREATE TABLE action_series (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    goal_id TEXT NOT NULL,
    title TEXT NOT NULL,
    notes TEXT,
    rrule TEXT NOT NULL,
    dtstart_local TEXT NOT NULL,
    timezone TEXT NOT NULL,
    duration_minutes INTEGER NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE actions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    goal_id TEXT NOT NULL,
    series_id TEXT,
    occurrence TEXT,
    title TEXT NOT NULL,
    notes TEXT,
    start_at TEXT NOT NULL,
    end_at TEXT NOT NULL,
    status TEXT NOT NULL,
    locked INTEGER NOT NULL CHECK (locked IN (0, 1)),
    done_at TEXT,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE INDEX actions_start ON actions (start_at);
CREATE INDEX actions_series ON actions (series_id, occurrence);
CREATE TABLE proposals (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    goal_id TEXT NOT NULL,
    run_id TEXT,
    summary TEXT NOT NULL,
    diff TEXT NOT NULL,
    size TEXT NOT NULL,
    status TEXT NOT NULL,
    undo TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE preferences (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    fact TEXT NOT NULL,
    source TEXT NOT NULL,
    noted_at TEXT NOT NULL,
    confirmed INTEGER NOT NULL CHECK (confirmed IN (0, 1)),
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE busy_blocks (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    calendar_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    title TEXT NOT NULL,
    start_at TEXT NOT NULL,
    end_at TEXT NOT NULL,
    all_day INTEGER NOT NULL CHECK (all_day IN (0, 1)),
    group_name TEXT,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE INDEX busy_blocks_start ON busy_blocks (start_at);
CREATE TABLE calendar_rules (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    calendar_id TEXT,
    series_key TEXT,
    pattern TEXT,
    group_name TEXT NOT NULL,
    decision TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE provider_settings (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    label TEXT NOT NULL,
    vendor TEXT NOT NULL,
    model TEXT NOT NULL,
    base_url TEXT,
    is_default INTEGER NOT NULL CHECK (is_default IN (0, 1)),
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE search_settings (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    label TEXT NOT NULL,
    provider TEXT NOT NULL,
    base_url TEXT,
    priority INTEGER NOT NULL,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE budgets (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    goal_id TEXT,
    limit_usd_micros INTEGER NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE usage (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    goal_id TEXT,
    run_id TEXT,
    vendor TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_usd_micros INTEGER NOT NULL,
    occurred_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
CREATE TABLE sync_state (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT;
"#, r#"
CREATE TABLE busy_blocks_next (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    calendar_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    title TEXT NOT NULL,
    start_at TEXT NOT NULL,
    end_at TEXT NOT NULL,
    all_day INTEGER NOT NULL CHECK (all_day IN (0, 1)),
    group_name TEXT,
    day_kind TEXT CHECK (day_kind IN ('reminder', 'busy', 'away')),
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
INSERT INTO busy_blocks_next SELECT id, user_id, calendar_id, event_id, title, start_at, end_at, all_day, group_name, NULL, updated_at, deleted FROM busy_blocks;
DROP TABLE busy_blocks;
ALTER TABLE busy_blocks_next RENAME TO busy_blocks;
CREATE INDEX busy_blocks_start ON busy_blocks (start_at);
CREATE TABLE calendar_rules_next (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    calendar_id TEXT,
    series_key TEXT,
    pattern TEXT,
    group_name TEXT NOT NULL,
    decision TEXT NOT NULL,
    day_kind TEXT CHECK (day_kind IN ('reminder', 'busy', 'away')),
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1))
) STRICT;
INSERT INTO calendar_rules_next SELECT id, user_id, calendar_id, series_key, pattern, group_name, decision, NULL, updated_at, deleted FROM calendar_rules;
DROP TABLE calendar_rules;
ALTER TABLE calendar_rules_next RENAME TO calendar_rules;
"#];

/// Schema version after every migration ran.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Opens (or creates) the database file at `path` in WAL mode and migrates it.
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

/// Applies every migration newer than the stored `user_version`, each in its own
/// transaction. Errors with `SchemaTooNew` when the file comes from a newer app.
pub fn migrate(conn: &Connection) -> Result<()> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if !(0..=SCHEMA_VERSION).contains(&current) {
        return Err(Error::SchemaTooNew(current));
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let next = index as i64 + 1;
        conn.execute_batch(&format!("BEGIN;\n{sql}\nPRAGMA user_version = {next};\nCOMMIT;"))?;
    }
    let after: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    assert_eq!(after, SCHEMA_VERSION, "migration must end at the schema version");
    Ok(())
}
