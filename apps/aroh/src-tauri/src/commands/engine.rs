//! Account, sync and engine commands: Supabase sign-in with the session in the keychain,
//! account adoption, LWW sync through PostgREST, and checked calls to the server engine.
//! Tokens and provider keys stay in Rust and are never logged or returned.

use super::with_core;
use crate::{notify, AppState, Core};
use aroh_app::local::{self, PendingLog};
use aroh_domain::rows::ProviderSetting;
use aroh_store::repo;
use aroh_sync::engine::Answer;
use aroh_sync::{auth, Config};
use chrono::Utc;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, State};

/// The sign-in state Settings shows: whether a session is stored, its email and account id.
/// `check_email` is true only right after a sign-up that needs the address confirmed: no
/// session yet, so the screen asks the user to open the email, then sign in.
#[derive(serde::Serialize)]
pub struct Account {
    pub signed_in: bool,
    pub email: Option<String>,
    pub user_id: Option<String>,
    pub check_email: bool,
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
    Account { signed_in: session.is_some(), email: session.and_then(|s| s.email.clone()), user_id: session.map(|s| s.user_id.clone()), check_email: false }
}

impl aroh_store::sync::Parts for Core {
    fn parts(&mut self) -> (&mut rusqlite::Connection, &mut aroh_domain::hlc::Hlc, &str) {
        (&mut self.conn, &mut self.clock, &self.user_id)
    }
}

/// Runs one sync on a blocking thread, then ticks and reschedules reminders. The sync
/// lock keeps two syncs apart for the whole run; the core lock is taken only per local
/// step, never across a network call. Errors are UI-safe strings.
fn sync_blocking(app: &AppHandle, config: Config, token: String) -> Result<Synced, String> {
    let state = app.state::<AppState>();
    let syncing = state.syncing.lock().map_err(|_| "sync lock poisoned".to_string())?;
    let user = with_core(&state, |c| Ok(c.user_id.clone()))?;
    let mut remote = aroh_sync::rest::PgRest::new(config, token);
    let mut store = aroh_store::sync::Locked::new(&state.core, user.clone());
    let moved = tauri::async_runtime::block_on(aroh_store::sync::sync_with(&mut store, &mut remote, &user)).map_err(text)?;
    drop(syncing);
    with_core(&state, |c| aroh_app::tick::tick(&c.conn, &mut c.clock, &c.user_id, Utc::now(), crate::zone()))?;
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
    let (storage, token) = (config.clone(), session.access_token.clone());
    let synced = tauri::async_runtime::spawn_blocking(move || sync_blocking(&handle, config, token)).await.map_err(text)??;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move { super::push::register(&handle).await });
    let handle = app.clone();
    tauri::async_runtime::spawn(async move { send_zone(&handle).await });
    let handle = app.clone();
    tauri::async_runtime::spawn(async move { remove_files(&handle, &storage, &session.access_token, &session.user_id).await });
    Ok(synced)
}

/// After a sync, removes the objects of deleted goal files from the owner's bucket with the
/// owner's session, in batches, and drops each removed batch from the device queue. A failed
/// batch stays queued for the next sync. Failures are logged.
async fn remove_files(app: &AppHandle, config: &Config, token: &str, user_id: &str) {
    let state = app.state::<AppState>();
    let Ok(paths) = with_core(&state, |c| local::pending_removals(&c.conn)) else { return };
    for batch in paths.chunks(aroh_sync::storage::REMOVE_BATCH) {
        if let Err(e) = aroh_sync::storage::remove(config, token, user_id, batch).await {
            eprintln!("aroh: files not removed: {}", e.to_string().chars().take(200).collect::<String>());
            return;
        }
        if let Err(e) = with_core(&state, |c| local::clear_removals(&c.conn, batch)) {
            eprintln!("aroh: removal queue not cleared: {e}");
            return;
        }
    }
}

