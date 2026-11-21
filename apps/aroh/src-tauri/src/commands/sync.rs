//! Sync commands: Supabase project config, sign-in, sign-out, manual and resume-triggered sync, status.

use crate::AppState;
use aroh_sync::{Error, SyncReport};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Sync state for the settings screen. Carries no tokens or keys.
#[derive(Serialize)]
pub struct SyncView {
    configured: bool,
    #[serde(flatten)]
    status: aroh_sync::SyncStatus,
}

fn ui(e: Error) -> String {
    e.to_string()
}

fn configured() -> Result<bool, String> {
    match aroh_sync::config::load() {
        Ok(_) => Ok(true),
        Err(Error::NotConfigured) => Ok(false),
        Err(e) => Err(ui(e)),
    }
}

fn view(state: &State<'_, AppState>) -> Result<SyncView, String> {
    let configured = configured()?;
    let status = {
        let core = state.core.lock().map_err(|_| "database lock poisoned".to_string())?;
        aroh_sync::status(&core.conn).map_err(ui)?
    };
    Ok(SyncView { configured, status })
}

/// Validates and stores the Supabase project URL and publishable key in the keychain.
#[tauri::command]
pub fn sync_configure(state: State<'_, AppState>, url: String, anon_key: String) -> Result<SyncView, String> {
    let anon_key = anon_key.trim();
    if anon_key.is_empty() {
        return Err("publishable key is empty".into());
    }
    aroh_sync::configure(&url, anon_key).map_err(ui)?;
    view(&state)
}

/// Signs in with email and password and stores the session in the keychain.
#[tauri::command]
pub async fn sync_sign_in(state: State<'_, AppState>, email: String, password: String) -> Result<SyncView, String> {
    aroh_sync::sign_in_password(email.trim(), &password).await.map_err(ui)?;
    view(&state)
}

/// Signs out and removes the stored session.
#[tauri::command]
pub async fn sync_sign_out(state: State<'_, AppState>) -> Result<SyncView, String> {
    aroh_sync::sign_out().await.map_err(ui)?;
    view(&state)
}

/// Reports configuration, sign-in state and the last sync outcome.
#[tauri::command]
pub fn sync_status(state: State<'_, AppState>) -> Result<SyncView, String> {
    view(&state)
}

/// Pushes local changes and pulls remote ones now.
#[tauri::command]
pub async fn sync_now(app: AppHandle) -> Result<SyncReport, String> {
    run(app).await
}

/// Runs one sync on its own connection to the app database, so the shared
/// core lock is only taken briefly to merge each remote stamp into the clock.
/// Input: the app handle. Output: the sync report, or a UI-safe error.
pub async fn run(app: AppHandle) -> Result<SyncReport, String> {
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("aroh.sqlite3");
    let mut conn = aroh_core::db::open(&path).map_err(|e| e.to_string())?;
    conn.busy_timeout(BUSY_TIMEOUT).map_err(|e| e.to_string())?;
    let clock_app = app.clone();
    let mut on_remote = move |stamp: &str| {
        let state = clock_app.state::<AppState>();
        if let Ok(mut core) = state.core.lock() {
            let _ = core.clock.receive(stamp);
        };
    };
    aroh_sync::sync_now(&mut conn, &mut on_remote).await.map_err(ui)
}

/// Starts a background sync when a session is stored; does nothing when signed out or busy.
#[cfg(mobile)]
pub fn on_resume(app: &AppHandle) {
    if !matches!(aroh_sync::auth::stored(), Ok(Some(_))) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = run(app).await;
    });
}
