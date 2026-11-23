//! Tauri command modules. Add `pub mod <name>;` here and list its commands in `lib.rs`.

pub mod app;
pub mod calendar_import;
pub mod engine;
pub mod plugins;
pub mod health;
pub mod learning;
pub mod location;
pub mod push;

use crate::{AppState, Core};

/// Runs `f` with the locked core state, turning every error into a UI-safe string.
pub fn with_core<T>(state: &tauri::State<'_, AppState>, f: impl FnOnce(&mut Core) -> aroh_app::Result<T>) -> Result<T, String> {
    let mut core = state.core.lock().map_err(|_| "database lock poisoned".to_string())?;
    f(&mut core).map_err(|e| e.to_string())
}
