//! Undo of a replan (Q77): the proposals a run wrote, and turning its applied
//! ones back. Reads both undo shapes: the server's `changes` (restore, delete)
//! and the device's `actions` (id and the row before).

use crate::{invalid, Result};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{Action, Proposal, ProposalStatus};
use aroh_store::repo;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::Value;

/// The live proposals run `run_id` wrote, in creation order, any status.
pub fn run_proposals(conn: &Connection, run_id: &str) -> Result<Vec<Proposal>> {
    Ok(repo::list::<Proposal>(conn, &[("run_id", run_id)])?)
}

fn time(v: &Value, key: &str) -> Result<DateTime<Utc>> {
    v.get(key)
        .and_then(Value::as_str)
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&Utc))
        .ok_or_else(|| invalid(format!("undo without {key}")))
}

fn restore(conn: &Connection, clock: &mut Hlc, id: &str, span: (DateTime<Utc>, DateTime<Utc>), deleted: bool) -> Result<()> {
    conn.execute("UPDATE actions SET deleted = 0 WHERE id = ?1", [id])?;
    let mut action: Action = repo::get(conn, id)?;
    (action.start_at, action.end_at) = span;
    action.deleted = deleted;
    repo::put(conn, clock, &mut action)?;
    Ok(())
}

fn undo_one(conn: &Connection, clock: &mut Hlc, undo: &Value) -> Result<usize> {
    let mut done = 0;
    for c in undo.get("changes").and_then(Value::as_array).into_iter().flatten() {
        let id = c.get("action_id").and_then(Value::as_str).ok_or_else(|| invalid("undo without action_id"))?;
        match c.get("op").and_then(Value::as_str) {
            Some("restore") => {
                let before = c.get("before").ok_or_else(|| invalid("undo without before"))?;
                let deleted = c.get("deleted").and_then(Value::as_bool).unwrap_or(false);
                restore(conn, clock, id, (time(before, "start_at")?, time(before, "end_at")?), deleted)?;
            }
            Some("delete") => repo::delete::<Action>(conn, clock, id)?,
            other => return Err(invalid(format!("undo op {other:?}"))),
        }
        done += 1;
    }
    for a in undo.get("actions").and_then(Value::as_array).into_iter().flatten() {
        let id = a.get("id").and_then(Value::as_str).ok_or_else(|| invalid("undo without id"))?;
        match a.get("before").filter(|b| !b.is_null()) {
            Some(before) => restore(conn, clock, id, (time(before, "start_at")?, time(before, "end_at")?), false)?,
            None => repo::delete::<Action>(conn, clock, id)?,
        }
        done += 1;
    }
    Ok(done)
}

/// Turns back every applied proposal of run `run_id` in one transaction:
/// moved actions return to their span, created ones are tombstoned, and
/// each proposal becomes `Undone`. Errors with `Invalid` when the run has
/// no applied proposal; nothing changes then. Returns the actions restored.
pub fn undo_run(conn: &Connection, clock: &mut Hlc, run_id: &str) -> Result<usize> {
    let applied: Vec<Proposal> = run_proposals(conn, run_id)?.into_iter().filter(|p| p.status == ProposalStatus::Applied).collect();
    if applied.is_empty() {
        return Err(invalid("nothing of this replan is left to undo"));
    }
    let tx = conn.unchecked_transaction()?;
    let mut done = 0;
    for mut p in applied {
        done += undo_one(&tx, clock, p.undo.as_ref().unwrap_or(&Value::Null))?;
        p.status = ProposalStatus::Undone;
        repo::put(&tx, clock, &mut p)?;
    }
    tx.commit()?;
    Ok(done)
}
