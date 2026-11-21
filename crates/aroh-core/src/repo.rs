//! Repository functions for goals and actions. Every write stamps `updated_at` with the HLC.

use crate::hlc::Hlc;
use crate::models::{Action, Goal};
use crate::{Error, Result};
use rusqlite::{params, Connection, Row};

const GOAL_COLUMNS: &str =
    "id, title, kind, source_url, target_date, status, answers_json, created_at, updated_at, deleted";
const ACTION_COLUMNS: &str =
    "id, goal_id, title, notes, start_at, end_at, est_minutes, status, seq, done_at, updated_at, deleted";

fn goal_from_row(row: &Row<'_>) -> rusqlite::Result<Goal> {
    Ok(Goal {
        id: row.get(0)?,
        title: row.get(1)?,
        kind: row.get(2)?,
        source_url: row.get(3)?,
        target_date: row.get(4)?,
        status: row.get(5)?,
        answers_json: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        deleted: row.get(9)?,
    })
}

fn action_from_row(row: &Row<'_>) -> rusqlite::Result<Action> {
    Ok(Action {
        id: row.get(0)?,
        goal_id: row.get(1)?,
        title: row.get(2)?,
        notes: row.get(3)?,
        start_at: row.get(4)?,
        end_at: row.get(5)?,
        est_minutes: row.get(6)?,
        status: row.get(7)?,
        seq: row.get(8)?,
        done_at: row.get(9)?,
        updated_at: row.get(10)?,
        deleted: row.get(11)?,
    })
}

fn expect_one(changed: usize, id: &str) -> Result<()> {
    assert!(changed <= 1, "primary key matched more than one row");
    if changed == 0 {
        return Err(Error::NotFound(id.chars().take(64).collect()));
    }
    Ok(())
}

fn soft_delete(conn: &Connection, clock: &mut Hlc, table: &str, id: &str) -> Result<()> {
    let sql = format!("UPDATE {table} SET deleted = 1, updated_at = ?2 WHERE id = ?1 AND deleted = 0");
    expect_one(conn.execute(&sql, params![id, clock.tick()])?, id)
}

/// Inserts `goal`, stamping its `updated_at` and clearing `deleted`.
pub fn insert_goal(conn: &Connection, clock: &mut Hlc, goal: &mut Goal) -> Result<()> {
    goal.updated_at = clock.tick();
    goal.deleted = false;
    conn.execute(
        &format!("INSERT INTO goals ({GOAL_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)"),
        params![goal.id, goal.title, goal.kind, goal.source_url, goal.target_date, goal.status,
            goal.answers_json, goal.created_at, goal.updated_at],
    )?;
    Ok(())
}

/// Updates every field of a live goal and stamps `updated_at`. Errors with `NotFound` if missing or deleted.
pub fn update_goal(conn: &Connection, clock: &mut Hlc, goal: &mut Goal) -> Result<()> {
    goal.updated_at = clock.tick();
    let changed = conn.execute(
        "UPDATE goals SET title = ?2, kind = ?3, source_url = ?4, target_date = ?5, status = ?6,
            answers_json = ?7, updated_at = ?8 WHERE id = ?1 AND deleted = 0",
        params![goal.id, goal.title, goal.kind, goal.source_url, goal.target_date, goal.status,
            goal.answers_json, goal.updated_at],
    )?;
    expect_one(changed, &goal.id)
}

/// Marks a goal deleted (tombstone) and stamps `updated_at`.
pub fn soft_delete_goal(conn: &Connection, clock: &mut Hlc, id: &str) -> Result<()> {
    soft_delete(conn, clock, "goals", id)
}

/// Lists live goals ordered by creation time.
pub fn list_goals(conn: &Connection) -> Result<Vec<Goal>> {
    let mut stmt = conn.prepare(&format!("SELECT {GOAL_COLUMNS} FROM goals WHERE deleted = 0 ORDER BY created_at, id"))?;
    let rows = stmt.query_map([], goal_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Inserts `action`, stamping its `updated_at` and clearing `deleted`.
pub fn insert_action(conn: &Connection, clock: &mut Hlc, action: &mut Action) -> Result<()> {
    action.updated_at = clock.tick();
    action.deleted = false;
    conn.execute(
        &format!("INSERT INTO actions ({ACTION_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0)"),
        params![action.id, action.goal_id, action.title, action.notes, action.start_at, action.end_at,
            action.est_minutes, action.status, action.seq, action.done_at, action.updated_at],
    )?;
    Ok(())
}

/// Updates every field of a live action and stamps `updated_at`. Errors with `NotFound` if missing or deleted.
pub fn update_action(conn: &Connection, clock: &mut Hlc, action: &mut Action) -> Result<()> {
    action.updated_at = clock.tick();
    let changed = conn.execute(
        "UPDATE actions SET goal_id = ?2, title = ?3, notes = ?4, start_at = ?5, end_at = ?6, est_minutes = ?7,
            status = ?8, seq = ?9, done_at = ?10, updated_at = ?11 WHERE id = ?1 AND deleted = 0",
        params![action.id, action.goal_id, action.title, action.notes, action.start_at, action.end_at,
            action.est_minutes, action.status, action.seq, action.done_at, action.updated_at],
    )?;
    expect_one(changed, &action.id)
}

/// Marks an action deleted (tombstone) and stamps `updated_at`.
pub fn soft_delete_action(conn: &Connection, clock: &mut Hlc, id: &str) -> Result<()> {
    soft_delete(conn, clock, "actions", id)
}

/// Lists live actions by start time, optionally only those of `goal_id`.
pub fn list_actions(conn: &Connection, goal_id: Option<&str>) -> Result<Vec<Action>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ACTION_COLUMNS} FROM actions WHERE deleted = 0 AND (?1 IS NULL OR goal_id = ?1) ORDER BY start_at, seq, id"
    ))?;
    let rows = stmt.query_map(params![goal_id], action_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
