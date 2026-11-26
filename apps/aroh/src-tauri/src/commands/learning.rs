//! Learning commands: owner edits of actions. An edit locks its action; the server engine
//! records it when sync pushes the row and learns a preference from repeated similar edits.

use super::app::{after_write, parse_time};
use super::engine::sync_if_signed_in;
use super::with_core;
use crate::AppState;
use aroh_app::write;
use aroh_domain::rows::Action;
use tauri::{AppHandle, State};

/// Moves or resizes action `id` to `start_at`..`end_at` (RFC 3339), or deletes it when
/// `delete` is true: with `scope` `future` it and every later session of its series (Q89),
/// else (`this` or none) only it. The action is locked, reminders are rescheduled and the
/// edit is pushed at once, so the server checks it for a replan within a minute (Q77).
#[tauri::command(async)]
pub fn app_edit_action(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    start_at: Option<String>,
    end_at: Option<String>,
    delete: bool,
    scope: Option<String>,
) -> Result<Action, String> {
    let future = match (delete, scope.as_deref()) {
        (_, None | Some("this")) => false,
        (true, Some("future")) => true,
        _ => return Err("scope is this or future, and only for a delete".into()),
    };
    let span = match (delete, start_at, end_at) {
        (true, None, None) => None,
        (false, Some(s), Some(e)) => Some((parse_time(&s, "start_at")?, parse_time(&e, "end_at")?)),
        _ => return Err("send start_at and end_at, or delete alone".into()),
    };
    let action = with_core(&state, |c| match future {
        true => write::delete_series_from(&c.conn, &mut c.clock, &id),
        false => write::edit_action(&c.conn, &mut c.clock, &id, span),
    })?;
    after_write(&app);
    sync_if_signed_in(&app);
    Ok(action)
}

/// Creates a one-off action from a Calendar slot tap (B121): `title` at `start_at` (RFC 3339)
/// for `minutes`, under `goal_id` or the built-in Routines goal. It is locked, reminders are
/// rescheduled and it is pushed at once like an edit. Output: the stored action.
#[tauri::command(async)]
pub fn app_create_action(
    app: AppHandle,
    state: State<'_, AppState>,
    title: String,
    start_at: String,
    minutes: i64,
    goal_id: Option<String>,
) -> Result<Action, String> {
    let start = parse_time(&start_at, "start_at")?;
    let action = with_core(&state, |c| write::create_action(&c.conn, &mut c.clock, &c.user_id, &title, start, minutes, goal_id.as_deref()))?;
    after_write(&app);
    sync_if_signed_in(&app);
    Ok(action)
}

/// Turns Fixed time on or off for action `id` (the edit sheet and plan approval switch),
/// then reschedules reminders. Output: the stored action.
#[tauri::command(async)]
pub fn app_set_locked(app: AppHandle, state: State<'_, AppState>, id: String, locked: bool) -> Result<Action, String> {
    let action = with_core(&state, |c| write::set_locked(&c.conn, &mut c.clock, &id, locked))?;
    after_write(&app);
    Ok(action)
}
