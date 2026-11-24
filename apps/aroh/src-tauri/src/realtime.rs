//! Realtime wake-ups for run views: one background task keeps a Supabase Realtime socket on
//! the owner's `run_events` while signed in. It emits `aroh://run` with the run id per insert
//! and `aroh://realtime` with `{ up }` when the channel joins or drops. Screens keep polling as
//! the fallback, so a missed event only delays a view.

use crate::AppState;
use aroh_sync::realtime::{self, Event};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

const RETRY_MIN_SECS: u64 = 2;
const RETRY_MAX_SECS: u64 = 30;

/// Whether the channel is joined now, and the signal that makes the task reconnect at once
/// (sign-in, sign-out).
#[derive(Default)]
pub struct RealtimeState {
    up: AtomicBool,
    restart: Notify,
}

/// Starts the background task once, at app start. It never ends; it waits while signed out.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { run(app).await });
}

/// Drops the current socket and connects again with the stored session, if any.
pub fn restart(app: &AppHandle) {
    app.state::<RealtimeState>().restart.notify_one();
}

/// Whether the Realtime channel is joined, for screens that start after the last change event.
#[tauri::command]
pub fn app_realtime_up(state: tauri::State<'_, RealtimeState>) -> bool {
    state.up.load(Ordering::SeqCst)
}

fn set_up(app: &AppHandle, up: bool) {
    let state = app.state::<RealtimeState>();
    if state.up.swap(up, Ordering::SeqCst) != up {
        if let Err(e) = app.emit("aroh://realtime", serde_json::json!({ "up": up })) {
            eprintln!("aroh: realtime event not emitted: {e}");
        }
    }
}

async fn run(app: AppHandle) {
    let state = app.state::<RealtimeState>();
    let mut retry = RETRY_MIN_SECS;
    loop {
        let joined = AtomicBool::new(false);
        let restarted = tokio::select! {
            r = connect(&app, &joined) => {
                if let Err(e) = r {
                    eprintln!("aroh: realtime: {e}");
                }
                false
            }
            _ = state.restart.notified() => true,
        };
        set_up(&app, false);
        if restarted {
            retry = RETRY_MIN_SECS;
            continue;
        }
        if joined.load(Ordering::SeqCst) {
            retry = RETRY_MIN_SECS;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(retry)) => {}
            _ = state.restart.notified() => {}
        }
        retry = (retry * 2).min(RETRY_MAX_SECS);
    }
}

/// One socket session: waits while signed out or unconfigured, else listens until the socket
/// ends. Sets `joined` once the channel joined. Errors are UI-safe strings without tokens.
async fn connect(app: &AppHandle, joined: &AtomicBool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let Ok(config) = crate::commands::engine::supabase(&state) else {
        std::future::pending::<()>().await;
        return Ok(());
    };
    if !matches!(aroh_sync::auth::stored(), Ok(Some(_))) {
        std::future::pending::<()>().await;
    }
    let session = aroh_sync::auth::valid_session(&config).await.map_err(|e| e.to_string())?;
    realtime::listen(&config, &session.user_id, &session.access_token, |event| match event {
        Event::Up => {
            joined.store(true, Ordering::SeqCst);
            set_up(app, true);
        }
        Event::Run(run_id) => {
            if let Err(e) = app.emit("aroh://run", serde_json::json!({ "run_id": run_id })) {
                eprintln!("aroh: run event not emitted: {e}");
            }
        }
    })
    .await
    .map_err(|e| e.to_string())
}
