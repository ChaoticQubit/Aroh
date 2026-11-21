//! Tauri command modules, one per pod. Add `pub mod <pod>;` here and list its commands in `lib.rs`.

pub mod core;
pub mod plugins;

use crate::{AppState, Core};

/// Runs `f` with the locked core state, turning every error into a UI-safe string.
pub fn with_core<T>(
    state: &tauri::State<'_, AppState>,
    f: impl FnOnce(&mut Core) -> aroh_core::Result<T>,
) -> Result<T, String> {
    let mut core = state.core.lock().map_err(|_| "database lock poisoned".to_string())?;
    f(&mut core).map_err(|e| e.to_string())
}
