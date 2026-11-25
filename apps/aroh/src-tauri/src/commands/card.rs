//! Action card commands (Q71): read one card page, edit it in place, undo an edit and
//! save the session log. Thin wrappers over `aroh_app::card`, parsed at the trust boundary.

use super::app::after_write;
use super::with_core;
use crate::AppState;
use aroh_app::card::{self, CardUndo, LogInput, Scope};
use aroh_domain::card::Card;
use aroh_domain::rows::{Action, ActionLog, ActionSeries, Proposal};
use aroh_store::repo;
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

/// What the card page reads: the action, its series' outline card, the session log
/// and the newest card chat edit with its diff and undo.
#[derive(Debug, Serialize)]
pub struct CardPage {
    pub action: Action,
    pub series_card: Option<Card>,
    pub log: Option<ActionLog>,
    pub edit: Option<Proposal>,
}

/// Reads the card page of action `id`. Errors for a missing or deleted action.
#[tauri::command(async)]
pub fn app_card_page(state: State<'_, AppState>, id: String) -> Result<CardPage, String> {
    with_core(&state, |c| {
        let action: Action = repo::get(&c.conn, &id)?;
        if action.deleted || action.user_id != c.user_id {
            return Err(aroh_app::Error::Invalid("action was deleted".into()));
        }
        let series_card = match action.series_id.as_deref() {
            Some(s) => repo::get::<ActionSeries>(&c.conn, s).ok().and_then(|s| s.card),
            None => None,
        };
        let log = card::log_of(&c.conn, &id)?;
        let edit = card::latest_card_edit(&c.conn, &c.user_id, &id)?;
        Ok(CardPage { action, series_card, log, edit })
    })
}

/// Saves the owner's in-place edit of the card of action `id` for `scope` (`this` or
/// `future`), with `minutes` when the length changed. Returns the undo of this save.
#[tauri::command(async)]
pub fn app_set_card(app: AppHandle, state: State<'_, AppState>, id: String, card: Value, minutes: Option<i64>, scope: String) -> Result<CardUndo, String> {
    let card: Card = serde_json::from_value(card).map_err(|_| "invalid card".to_string())?;
    let scope: Scope = serde_json::from_value(Value::String(scope)).map_err(|_| "invalid scope".to_string())?;
    let undo = with_core(&state, |c| card::set_card(&c.conn, &mut c.clock, &id, card, minutes, scope))?;
    after_write(&app);
    Ok(undo)
}

/// Saves the owner's notes of action `id` as typed.
#[tauri::command(async)]
pub fn app_set_notes(app: AppHandle, state: State<'_, AppState>, id: String, notes: String) -> Result<Action, String> {
    let a = with_core(&state, |c| card::set_notes(&c.conn, &mut c.clock, &id, &notes))?;
    after_write(&app);
    Ok(a)
}

/// Puts back the rows of a manual edit's `undo`.
#[tauri::command(async)]
pub fn app_restore_cards(app: AppHandle, state: State<'_, AppState>, undo: CardUndo) -> Result<(), String> {
    if undo.rows.len() > 400 {
        return Err("invalid undo".into());
    }
    with_core(&state, |c| card::restore_cards(&c.conn, &mut c.clock, &undo))?;
    after_write(&app);
    Ok(())
}

/// Undoes the card coach's applied chat edit `id`. Returns the undone proposal.
#[tauri::command(async)]
pub fn app_undo_card_edit(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<Proposal, String> {
    let p = with_core(&state, |c| card::undo_card_edit(&c.conn, &mut c.clock, &id))?;
    after_write(&app);
    Ok(p)
}

/// Saves the session log of action `action_id` (sets, ticked rows, effort, note).
#[tauri::command(async)]
pub fn app_save_log(state: State<'_, AppState>, action_id: String, log: LogInput) -> Result<ActionLog, String> {
    with_core(&state, |c| card::save_log(&c.conn, &mut c.clock, &action_id, log, Utc::now()))
}
