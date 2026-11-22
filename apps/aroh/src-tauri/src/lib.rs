//! Aroh Tauri app: opens the rebuild store, runs the start-up tick and registers the commands.

mod commands;
mod notify;

use aroh_domain::hlc::{self, Hlc};
use chrono::Utc;
use chrono_tz::Tz;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;
use tauri::Manager;

/// Store connection, clock and local user shared by all commands. Lock `core` for every read or write.
pub struct AppState {
    pub core: Mutex<Core>,
}

/// The local SQLite connection, this device's clock and the local user id.
pub struct Core {
    pub conn: Connection,
    pub clock: Hlc,
    pub user_id: String,
}

/// The device's IANA zone, or UTC when the OS does not report a known one.
pub fn zone() -> Tz {
    iana_time_zone::get_timezone().ok().and_then(|name| name.parse().ok()).unwrap_or(Tz::UTC)
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
    let conn = aroh_store::db::open(&dir.join("aroh-store.sqlite3"))?;
    let clock = Hlc::new(load_node_id(&dir)?);
    let user_id = aroh_app::local::user_id(&conn)?;
    Ok(Core { conn, clock, user_id })
}

/// Runs the start-up tick (seed, materialize, mark missed) and reschedules reminders.
/// Runs at app start and on resume; the UI also calls `app_tick` when the date changes.
fn on_start_or_resume(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    if let Err(e) = commands::with_core(&state, |c| aroh_app::tick::tick(&c.conn, &mut c.clock, &c.user_id, Utc::now())) {
        eprintln!("aroh: tick failed: {e}");
    }
    if let Err(e) = notify::sync(app) {
        eprintln!("aroh: reminders not scheduled: {e}");
    }
    commands::engine::sync_if_signed_in(app);
    commands::health::import_if_enabled(app);
}

/// Debug builds only: three seconds after start, evaluates the JavaScript file at
/// `AROH_DEV_SCRIPT` in the main window. Simulator checks use it to drive the real UI
/// where no tap tool exists. Release builds do not contain it.
#[cfg(debug_assertions)]
fn dev_script(app: &tauri::App) {
    let Ok(path) = std::env::var("AROH_DEV_SCRIPT") else { return };
    let Some(window) = app.get_webview_window("main") else { return };
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        match std::fs::read_to_string(&path) {
            Ok(script) => drop(window.eval(&script)),
            Err(e) => eprintln!("dev script {path} not read: {e}"),
        }
    });
}

/// Builds and runs the app.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_aroh_speech::init())
        .plugin(tauri_plugin_aroh_ios::init())
        .setup(|app| {
            let core = open_core(app)?;
            app.manage(AppState { core: Mutex::new(core) });
            app.manage(notify::ReminderState::default());
            on_start_or_resume(app.handle());
            #[cfg(debug_assertions)]
            dev_script(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_snapshot,
            commands::app::app_tick,
            commands::app::app_set_action_status,
            commands::app::app_add_entry,
            commands::app::app_log_text,
            commands::app::app_decide_proposal,
            commands::app::app_decide_preference,
            commands::app::app_set_autonomy,
            commands::app::app_save_provider,
            commands::app::app_save_budget,
            commands::app::app_set_setting,
            commands::app::app_notification_permission,
            commands::app::app_dev_seed,
            commands::app::app_dev_mark,
            commands::engine::app_sign_in,
            commands::engine::app_sign_out,
            commands::engine::app_account,
            commands::engine::app_sync,
            commands::engine::app_engine,
            commands::engine::app_save_key,
            commands::engine::app_logs_sent,
            commands::health::app_health_connect,
            commands::health::app_dev_health_seed,
            commands::push::app_dev_push_probe,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {
            #[cfg(mobile)]
            if let tauri::RunEvent::WindowEvent { event: tauri::WindowEvent::Resumed, .. } = _event {
                on_start_or_resume(_app);
            }
        });
}