/// After a sync, while a zone change is pending, moves the engine's weekly reviews to
/// the zone the rows follow (`PUT /v1/timezone`). The flag clears on 200 when the zone
/// did not change meanwhile; otherwise the next sync retries. Failures are logged.
async fn send_zone(app: &AppHandle) {
    let state = app.state::<AppState>();
    let pending = with_core(&state, |c| Ok(if local::zone_pending(&c.conn)? { local::zone(&c.conn)? } else { None }));
    let Ok(Some(zone)) = pending else { return };
    match engine_call(&state, "PUT", "/v1/timezone", Some(&json!({ "timezone": zone }))).await {
        Ok(answer) if answer.status == 200 => {
            let cleared = with_core(&state, |c| {
                if local::zone(&c.conn)?.as_deref() == Some(zone.as_str()) {
                    local::clear_zone_pending(&c.conn)?;
                }
                Ok(())
            });
            if let Err(e) = cleared {
                eprintln!("aroh: zone flag not cleared: {e}");
            }
        }
        Ok(answer) => eprintln!("aroh: zone not sent, engine answered {}", answer.status),
        Err(e) => eprintln!("aroh: zone not sent: {}", e.chars().take(200).collect::<String>()),
    }
}

/// Signs in (or with `create`, signs up) with email and password through GoTrue, stores
/// the session in the keychain, adopts the account id for every local row, then syncs.
/// The password is never stored. Returns the account state; after a sign-up that needs
/// email confirmation it is signed out with `check_email` set and nothing is adopted.
#[tauri::command]
pub async fn app_sign_in(app: AppHandle, state: State<'_, AppState>, email: String, password: String, create: bool) -> Result<Account, String> {
    if email.len() > 320 || password.is_empty() || password.len() > 256 {
        return Err("Enter an email and a password.".into());
    }
    let config = supabase(&state)?;
    let session = if create {
        match auth::sign_up(&config, &email, &password).await.map_err(text)? {
            auth::SignUp::SignedIn(session) => session,
            auth::SignUp::ConfirmEmail => {
                return Ok(Account { signed_in: false, email: Some(email.trim().to_string()), user_id: None, check_email: true });
            }
        }
    } else {
        auth::sign_in_password(&config, &email, &password).await.map_err(text)?
    };
    signed_in(&app, &state, config, session).await
}

/// Signs in with Apple (Q76): asks the iOS plugin for an Apple identity token bound to the
/// SHA-256 of a fresh nonce, trades it with the raw nonce for a GoTrue session, then adopts
/// the account and syncs like `app_sign_in`. Errors with the Apple or server reason.
#[tauri::command]
pub async fn app_sign_in_apple(app: AppHandle, state: State<'_, AppState>) -> Result<Account, String> {
    use tauri_plugin_aroh_ios::IosExt;
    #[derive(serde::Deserialize)]
    struct Apple {
        id_token: String,
    }
    let config = supabase(&state)?;
    let nonce = format!("{}{}", uuid::Uuid::now_v7().simple(), uuid::Uuid::now_v7().simple());
    let apple: Apple = app.ios().call("apple_sign_in", json!({ "nonce": nonce })).await?;
    let session = auth::sign_in_id_token(&config, "apple", &apple.id_token, &nonce).await.map_err(text)?;
    signed_in(&app, &state, config, session).await
}

/// Adopts the new session's account for every local row, restarts realtime and syncs;
/// signs out again when the adoption fails. Returns the account state.
async fn signed_in(app: &AppHandle, state: &State<'_, AppState>, config: Config, session: auth::Session) -> Result<Account, String> {
    let adopted = with_core(state, |c| {
        local::adopt(&mut c.conn, &session.user_id)?;
        c.user_id = session.user_id.clone();
        aroh_app::tick::tick(&c.conn, &mut c.clock, &c.user_id, Utc::now(), crate::zone())?;
        Ok(())
    });
    if let Err(e) = adopted {
        let _ = auth::sign_out(Some(&config)).await;
        return Err(e);
    }
    crate::realtime::restart(app);
    if let Err(e) = run_sync(app, config).await {
        eprintln!("aroh: first sync failed: {e}");
    }
    Ok(account_of(Some(&session)))
}

