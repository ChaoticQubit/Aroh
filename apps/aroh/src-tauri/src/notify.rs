//! Local notification delivery (MVP code, rebuild planner): hands the planned reminders
//! to the OS on mobile, or shows them from a timer thread on desktop.

use crate::commands::with_core;
use crate::AppState;
use aroh_app::remind::Planned;
use chrono::Utc;
#[cfg(desktop)]
use chrono::Duration;
#[cfg(desktop)]
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(mobile)]
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

/// Serialises reminder delivery: holds the latest generation so stale schedules and desktop timers stop.
#[derive(Default)]
pub struct ReminderState {
    #[cfg(desktop)]
    generation: Arc<AtomicU64>,
    #[cfg(mobile)]
    generation: AtomicU64,
    #[cfg(mobile)]
    deliver: Mutex<()>,
}

/// Emits `aroh://changed` so open screens reload, replans the local notifications and hands
/// them to the OS in the background, replacing every pending one. Plans none while the store
/// belongs to an account that is signed out. Call after every write and at start or resume.
/// Returns an error only when the database read fails.
pub fn sync(app: &AppHandle) -> Result<(), String> {
    if let Err(e) = app.emit("aroh://changed", ()) {
        eprintln!("aroh: changed event not emitted: {e}");
    }
    let signed_in = matches!(aroh_sync::auth::stored(), Ok(Some(_)));
    let state = app.state::<AppState>();
    let planned = with_core(&state, |c| {
        if !aroh_app::remind::reminders_allowed(&c.conn, signed_in)? {
            return Ok(Vec::new());
        }
        aroh_app::remind::plan(&c.conn, &c.user_id, Utc::now(), crate::zone())
    })?;
    let reminders = app.state::<ReminderState>();
    let generation = reminders.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || deliver(&app, generation, planned));
    Ok(())
}

/// Stops every scheduled reminder and removes the delivered ones, then bumps the generation
/// so a delivery already running stops. Used at sign-out. Failures are logged, since a
/// sign-out must not fail on the notification center.
pub async fn clear(app: &AppHandle) {
    app.state::<ReminderState>().generation.fetch_add(1, Ordering::SeqCst);
    #[cfg(mobile)]
    {
        let app = app.clone();
        let cleared = tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<ReminderState>();
            let _guard = state.deliver.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(e) = cancel_pending(&app) {
                eprintln!("aroh: cancel reminders failed: {e}");
            }
            if let Err(e) = app.notification().remove_all_active() {
                eprintln!("aroh: remove delivered reminders failed: {e}");
            }
        });
        if let Err(e) = cleared.await {
            eprintln!("aroh: clear reminders failed: {e}");
        }
    }
}

#[cfg(mobile)]
fn deliver(app: &AppHandle, generation: u64, planned: Vec<Planned>) {
    use aroh_app::remind::PlannedKind;
    use tauri_plugin_notification::Schedule;
    let state = app.state::<ReminderState>();
    let _guard = state.deliver.lock().unwrap_or_else(|e| e.into_inner());
    if state.generation.load(Ordering::SeqCst) != generation {
        return;
    }
    let notifications = app.notification();
    if notifications.permission_state().ok() != Some(tauri_plugin_notification::PermissionState::Granted) {
        return;
    }
    if let Err(e) = cancel_pending(app) {
        eprintln!("aroh: cancel reminders failed: {e}");
    }
    for p in planned {
        let Some(date) = plugin_fire_date(p.fire_at) else { continue };
        let mut builder = notifications.builder().id(p.id).title(p.title).body(p.body);
        if p.kind == PlannedKind::Action {
            builder = builder.extra("action_id", &p.row_id);
        }
        if let Err(e) = builder.schedule(Schedule::At { date, repeating: false, allow_while_idle: true }).show() {
            eprintln!("aroh: schedule reminder failed: {e}");
        }
    }
}

/// Cancels every pending local notification through the app's own iOS plugin. The
/// notification plugin's `cancel_all` sends no arguments, which its iOS side (2.8.1) fails to
/// decode, and its `pending` list fails to parse. Call from a blocking thread only.
#[cfg(mobile)]
fn cancel_pending(app: &AppHandle) -> Result<(), String> {
    use tauri_plugin_aroh_ios::IosExt;
    tauri::async_runtime::block_on(app.ios().call::<serde_json::Value>("reminders_cancel", ())).map(|_| ())
}

/// The iOS plugin (2.7 and 2.8) parses the date as local wall-clock time and ignores the trailing `Z`,
/// so pass the instant shifted by the device's UTC offset at that instant.
/// Returns `None` if the shifted time is out of range.
#[cfg(mobile)]
fn plugin_fire_date(fire_at: chrono::DateTime<Utc>) -> Option<time::OffsetDateTime> {
    use chrono::Offset;
    let offset = fire_at.with_timezone(&chrono::Local).offset().fix().local_minus_utc();
    let date = time::OffsetDateTime::from_unix_timestamp(fire_at.timestamp() + i64::from(offset)).ok()?;
    assert_eq!(date.offset(), time::UtcOffset::UTC);
    Some(date)
}

#[cfg(desktop)]
fn deliver(app: &AppHandle, generation: u64, planned: Vec<Planned>) {
    let current = Arc::clone(&app.state::<ReminderState>().generation);
    for p in planned {
        loop {
            if current.load(Ordering::SeqCst) != generation {
                return;
            }
            let wait = p.fire_at - Utc::now();
            if wait <= Duration::zero() {
                break;
            }
            std::thread::sleep(wait.min(Duration::seconds(30)).to_std().unwrap_or_default());
        }
        if let Err(e) = app.notification().builder().title(p.title).body(p.body).show() {
            eprintln!("aroh: show reminder failed: {e}");
        }
    }
}
