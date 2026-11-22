//! Account, sync and engine commands: Supabase sign-in with the session in the keychain,
//! account adoption, LWW sync through PostgREST, and checked calls to the server engine.
//! Tokens and provider keys stay in Rust and are never logged or returned.

use super::with_core;
use crate::{notify, AppState};
use aroh_app::local::{self, PendingLog};
use aroh_domain::rows::ProviderSetting;
use aroh_store::repo;
use aroh_sync::engine::Answer;
use aroh_sync::{auth, Config};
use chrono::Utc;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, State};

/// The sign-in state Settings shows: whether a session is stored, its email and account id.
#[derive(serde::Serialize)]
pub struct Account {
    pub signed_in: bool,
    pub email: Option<String>,
    pub user_id: Option<String>,
}

/// Rows one sync moved, for the status line.
#[derive(serde::Serialize)]
pub struct Synced {
    pub pushed: usize,
    pub pulled: usize,
}

fn text(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The Supabase project from the owner's settings, or a UI-safe error when it is not set.
pub(crate) fn supabase(state: &State<'_, AppState>) -> Result<Config, String> {
    let (url, key) = with_core(state, |c| Ok((local::setting(&c.conn, "supabase_url")?, local::setting(&c.conn, "supabase_anon_key")?)))?;
    match (url, key) {
        (Some(url), Some(key)) => Config::new(&url, &key).map_err(text),
        _ => Err("Set the account URL and anon key first.".into()),
    }
}

fn account_of(session: Option<&auth::Session>) -> Account {
    Account { signed_in: session.is_some(), email: session.and_then(|s| s.email.clone()), user_id: session.map(|s| s.user_id.clone()) }
}

/// Runs one sync on a blocking thread that holds the core lock for its whole
/// length, then ticks and reschedules reminders. Errors are UI-safe strings.
fn sync_blocking(app: &AppHandle, config: Config, token: String) -> Result<Synced, String> {
    let state = app.state::<AppState>();
    let moved = with_core(&state, |c| {
        let mut remote = aroh_sync::rest::PgRest::new(config, token);
        let user = c.user_id.clone();
        let moved = tauri::async_runtime::block_on(aroh_store::sync::sync_all(&mut c.conn, &mut remote, &user, &mut c.clock))?;
        aroh_app::tick::tick(&c.conn, &mut c.clock, &c.user_id, Utc::now())?;
        Ok(moved)
    })?;
    if let Err(e) = notify::sync(app) {
        eprintln!("aroh: reminders not scheduled: {e}");
    }
    Ok(Synced { pushed: moved.pushed, pulled: moved.pulled })
}

/// Syncs in the background when a session and the account settings exist; used at
/// start and on resume. Failures are logged, never shown.
pub fn sync_if_signed_in(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(config) = supabase(&state) else { return };
    if !matches!(auth::stored(), Ok(Some(_))) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_sync(&app, config).await {
            eprintln!("aroh: sync failed: {e}");
        }
    });
}

async fn run_sync(app: &AppHandle, config: Config) -> Result<Synced, String> {
    let session = auth::valid_session(&config).await.map_err(text)?;
    let handle = app.clone();
    let synced = tauri::async_runtime::spawn_blocking(move || sync_blocking(&handle, config, session.access_token)).await.map_err(text)??;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move { super::push::register(&handle).await });
    Ok(synced)
}

/// Signs in (or with `create`, signs up) with email and password through GoTrue, stores
/// the session in the keychain, adopts the account id for every local row, then syncs.
/// The password is never stored. Returns the account state.
#[tauri::command]
pub async fn app_sign_in(app: AppHandle, state: State<'_, AppState>, email: String, password: String, create: bool) -> Result<Account, String> {
    if email.len() > 320 || password.is_empty() || password.len() > 256 {
        return Err("Enter an email and a password.".into());
    }
    let config = supabase(&state)?;
    let session = if create { auth::sign_up(&config, &email, &password).await } else { auth::sign_in_password(&config, &email, &password).await }.map_err(text)?;
    let adopted = with_core(&state, |c| {
        local::adopt(&mut c.conn, &session.user_id)?;
        c.user_id = session.user_id.clone();
        aroh_app::tick::tick(&c.conn, &mut c.clock, &c.user_id, Utc::now())?;
        Ok(())
    });
    if let Err(e) = adopted {
        let _ = auth::sign_out(Some(&config)).await;
        return Err(e);
    }
    crate::realtime::restart(&app);
    if let Err(e) = run_sync(&app, config).await {
        eprintln!("aroh: first sync failed: {e}");
    }
    Ok(account_of(Some(&session)))
}

