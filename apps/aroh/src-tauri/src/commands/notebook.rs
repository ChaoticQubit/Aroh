//! Notebook commands (Q96, B126): open, create, save, rename, move and delete pages, the
//! page tree and search. Pages and blocks are sealed with the diary key, which stays in
//! Rust; invoke arguments are snake_case. Off iPhone every command answers `unavailable`.

use super::diary::{ensure_ring, ios_only};
use super::with_core;
use crate::AppState;
use aroh_app::notebook::{self, Block, PageView, SearchHit, TreeRow};
use chrono::Utc;
use tauri::{AppHandle, State};

/// Opens the page of `kind` ('page', 'day', 'action' or 'goal') and `target` (page id,
/// YYYY-MM-DD, action id or goal id), creating a day, action or goal page on first open.
#[tauri::command(rename_all = "snake_case")]
pub async fn app_page_get(app: AppHandle, state: State<'_, AppState>, kind: String, target: String) -> Result<PageView, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    with_core(&state, |c| notebook::get(&c.conn, &mut c.clock, &keys, &kind, &target, Utc::now()))
}

/// Makes a free page titled `title` under `parent_id` (None for the top level).
#[tauri::command(rename_all = "snake_case")]
pub async fn app_page_create(app: AppHandle, state: State<'_, AppState>, parent_id: Option<String>, title: String) -> Result<PageView, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    with_core(&state, |c| notebook::create(&c.conn, &mut c.clock, &keys, parent_id.as_deref(), &title, Utc::now()))
}

/// Saves the full ordered `blocks` of page `page_id`, diffed per block.
#[tauri::command(rename_all = "snake_case")]
pub async fn app_page_save(app: AppHandle, state: State<'_, AppState>, page_id: String, blocks: Vec<Block>) -> Result<PageView, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    with_core(&state, |c| notebook::save(&c.conn, &mut c.clock, &keys, &page_id, blocks, Utc::now()))
}

/// Renames page `page_id` to `title`.
#[tauri::command(rename_all = "snake_case")]
pub async fn app_page_rename(app: AppHandle, state: State<'_, AppState>, page_id: String, title: String) -> Result<PageView, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    with_core(&state, |c| notebook::rename(&c.conn, &mut c.clock, &keys, &page_id, &title))
}

/// Moves page `page_id` under `parent_id` (None for the top level); refuses cycles.
#[tauri::command(rename_all = "snake_case")]
pub async fn app_page_move(app: AppHandle, state: State<'_, AppState>, page_id: String, parent_id: Option<String>) -> Result<PageView, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    with_core(&state, |c| notebook::move_page(&c.conn, &mut c.clock, &keys, &page_id, parent_id.as_deref()))
}

/// Deletes page `page_id` with its subpages and blocks. Returns null.
#[tauri::command(rename_all = "snake_case")]
pub async fn app_page_delete(state: State<'_, AppState>, page_id: String) -> Result<(), String> {
    ios_only()?;
    with_core(&state, |c| notebook::delete(&c.conn, &mut c.clock, &page_id))
}

/// Every live page as `{id, kind, title, parent_id, updated_at}`.
#[tauri::command(rename_all = "snake_case")]
pub async fn app_pages_tree(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<TreeRow>, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    with_core(&state, |c| notebook::tree(&c.conn, &keys))
}

/// Pages whose title or text holds `query`, at most 50.
#[tauri::command(rename_all = "snake_case")]
pub async fn app_pages_search(app: AppHandle, state: State<'_, AppState>, query: String) -> Result<Vec<SearchHit>, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    with_core(&state, |c| notebook::search(&c.conn, &keys, &query))
}
