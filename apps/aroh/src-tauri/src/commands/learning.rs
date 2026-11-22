//! Learning commands: owner edits of actions. An edit locks its action; the server engine
//! records it when sync pushes the row and learns a preference from repeated similar edits.

use super::app::{after_write, parse_time};
use super::with_core;
use crate::AppState;
use aroh_app::write;
use aroh_domain::rows::Action;
use tauri::{AppHandle, State};

/// Moves or resizes action `id` to `start_at`..`end_at` (RFC 3339), or deletes it when
/// `delete` is true. The action is locked, then reminders are rescheduled.
#[tauri::command]
pub fn app_edit_action(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    start_at: Option<String>,
    end_at: Option<String>,
    delete: bool,
) -> Result<Action, String> {
    let span = match (delete, start_at, end_at) {
        (true, None, None) => None,
        (false, Some(s), Some(e)) => Some((parse_time(&s, "start_at")?, parse_time(&e, "end_at")?)),
        _ => return Err("send start_at and end_at, or delete alone".into()),
    };
    let action = with_core(&state, |c| write::edit_action(&c.conn, &mut c.clock, &id, span))?;
    after_write(&app);
    Ok(action)
}
