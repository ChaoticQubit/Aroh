//! Foundation commands: goal and action CRUD plus Claude API key presence.

use super::with_core;
use crate::AppState;
use aroh_core::models::{Action, Goal};
use aroh_core::{keychain, repo};
use serde::Serialize;
use tauri::State;

/// Lists live goals.
#[tauri::command]
pub fn list_goals(state: State<'_, AppState>) -> Result<Vec<Goal>, String> {
    with_core(&state, |c| repo::list_goals(&c.conn))
}

/// Inserts a goal with a fresh id and returns the stored row.
#[tauri::command]
pub fn create_goal(state: State<'_, AppState>, mut goal: Goal) -> Result<Goal, String> {
    goal.id = aroh_core::new_id();
    with_core(&state, |c| repo::insert_goal(&c.conn, &mut c.clock, &mut goal))?;
    Ok(goal)
}

/// Updates a live goal and returns the stored row.
#[tauri::command]
pub fn update_goal(state: State<'_, AppState>, mut goal: Goal) -> Result<Goal, String> {
    with_core(&state, |c| repo::update_goal(&c.conn, &mut c.clock, &mut goal))?;
    Ok(goal)
}

/// Soft-deletes a goal.
#[tauri::command]
pub fn delete_goal(state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_core(&state, |c| repo::soft_delete_goal(&c.conn, &mut c.clock, &id))
}

/// Lists live actions, optionally for one goal.
#[tauri::command]
pub fn list_actions(state: State<'_, AppState>, goal_id: Option<String>) -> Result<Vec<Action>, String> {
    with_core(&state, |c| repo::list_actions(&c.conn, goal_id.as_deref()))
}

/// Inserts an action with a fresh id and returns the stored row.
#[tauri::command]
pub fn create_action(state: State<'_, AppState>, mut action: Action) -> Result<Action, String> {
    action.id = aroh_core::new_id();
    with_core(&state, |c| repo::insert_action(&c.conn, &mut c.clock, &mut action))?;
    Ok(action)
}

/// Updates a live action and returns the stored row.
#[tauri::command]
pub fn update_action(state: State<'_, AppState>, mut action: Action) -> Result<Action, String> {
    with_core(&state, |c| repo::update_action(&c.conn, &mut c.clock, &mut action))?;
    Ok(action)
}

/// Soft-deletes an action.
#[tauri::command]
pub fn delete_action(state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_core(&state, |c| repo::soft_delete_action(&c.conn, &mut c.clock, &id))
}

/// Whether a Claude key is saved, with only its last 4 characters. The full key never leaves Rust.
#[derive(Serialize)]
pub struct KeyStatus {
    present: bool,
    last4: Option<String>,
}

fn key_status() -> Result<KeyStatus, String> {
    let key = keychain::claude_api_key().map_err(|e| e.to_string())?;
    let last4 = key.as_deref().map(|k| k.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect());
    Ok(KeyStatus { present: key.is_some(), last4 })
}

/// Reports whether a Claude API key is saved in the keychain.
#[tauri::command]
pub fn claude_key_status() -> Result<KeyStatus, String> {
    key_status()
}

/// Saves the Claude API key to the keychain and returns the new status.
#[tauri::command]
pub fn save_claude_key(key: String) -> Result<KeyStatus, String> {
    keychain::set_claude_api_key(&key).map_err(|e| e.to_string())?;
    key_status()
}

/// Removes the Claude API key from the keychain and returns the new status.
#[tauri::command]
pub fn clear_claude_key() -> Result<KeyStatus, String> {
    keychain::delete_claude_api_key().map_err(|e| e.to_string())?;
    key_status()
}
