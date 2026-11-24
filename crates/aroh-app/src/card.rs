//! Owner writes on action cards (Q71): manual edits in place for this session or all
//! future ones, undo of a manual or chat edit, and the per-set session log.

use crate::{invalid, Result};
use aroh_domain::card::{check_log, log_id, Card, Checked, Depth, Effort, SetLog};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{Action, ActionLog, ActionSeries, ActionStatus, Proposal, ProposalStatus};
use aroh_store::repo;
use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Which sessions a card edit reaches: this one, or this one, the series template and
/// every later planned occurrence of the series that has no log yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    This,
    Future,
}

/// The undo of one card edit: each changed row's card and length before it,
/// as `{"rows": [...]}`, the same shape the server's chat edits store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardUndo {
    pub rows: Vec<Value>,
}

fn live_action(conn: &Connection, id: &str) -> Result<Action> {
    let a: Action = repo::get(conn, id)?;
    if a.deleted {
        return Err(invalid("action was deleted"));
    }
    Ok(a)
}

fn later_of_series(conn: &Connection, user_id: &str, series_id: &str, after: DateTime<Utc>) -> Result<Vec<Action>> {
    let logged: Vec<String> = repo::list::<ActionLog>(conn, &[("user_id", user_id)])?.into_iter().filter(|l| !l.deleted).map(|l| l.action_id).collect();
    let mut later: Vec<Action> = repo::list::<Action>(conn, &[("user_id", user_id), ("series_id", series_id)])?
        .into_iter()
        .filter(|a| !a.deleted && a.status == ActionStatus::Planned && a.start_at > after && !logged.contains(&a.id))
        .collect();
    later.sort_by_key(|a| a.start_at);
    Ok(later)
}

/// Writes `card` on action `id` and, with `Scope::Future`, on its series template
/// (as an outline copy) and later unlogged planned occurrences, in one transaction.
/// `minutes`, when set, moves each end. Output: the undo of this edit.
pub fn set_card(conn: &Connection, clock: &mut Hlc, id: &str, card: Card, minutes: Option<i64>, scope: Scope) -> Result<CardUndo> {
    if let Some(m) = minutes {
        if !(1..=1440).contains(&m) {
            return Err(invalid("minutes between 1 and 1440"));
        }
    }
    let card = Card { depth: Depth::Full, ..card };
    card.check()?;
    let this = live_action(conn, id)?;
    let tx = conn.unchecked_transaction()?;
    let mut targets = vec![this.clone()];
    let mut rows = Vec::new();
    if let (Scope::Future, Some(series_id)) = (scope, this.series_id.as_deref()) {
        let mut s: ActionSeries = repo::get(&tx, series_id)?;
        rows.push(json!({"table": "action_series", "id": s.id, "card": s.card, "duration_minutes": s.duration_minutes}));
        s.card = Some(card.as_outline());
        if let Some(m) = minutes {
            s.duration_minutes = m;
        }
        repo::put(&tx, clock, &mut s)?;
        targets.extend(later_of_series(&tx, &this.user_id, series_id, this.start_at)?);
    }
    for a in targets.iter_mut() {
        rows.push(json!({"table": "actions", "id": a.id, "card": a.card, "end_at": a.end_at}));
        a.card = Some(Card { outline: a.card.as_ref().and_then(|c| c.outline.clone()), ..card.clone() });
        if let Some(m) = minutes {
            a.end_at = a.start_at + Duration::minutes(m);
        }
        repo::put(&tx, clock, a)?;
    }
    tx.commit()?;
    assert!(rows.len() >= targets.len());
    Ok(CardUndo { rows })
}

/// Saves the owner's notes of action `id` as typed; blank notes clear them.
/// Errors with `Invalid` for a deleted action or notes over 4000 characters.
pub fn set_notes(conn: &Connection, clock: &mut Hlc, id: &str, notes: &str) -> Result<Action> {
    if notes.chars().count() > 4000 {
        return Err(invalid("notes are at most 4000 characters"));
    }
    let mut a = live_action(conn, id)?;
    a.notes = Some(notes.to_string()).filter(|n| !n.trim().is_empty());
    repo::put(conn, clock, &mut a)?;
    Ok(a)
}

