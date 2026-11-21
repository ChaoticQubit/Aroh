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
/// them to the OS in the background. Call after every write and at start or resume.
/// Returns an error only when the database read fails.
pub fn sync(app: &AppHandle) -> Result<(), String> {
    if let Err(e) = app.emit("aroh://changed", ()) {
        eprintln!("aroh: changed event not emitted: {e}");
    }
    let state = app.state::<AppState>();
    let (planned, cancel_ids) = with_core(&state, |c| {
        Ok((aroh_app::remind::plan(&c.conn, &c.user_id, Utc::now(), crate::zone())?, aroh_app::remind::all_ids(&c.conn)?))
    })?;
    let reminders = app.state::<ReminderState>();
    let generation = reminders.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || deliver(&app, generation, planned, cancel_ids));
    Ok(())
}

#[cfg(mobile)]
fn deliver(app: &AppHandle, generation: u64, planned: Vec<Planned>, cancel_ids: Vec<i32>) {
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
    if !cancel_ids.is_empty() {
        if let Err(e) = notifications.cancel(cancel_ids) {
            eprintln!("aroh: cancel reminders failed: {e}");
        }
    }
    for p in planned {
        let Some(date) = plugin_fire_date(p.fire_at) else { continue };
        let mut builder = notifications.builder().id(p.id).title(p.title).body(p.body);
        if let Some(action_id) = &p.action_id {
            builder = builder.extra("action_id", action_id);
        }
        if let Err(e) = builder.schedule(Schedule::At { date, repeating: false, allow_while_idle: true }).show() {
            eprintln!("aroh: schedule reminder failed: {e}");
        }
    }
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
fn deliver(app: &AppHandle, generation: u64, planned: Vec<Planned>, _cancel_ids: Vec<i32>) {
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
