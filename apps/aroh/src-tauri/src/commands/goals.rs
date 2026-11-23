//! Goals commands: the New Goal form's offline category guess and owner-defined categories,
//! per-goal controls, profile facts, and goal files read on device and stored in the owner's bucket.

use super::app::after_write;
use super::engine::{supabase, sync_if_signed_in};
use super::with_core;
use crate::{files, AppState};
use aroh_app::write;
use aroh_domain::rows::{Category, Goal, GoalFile, Preference};
use aroh_sync::{auth, storage};
use serde::Deserialize;
use tauri::{AppHandle, Manager, State};

/// The request header that carries a file's details as `encodeURIComponent(JSON)`.
const FILE_HEADER: &str = "x-aroh-file";

/// Details of a file sent to `attach_file`, beside its raw bytes.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileMeta {
    goal_id: String,
    name: String,
    mime: String,
}

/// Whether Attach can show: the owner is signed in and the project serves the goal-files
/// bucket (Q61). Any failure is false.
#[tauri::command]
pub async fn files_available(app: AppHandle) -> Result<bool, String> {
    let state = app.state::<AppState>();
    let Ok(config) = supabase(&state) else { return Ok(false) };
    let Ok(session) = auth::valid_session(&config).await else { return Ok(false) };
    Ok(storage::available(&config, &session.access_token).await)
}

/// Attaches the file in the raw request body to a goal: reads its text off the main thread
/// (a PDF that cannot be read is stored with ingest status failed), uploads the bytes to the
/// owner's folder, stores the row and syncs. Details come in the `x-aroh-file` header.
/// Output: the stored row without its text. Nothing is stored when the upload fails.
#[tauri::command]
pub async fn attach_file(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<GoalFile, String> {
    let meta = request.headers().get(FILE_HEADER).and_then(|v| v.to_str().ok()).and_then(files::percent_decode).ok_or("The file details are missing.")?;
    let meta: FileMeta = serde_json::from_str(&meta).map_err(|_| "The file details are not valid.".to_string())?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err("The file bytes are missing.".into()) };
    let bytes = bytes.clone();
    if bytes.len() > storage::MAX_BYTES {
        return Err(format!("Files can be up to {} MB.", storage::MAX_BYTES / 1024 / 1024));
    }
    let mime = if meta.mime.trim().is_empty() { "application/octet-stream".to_string() } else { meta.mime.trim().to_string() };
    let state = app.state::<AppState>();
    let config = supabase(&state)?;
    aroh_domain::rows::check_uuid("goal_id", &meta.goal_id).map_err(|e| e.to_string())?;
    let user = with_core(&state, |c| Ok(c.user_id.clone()))?;
    let (read_mime, read_name, data) = (mime.clone(), meta.name.clone(), bytes.clone());
    let read = tauri::async_runtime::spawn_blocking(move || files::extract(&read_mime, &read_name, &data)).await.map_err(|e| e.to_string())?;
    let file = write::stored_file(&user, &meta.goal_id, &meta.name, &mime, bytes.len() as i64, read);
    let path = file.storage_path.clone().expect("a stored file has a storage path");
    let session = auth::valid_session(&config).await.map_err(|e| e.to_string())?;
    if session.user_id != user {
        return Err("Sign in with the account that owns this goal.".into());
    }
    storage::upload(&config, &session.access_token, &session.user_id, &path, &mime, bytes).await.map_err(|e| format!("Could not upload the file: {e}"))?;
    let stored = with_core(&state, |c| write::add_goal_file(&c.conn, &mut c.clock, file))?;
    sync_if_signed_in(&app);
    Ok(GoalFile { extracted_text: None, ..stored })
}

/// Adds the `https` link `url` to goal `goal_id` for the agents to fetch, then syncs.
/// Output: the stored row.
#[tauri::command]
pub fn attach_link(app: AppHandle, state: State<'_, AppState>, goal_id: String, url: String) -> Result<GoalFile, String> {
    let file = with_core(&state, |c| write::add_goal_link(&c.conn, &mut c.clock, &c.user_id.clone(), &goal_id, &url))?;
    sync_if_signed_in(&app);
    Ok(file)
}

/// Removes goal file `id` from its goal, then syncs.
#[tauri::command]
pub fn remove_goal_file(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_core(&state, |c| write::remove_goal_file(&c.conn, &mut c.clock, &id))?;
    sync_if_signed_in(&app);
    Ok(())
}

/// Guesses the category of a new goal from `title` and `description` without a model call.
/// Output: the id of the owner's matching seeded category, or null.
#[tauri::command]
pub fn guess_category(state: State<'_, AppState>, title: String, description: Option<String>) -> Result<Option<String>, String> {
    with_core(&state, |c| Ok(write::guess_category(&c.user_id, &title, description.as_deref())))
}

/// Sets goal `goal_id`'s reminders, calendar reads and run time limit (null: the Settings
/// limit), then reschedules reminders. Output: the stored goal.
#[tauri::command]
pub fn set_goal_controls(
    app: AppHandle,
    state: State<'_, AppState>,
    goal_id: String,
    reminders: bool,
    use_calendars: bool,
    run_limit_minutes: Option<i64>,
) -> Result<Goal, String> {
    let goal = with_core(&state, |c| write::set_goal_controls(&c.conn, &mut c.clock, &goal_id, reminders, use_calendars, run_limit_minutes))?;
    after_write(&app);
    Ok(goal)
}

/// Edits profile fact `id` to `fact`, dated now. Output: the stored fact.
#[tauri::command]
pub fn edit_fact(state: State<'_, AppState>, id: String, fact: String) -> Result<Preference, String> {
    with_core(&state, |c| write::edit_fact(&c.conn, &mut c.clock, &id, &fact, chrono::Utc::now()))
}

/// "Never learn this": turns fact `id` into a blocked topic. Output: the stored row.
#[tauri::command]
pub fn block_topic(state: State<'_, AppState>, id: String) -> Result<Preference, String> {
    with_core(&state, |c| write::block_topic(&c.conn, &mut c.clock, &id))
}

/// "Allow again": removes blocked topic `id`, so it may be learned again.
#[tauri::command]
pub fn allow_topic(state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_core(&state, |c| write::allow_topic(&c.conn, &mut c.clock, &id))
}

/// Adds the owner's category `name` on `color` (one of the 6 hues), or on the next free
/// hue when `color` is null. Synced like every row. Output: the stored category.
#[tauri::command]
pub fn add_category(state: State<'_, AppState>, name: String, color: Option<String>) -> Result<Category, String> {
    with_core(&state, |c| write::add_category(&c.conn, &mut c.clock, &c.user_id, &name, color.as_deref()))
}
