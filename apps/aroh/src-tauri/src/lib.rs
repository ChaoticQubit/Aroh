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

/// Reschedules reminders. Runs at app start and on resume.
fn on_start_or_resume(app: &tauri::AppHandle) {
    if let Err(e) = commands::calendar::sync_reminders(app) {
        eprintln!("aroh: sync_reminders failed: {e}");
    }
}

/// On resume, replans missed actions in the background; check_missed reschedules reminders itself.
/// At start the UI's ReplanWatcher calls check_missed once its event listeners exist.
#[cfg(mobile)]
fn on_resume(app: &tauri::AppHandle) {
    on_start_or_resume(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = commands::replan::check_missed(app).await {
            eprintln!("aroh: check_missed failed: {e}");
        }
    });
}

/// Debug builds only: saves `AROH_DEV_CLAUDE_KEY` to the keychain and, three seconds after start,
/// evaluates the JavaScript file at `AROH_DEV_SCRIPT` in the main window. Simulator checks use it
/// to drive the real UI where no tap tool exists. Release builds do not contain it.
#[cfg(debug_assertions)]
fn dev_hooks(app: &tauri::App) {
    if let Ok(key) = std::env::var("AROH_DEV_CLAUDE_KEY") {
        if let Err(e) = aroh_core::keychain::set_claude_api_key(key.trim()) {
            eprintln!("dev key not saved: {e}");
        }
    }
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
        .setup(|app| {
            let core = open_core(app)?;
            app.manage(AppState { core: Mutex::new(core) });
            app.manage(commands::calendar::ReminderState::default());
            if let Err(e) = commands::intake::sync_checkin(app.handle(), &app.state::<AppState>()) {
                eprintln!("daily check-in not scheduled: {e}");
            }
            on_start_or_resume(app.handle());
            #[cfg(debug_assertions)]
            dev_hooks(app);
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
            commands::calendar::calendar_list_actions,
            commands::calendar::calendar_set_status,
            commands::calendar::calendar_move_action,
            commands::calendar::calendar_get_reminder_settings,
            commands::calendar::calendar_set_reminder_settings,
            commands::calendar::calendar_ensure_notification_permission,
            commands::sync::sync_configure,
            commands::sync::sync_sign_in,
            commands::sync::sync_sign_out,
            commands::sync::sync_now,
            commands::sync::sync_status,
            commands::intake::intake_log_text,
            commands::intake::intake_log_chip,
            commands::intake::intake_list_today,
            commands::intake::intake_totals,
            commands::intake::intake_set_targets,
            commands::intake::intake_get_checkin,
            commands::intake::intake_set_checkin,
            commands::intake::intake_checkin_question,
            commands::plugins::plugins_list,
            commands::plugins::plugins_model,
            commands::plugins::plugins_set_enabled,
            commands::plugins::plugins_data,
            commands::replan::check_missed,
            commands::replan::replan_list_proposed,
            commands::replan::replan_apply,
            commands::replan::replan_reject,
            commands::replan::replan_undo,
            commands::goals::goals_create,
            commands::goals::goals_analyze,
            commands::goals::goals_plan,
            commands::goals::goals_approve,
            commands::goals::goals_detail,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {
            #[cfg(mobile)]
            if let tauri::RunEvent::WindowEvent { event: tauri::WindowEvent::Resumed, .. } = _event {
                on_resume(_app);
                commands::sync::on_resume(_app);
            }
        });
}
