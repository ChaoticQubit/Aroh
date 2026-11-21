//! Aroh Tauri app: opens the local database and registers every pod's commands.

mod commands;

use aroh_core::hlc::{self, Hlc};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;
use tauri::Manager;

/// Database connection and HLC clock shared by all commands. Lock `core` for every read or write.
pub struct AppState {
    pub core: Mutex<Core>,
}

/// The local SQLite connection and this device's clock.
pub struct Core {
    pub conn: Connection,
    pub clock: Hlc,
}

fn load_node_id(dir: &Path) -> std::io::Result<String> {
    let path = dir.join("hlc_node_id");
    match std::fs::read_to_string(&path) {
        Ok(text) if hlc::parse(&format!("0000000000000-0000-{}", text.trim())).is_ok() => Ok(text.trim().to_string()),
        _ => {
            let node = hlc::random_node_id();
            std::fs::write(&path, &node)?;
            Ok(node)
        }
    }
}

fn open_core(app: &tauri::App) -> Result<Core, Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    let conn = aroh_core::db::open(&dir.join("aroh.sqlite3"))?;
    let clock = Hlc::new(load_node_id(&dir)?);
    Ok(Core { conn, clock })
}

/// Builds and runs the app.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let core = open_core(app)?;
            app.manage(AppState { core: Mutex::new(core) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::core::list_goals,
            commands::core::create_goal,
            commands::core::update_goal,
            commands::core::delete_goal,
            commands::core::list_actions,
            commands::core::create_action,
            commands::core::update_action,
            commands::core::delete_action,
            commands::core::claude_key_status,
            commands::core::save_claude_key,
            commands::core::clear_claude_key,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