/// Signs out: revokes the session when the server is reachable, removes it from the
/// keychain with any Claude key copy an older build kept, and clears the scheduled and delivered reminders. Local rows stay, owned by the
/// account; no reminders are planned for them until the account signs in again.
#[tauri::command]
pub async fn app_sign_out(app: AppHandle, state: State<'_, AppState>) -> Result<Account, String> {
    if cfg!(target_os = "ios") {
        super::push::unregister(&app).await;
    }
    let config = supabase(&state).ok();
    auth::sign_out(config.as_ref()).await.map_err(text)?;
    super::diary::forget_old_claude_key();
    notify::clear(&app).await;
    crate::realtime::restart(&app);
    Ok(account_of(None))
}

fn wipe_device(state: &State<'_, AppState>) -> Result<(), String> {
    super::diary::forget_old_claude_key();
    with_core(state, |c| {
        local::wipe(&mut c.conn)?;
        c.user_id = local::user_id(&c.conn)?;
        aroh_app::tick::tick(&c.conn, &mut c.clock, &c.user_id, Utc::now(), crate::zone())?;
        Ok(())
    })
}

/// When a requested account deletion becomes final, for the confirm screen.
#[derive(serde::Serialize)]
pub struct DeletionRequested {
    pub purge_after: String,
}

/// Requests account deletion (App Store 5.1.1(v), Q76): the engine disables the account now
/// and deletes everything after 15 days unless the owner signs in again
/// (`DELETE /v1/account`); then this device drops the session and every local row. Nothing
/// local is touched unless the engine confirms. Returns when the deletion becomes final.
#[tauri::command]
pub async fn app_delete_account(app: AppHandle, state: State<'_, AppState>) -> Result<DeletionRequested, String> {
    let answer = engine_call(&state, "DELETE", "/v1/account", None).await?;
    let purge_after = answer.body["purge_after"].as_str().filter(|_| answer.status == 202).map(str::to_string);
    let Some(purge_after) = purge_after else {
        let message = answer.body["error"]["message"].as_str().unwrap_or("The account was not deleted. Try again.");
        return Err(message.chars().take(300).collect());
    };
    auth::sign_out(None).await.map_err(text)?;
    wipe_device(&state)?;
    notify::clear(&app).await;
    crate::realtime::restart(&app);
    Ok(DeletionRequested { purge_after })
}

/// Erases this device's Aroh data without touching the server: signs out locally and deletes
/// every local row, setting and queue. Used after account deletion and as a reset.
/// Returns signed out.
#[tauri::command]
pub async fn app_wipe_local(app: AppHandle, state: State<'_, AppState>) -> Result<Account, String> {
    auth::sign_out(None).await.map_err(text)?;
    wipe_device(&state)?;
    notify::clear(&app).await;
    crate::realtime::restart(&app);
    Ok(account_of(None))
}

/// The stored sign-in state, read from the keychain without a network call.
#[tauri::command(async)]
pub fn app_account() -> Result<Account, String> {
    Ok(account_of(auth::stored().map_err(text)?.as_ref()))
}

/// Runs one sync now and waits for it; the diary uses it before creating a key and
/// while rotating. Errors are UI-safe strings.
pub(crate) async fn sync_now(app: &AppHandle, state: &State<'_, AppState>) -> Result<(), String> {
    let config = supabase(state)?;
    run_sync(app, config).await.map(|_| ())
}

/// Pushes local changes and pulls the account's rows (LWW), then reloads the screens.
#[tauri::command]
pub async fn app_sync(app: AppHandle, state: State<'_, AppState>) -> Result<Synced, String> {
    let config = supabase(&state)?;
    run_sync(&app, config).await
}

