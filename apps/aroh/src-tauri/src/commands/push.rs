//! Push registration (Q22): hands this iPhone's APNs token to the engine with what the
//! local schedule covers, so the engine pushes only what the device will not show itself.
//! The token is never stored on the device, logged or shown. A tapped run or proposal push
//! (Q83) is kept here and announced with `aroh://push-tap` so the webview opens its screen.

use super::engine::engine_call;
use super::with_core;
use crate::AppState;
use aroh_app::{local, remind};
use chrono::{SecondsFormat, Utc};
use serde_json::json;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_aroh_ios::IosExt;
use tauri_plugin_notification::{NotificationExt, PermissionState};

#[derive(serde::Deserialize)]
struct Token {
    token: String,
}

const TAP_RETRY_MAX_SECS: u64 = 60;

/// A tapped run or proposal push as Swift hands it over (Q83): its kind, the row id and the
/// goal id, `None` for a run without a goal. Ids are hyphenated uuids once `checked`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tap {
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub goal_id: Option<String>,
}

fn is_uuid(text: &str) -> bool {
    text.len() == 36 && uuid::Uuid::parse_str(text).is_ok()
}

impl Tap {
    /// The tap when its kind is `run` or `proposal` and its ids are hyphenated uuids.
    /// A proposal needs a goal. Anything else from the push payload gives `None`.
    fn checked(self) -> Option<Tap> {
        let kind_ok = self.kind == "run" || (self.kind == "proposal" && self.goal_id.is_some());
        (kind_ok && is_uuid(&self.id) && self.goal_id.as_deref().is_none_or(is_uuid)).then_some(self)
    }
}

/// The last tapped push the webview has not taken yet. A newer tap replaces an older one.
#[derive(Default)]
pub struct TapState(Mutex<Option<Tap>>);

/// On iPhone, starts the one task that waits for tapped pushes: each checked tap is stored
/// and announced with `aroh://push-tap`. Swift queues taps until asked, so a tap at cold
/// start is not lost. Errors back off up to a minute. Off iPhone it does nothing.
pub fn listen_taps(app: &AppHandle) {
    if cfg!(not(target_os = "ios")) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut retry = 1;
        loop {
            match app.ios().call::<Tap>("push_tap_next", ()).await {
                Ok(tap) => {
                    retry = 1;
                    let Some(tap) = tap.checked() else { continue };
                    *app.state::<TapState>().0.lock().unwrap_or_else(|p| p.into_inner()) = Some(tap);
                    if let Err(e) = app.emit("aroh://push-tap", ()) {
                        eprintln!("aroh: push tap event not emitted: {e}");
                    }
                }
                Err(e) => {
                    eprintln!("aroh: push taps: {}", e.chars().take(200).collect::<String>());
                    tokio::time::sleep(Duration::from_secs(retry)).await;
                    retry = (retry * 2).min(TAP_RETRY_MAX_SECS);
                }
            }
        }
    });
}

/// Returns the last tapped run or proposal push and clears it, so a screen opens it once.
/// `None` when nothing was tapped since the last call.
#[tauri::command(async)]
pub fn push_take_tap(state: tauri::State<'_, TapState>) -> Option<Tap> {
    state.0.lock().unwrap_or_else(|p| p.into_inner()).take()
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

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: &str = "0190f2a0-0000-7000-8000-000000000009";
    const GOAL: &str = "0190f2a0-0000-7000-8000-00000000000a";

    fn tap(kind: &str, id: &str, goal: Option<&str>) -> Tap {
        Tap { kind: kind.into(), id: id.into(), goal_id: goal.map(str::to_string) }
    }

    #[test]
    fn only_run_and_proposal_taps_with_uuid_ids_pass() {
        assert_eq!(tap("run", RUN, Some(GOAL)).checked(), Some(tap("run", RUN, Some(GOAL))));
        assert_eq!(tap("run", RUN, None).checked(), Some(tap("run", RUN, None)));
        assert_eq!(tap("proposal", RUN, Some(GOAL)).checked(), Some(tap("proposal", RUN, Some(GOAL))));
        assert_eq!(tap("reminder", RUN, Some(GOAL)).checked(), None);
        assert_eq!(tap("run", "x", Some(GOAL)).checked(), None);
        assert_eq!(tap("run", RUN, Some("../settings")).checked(), None);
        assert_eq!(tap("run", &RUN.replace('-', ""), None).checked(), None);
        let parsed: Tap = serde_json::from_str(&format!(r#"{{"kind":"run","id":"{RUN}"}}"#)).unwrap();
        assert_eq!(parsed.goal_id, None);
    }
}