/// Signs out: revokes the session when the server is reachable, removes it from the
/// keychain and clears the scheduled and delivered reminders. Local rows stay, owned by the
/// account; no reminders are planned for them until the account signs in again.
#[tauri::command]
pub async fn app_sign_out(app: AppHandle, state: State<'_, AppState>) -> Result<Account, String> {
    if cfg!(target_os = "ios") {
        super::push::unregister(&app).await;
    }
    let config = supabase(&state).ok();
    auth::sign_out(config.as_ref()).await.map_err(text)?;
    notify::clear(&app).await;
    crate::realtime::restart(&app);
    Ok(account_of(None))
}

/// The stored sign-in state, read from the keychain without a network call.
#[tauri::command]
pub fn app_account() -> Result<Account, String> {
    Ok(account_of(auth::stored().map_err(text)?.as_ref()))
}

/// Pushes local changes and pulls the account's rows (LWW), then reloads the screens.
#[tauri::command]
pub async fn app_sync(app: AppHandle, state: State<'_, AppState>) -> Result<Synced, String> {
    let config = supabase(&state)?;
    run_sync(&app, config).await
}

pub(crate) async fn engine_call(state: &State<'_, AppState>, method: &str, path: &str, body: Option<&Value>) -> Result<Answer, String> {
    let base = with_core(state, |c| local::setting(&c.conn, "server_url"))?.ok_or("needs_engine: set the server URL in Settings")?;
    let config = supabase(state).map_err(|e| format!("needs_engine: {e}"))?;
    let session = auth::valid_session(&config).await.map_err(|e| match e {
        aroh_sync::Error::NotSignedIn => "needs_engine: sign in first".to_string(),
        other => other.to_string(),
    })?;
    aroh_sync::engine::call(&base, &session.access_token, method, path, body).await.map_err(text)
}

/// One checked call to the server engine (`/v1/...` only) with the account's token.
/// Returns the status and JSON body. Errors starting with `needs_engine` mean no
/// server URL, no account config or no session.
#[tauri::command]
pub async fn app_engine(state: State<'_, AppState>, method: String, path: String, body: Option<Value>) -> Result<Answer, String> {
    engine_call(&state, &method, &path, body.as_ref()).await
}

/// Hands a provider key to the engine for the vendor's provider row (`PUT /v1/keys/{row id}`
/// with vendor, model and label). The key is dropped after the call and never logged,
/// stored or echoed. Returns the engine's answer (`last4` only).
#[tauri::command]
pub async fn app_save_key(state: State<'_, AppState>, vendor: String, key: String) -> Result<Answer, String> {
    if key.trim().len() < 8 || key.len() > 512 {
        return Err("A key is 8 to 512 characters.".into());
    }
    let row: ProviderSetting = with_core(&state, |c| {
        let rows = repo::list::<ProviderSetting>(&c.conn, &[("user_id", &c.user_id), ("vendor", &vendor)])?;
        rows.into_iter().next().ok_or_else(|| aroh_app::Error::Invalid("pick this provider first".into()))
    })?;
    let body = json!({"vendor": row.vendor, "key": key.trim(), "model": row.model, "label": row.label, "is_default": row.is_default});
    engine_call(&state, "PUT", &format!("/v1/keys/{}", row.id), Some(&body)).await
}

/// Removes the free-text logs the engine accepted from the local queue; logs queued
/// meanwhile stay. Returns how many were removed.
#[tauri::command]
pub fn app_logs_sent(app: AppHandle, state: State<'_, AppState>, sent: Vec<PendingLog>) -> Result<usize, String> {
    let removed = with_core(&state, |c| local::remove_sent(&c.conn, &sent))?;
    if let Err(e) = notify::sync(&app) {
        eprintln!("aroh: reminders not scheduled: {e}");
    }
    Ok(removed)
}