async fn engine_target(state: &State<'_, AppState>) -> Result<(String, String), String> {
    let base = with_core(state, |c| local::setting(&c.conn, "server_url"))?.ok_or("needs_engine: set the server URL in Settings")?;
    let config = supabase(state).map_err(|e| format!("needs_engine: {e}"))?;
    let session = auth::valid_session(&config).await.map_err(|e| match e {
        aroh_sync::Error::NotSignedIn => "needs_engine: sign in first".to_string(),
        other => other.to_string(),
    })?;
    Ok((base, session.access_token))
}

pub(crate) async fn engine_call(state: &State<'_, AppState>, method: &str, path: &str, body: Option<&Value>) -> Result<Answer, String> {
    let (base, token) = engine_target(state).await?;
    aroh_sync::engine::call(&base, &token, method, path, body).await.map_err(text)
}

/// One engine call that makes a model turn on the engine (Q94: the phone never calls a
/// model provider), with the longer model wait. Returns the status and JSON body.
pub(crate) async fn engine_call_model(state: &State<'_, AppState>, path: &str, body: &Value) -> Result<Answer, String> {
    let (base, token) = engine_target(state).await?;
    aroh_sync::engine::call_model(&base, &token, "POST", path, Some(body)).await.map_err(text)
}

/// The owner-facing sentence of a failed engine `answer`: the engine's message for a refusal
/// code (consent, limits, `model_failed`, `diary_key_missing`), which is a plain sentence; else
/// `fallback`, since generic codes carry a technical prefix. Never a status.
pub(crate) fn engine_error(answer: &Answer, fallback: &str) -> String {
    const GENERIC: [&str; 7] = ["bad_request", "unauthorized", "forbidden", "not_found", "conflict", "setup", "internal"];
    let code = answer.body["error"]["code"].as_str().unwrap_or("internal");
    let message = answer.body["error"]["message"].as_str().filter(|m| !m.trim().is_empty() && !GENERIC.contains(&code)).unwrap_or(fallback);
    message.chars().take(300).collect()
}

/// One checked call to the server engine (`/v1/...` only) with the account's token.
/// Returns the status and JSON body. Errors starting with `needs_engine` mean no
/// server URL, no account config or no session.
#[tauri::command]
pub async fn app_engine(state: State<'_, AppState>, method: String, path: String, body: Option<Value>) -> Result<Answer, String> {
    engine_call(&state, &method, &path, body.as_ref()).await
}

/// Hands a provider key to the engine for the vendor's provider row (`PUT /v1/keys/{row id}`
/// with vendor, model and label). The phone keeps no copy (Q94); the key is never logged or
/// echoed. Returns the engine's answer (`last4` only).
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
#[tauri::command(async)]
pub fn app_logs_sent(app: AppHandle, state: State<'_, AppState>, sent: Vec<PendingLog>) -> Result<usize, String> {
    let removed = with_core(&state, |c| local::remove_sent(&c.conn, &sent))?;
    if let Err(e) = notify::sync(&app) {
        eprintln!("aroh: reminders not scheduled: {e}");
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_refusal_sentences_reach_the_owner() {
        let answer = |code: &str, message: &str| Answer { status: 400, body: json!({"error": {"code": code, "message": message}}) };
        assert_eq!(engine_error(&answer("model_failed", "The AI provider did not answer."), "Fallback."), "The AI provider did not answer.");
        assert_eq!(engine_error(&answer("consent_required", "Allow it in Settings."), "Fallback."), "Allow it in Settings.");
        assert_eq!(engine_error(&answer("bad_request", "bad request: invalid JSON body"), "Fallback."), "Fallback.");
        assert_eq!(engine_error(&answer("internal", "internal error"), "Fallback."), "Fallback.");
        assert_eq!(engine_error(&Answer { status: 502, body: Value::Null }, "Fallback."), "Fallback.");
    }
}
