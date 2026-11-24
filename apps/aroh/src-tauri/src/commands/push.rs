//! Push registration (Q22): hands this iPhone's APNs token to the engine with what the
//! local schedule covers, so the engine pushes only what the device will not show itself.
//! The token is never stored on the device, logged or shown.

use super::engine::engine_call;
use super::with_core;
use crate::AppState;
use aroh_app::{local, remind};
use chrono::{SecondsFormat, Utc};
use serde_json::json;
use tauri::{AppHandle, Manager};
use tauri_plugin_aroh_ios::IosExt;
use tauri_plugin_notification::{NotificationExt, PermissionState};

#[derive(serde::Deserialize)]
struct Token {
    token: String,
}

const ENVIRONMENT: &str = if cfg!(debug_assertions) { "sandbox" } else { "production" };

async fn try_register(app: &AppHandle) -> Result<(), String> {
    let Token { token } = app.ios().call("push_register", ()).await?;
    let granted = app.notification().permission_state().ok() == Some(PermissionState::Granted);
    let state = app.state::<AppState>();
    let now = Utc::now();
    let zone = crate::zone();
    let (device_id, synced_seq, until) = with_core(&state, |c| {
        let planned = remind::plan(&c.conn, &c.user_id, now, zone)?;
        Ok((local::device_id(&c.conn)?, remind::synced_seq(&c.conn, &c.user_id)?, remind::scheduled_until(&planned, now, granted)))
    })?;
    let body = json!({
        "platform": "ios",
        "token": token,
        "environment": ENVIRONMENT,
        "zone": zone.name(),
        "synced_seq": synced_seq,
        "scheduled_until": until.to_rfc3339_opts(SecondsFormat::Secs, true),
    });
    let answer = engine_call(&state, "PUT", &format!("/v1/devices/{device_id}"), Some(&body)).await?;
    if answer.status != 200 {
        return Err(format!("engine answered {}", answer.status));
    }
    Ok(())
}

/// Registers this device's push token with the engine after a sync, with the lowest pull
/// cursor and how far local reminders reach. Off iPhone, signed out or offline it does
/// nothing; failures are logged without the token.
pub async fn register(app: &AppHandle) {
    if cfg!(not(target_os = "ios")) {
        return;
    }
    if let Err(e) = try_register(app).await {
        eprintln!("aroh: push registration skipped: {}", e.chars().take(200).collect::<String>());
    }
}

/// Debug builds only: asks iOS for the push token and reports its length or the error,
/// never the token itself, so simulator checks can prove the delegate hook works.
#[tauri::command]
pub async fn app_dev_push_probe(app: AppHandle) -> Result<String, String> {
    if !cfg!(debug_assertions) {
        return Err("debug builds only".into());
    }
    let Token { token } = app.ios().call("push_register", ()).await?;
    Ok(format!("token {} hex chars", token.len()))
}

/// Removes this device from the engine before sign-out, so the account's pushes stop
/// reaching it. A missing device or an unreachable engine is logged and ignored.
pub async fn unregister(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(device_id) = with_core(&state, |c| local::device_id(&c.conn)) else { return };
    match engine_call(&state, "DELETE", &format!("/v1/devices/{device_id}"), None).await {
        Ok(answer) if answer.status == 204 || answer.status == 404 => {}
        Ok(answer) => eprintln!("aroh: device not removed, engine answered {}", answer.status),
        Err(e) => eprintln!("aroh: device not removed: {e}"),
    }
}