/// Puts back every row of `undo`: its card and its end or series length.
/// Rows deleted since are skipped. Errors with `Invalid` for a malformed undo.
pub fn restore_cards(conn: &Connection, clock: &mut Hlc, undo: &CardUndo) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for row in &undo.rows {
        let id = row.get("id").and_then(Value::as_str).ok_or_else(|| invalid("undo row without id"))?;
        let card: Option<Card> = serde_json::from_value(row.get("card").cloned().unwrap_or(Value::Null)).map_err(|e| invalid(format!("undo card: {e}")))?;
        match row.get("table").and_then(Value::as_str) {
            Some("actions") => {
                let mut a: Action = repo::get(&tx, id)?;
                if a.deleted {
                    continue;
                }
                a.card = card;
                if let Some(end) = row.get("end_at").and_then(Value::as_str) {
                    a.end_at = end.parse().map_err(|_| invalid("undo end_at"))?;
                }
                repo::put(&tx, clock, &mut a)?;
            }
            Some("action_series") => {
                let mut s: ActionSeries = repo::get(&tx, id)?;
                if s.deleted {
                    continue;
                }
                s.card = card;
                if let Some(m) = row.get("duration_minutes").and_then(Value::as_i64) {
                    s.duration_minutes = m;
                }
                repo::put(&tx, clock, &mut s)?;
            }
            _ => return Err(invalid("undo row of an unknown table")),
        }
    }
    tx.commit()?;
    Ok(())
}

/// Undo of the card coach's applied chat edit `proposal_id`: restores its rows and
/// marks it undone. Errors with `Invalid` for a proposal that is not an applied card edit.
pub fn undo_card_edit(conn: &Connection, clock: &mut Hlc, proposal_id: &str) -> Result<Proposal> {
    let mut p: Proposal = repo::get(conn, proposal_id)?;
    if p.status != ProposalStatus::Applied || p.diff.get("op").and_then(Value::as_str) != Some("card") {
        return Err(invalid("only an applied card edit is undone here"));
    }
    let undo: CardUndo = serde_json::from_value(p.undo.clone().unwrap_or(Value::Null)).map_err(|e| invalid(format!("proposal undo: {e}")))?;
    restore_cards(conn, clock, &undo)?;
    p.status = ProposalStatus::Undone;
    repo::put(conn, clock, &mut p)?;
    Ok(p)
}

/// The newest card chat edit of action `action_id`, applied or undone, if any.
/// The card page shows its diff and Undo once the coach's run is done.
pub fn latest_card_edit(conn: &Connection, user_id: &str, action_id: &str) -> Result<Option<Proposal>> {
    let edits = repo::list::<Proposal>(conn, &[("user_id", user_id)])?
        .into_iter()
        .filter(|p| !p.deleted && p.diff.get("op").and_then(Value::as_str) == Some("card") && p.diff.get("action_id").and_then(Value::as_str) == Some(action_id));
    Ok(edits.max_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id))))
}

/// The owner's session log of action `action_id`, as one owner tap or voice line saves it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogInput {
    pub sets: Vec<SetLog>,
    pub checked: Vec<Checked>,
    pub effort: Option<Effort>,
    pub note: Option<String>,
}

/// Saves the one log of action `action_id` (derived id, so every device writes the
/// same row), stamped `now`. Errors with `Invalid` for bad sets or a deleted action.
/// Output: the stored row.
pub fn save_log(conn: &Connection, clock: &mut Hlc, action_id: &str, input: LogInput, now: DateTime<Utc>) -> Result<ActionLog> {
    check_log(&input.sets, &input.checked)?;
    let a = live_action(conn, action_id)?;
    let mut log = ActionLog {
        id: log_id(&a.id),
        user_id: a.user_id.clone(),
        action_id: a.id.clone(),
        goal_id: a.goal_id.clone(),
        sets: input.sets,
        checked: input.checked,
        effort: input.effort,
        note: input.note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
        logged_at: now,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut log)?;
    Ok(log)
}

/// The log of action `action_id`, if the owner logged anything.
pub fn log_of(conn: &Connection, action_id: &str) -> Result<Option<ActionLog>> {
    match repo::get::<ActionLog>(conn, &log_id(action_id)) {
        Ok(l) if !l.deleted => Ok(Some(l)),
        Ok(_) => Ok(None),
        Err(aroh_store::Error::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
