//! Replan status commands (Q77): what a replan run changed on device, and Undo.

use super::app::after_write;
use super::engine::sync_if_signed_in;
use super::with_core;
use crate::AppState;
use aroh_app::undo;
use aroh_domain::rows::Proposal;
use tauri::{AppHandle, State};

fn checked_run(text: &str) -> Result<&str, String> {
    let ok = text.len() == 36 && text.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    ok.then_some(text).ok_or_else(|| "invalid run id".to_string())
}

/// Lists the proposals replan run `run_id` wrote, any status, for "what moved".
#[tauri::command]
pub fn app_run_proposals(state: State<'_, AppState>, run_id: String) -> Result<Vec<Proposal>, String> {
    let id = checked_run(&run_id)?;
    with_core(&state, |c| undo::run_proposals(&c.conn, id))
}

/// Undoes every applied change of replan run `run_id`, reschedules reminders and
/// syncs so the server sees it. Returns how many actions were restored.
#[tauri::command]
pub fn app_undo_run(app: AppHandle, state: State<'_, AppState>, run_id: String) -> Result<usize, String> {
    let id = checked_run(&run_id)?;
    let restored = with_core(&state, |c| undo::undo_run(&c.conn, &mut c.clock, id))?;
    after_write(&app);
    sync_if_signed_in(&app);
    Ok(restored)
}
