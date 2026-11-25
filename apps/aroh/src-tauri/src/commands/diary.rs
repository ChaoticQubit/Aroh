//! Diary commands (Q85): append, read and structure a day, and the agents read toggle.
//! The diary key lives in the iCloud Keychain through the iOS plugin and the device copy
//! of the Claude key in the device keychain; both stay in Rust and are never returned,
//! logged or synced. Off iPhone every command answers `unavailable`.

use super::engine::{engine_call, sync_now};
use super::with_core;
use crate::AppState;
use aroh_app::diary::{self, DayView, Keyring};
use aroh_diary::{DiaryKey, Source};
use aroh_domain::rows::Proposal;
use chrono::Utc;
use serde_json::json;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_aroh_ios::IosExt;

const KEYCHAIN_SERVICE: &str = "aroh";
const CLAUDE_DEVICE_KEY: &str = "claude_device_key";
const UNAVAILABLE: &str = "unavailable: the diary is on iPhone only";

/// A day as the diary screen shows it, with `structure` saying why no fresh card came:
/// `ok`, `no_key` (no Claude key on this phone), `empty`, or a short failure reason.
#[derive(serde::Serialize)]
pub struct DiaryDay {
    pub view: DayView,
    pub structure: String,
}

/// Whether the agents read toggle is on, the key version the server holds, and the
/// versions of the diary keys this phone holds (numbers only, never key bytes).
#[derive(serde::Serialize)]
pub struct DiaryShare {
    pub on: bool,
    pub version: Option<i64>,
    pub held: Vec<i64>,
}

async fn held(app: &AppHandle, state: &State<'_, AppState>) -> Result<Vec<i64>, String> {
    Ok(ring(app, state).await?.map(|r| r.all().iter().map(|k| k.version()).collect()).unwrap_or_default())
}

fn ios_only() -> Result<(), String> {
    if cfg!(target_os = "ios") {
        Ok(())
    } else {
        Err(UNAVAILABLE.into())
    }
}

#[derive(serde::Deserialize)]
struct KeyAnswer {
    #[serde(default)]
    key: Option<String>,
}

async fn key_get(app: &AppHandle, version: i64) -> Result<Option<DiaryKey>, String> {
    let answer: KeyAnswer = app.ios().call("diary_key_get", json!({ "version": version })).await?;
    match answer.key {
        Some(text) => DiaryKey::from_base64(version, &text).map(Some).map_err(|e| e.to_string()),
        None => Ok(None),
    }
}

async fn key_put(app: &AppHandle, key: &DiaryKey) -> Result<(), String> {
    let _: serde_json::Value = app.ios().call("diary_key_put", json!({ "version": key.version(), "key": key.to_base64().as_str() })).await?;
    Ok(())
}

async fn key_delete(app: &AppHandle, version: i64) -> Result<(), String> {
    let _: serde_json::Value = app.ios().call("diary_key_delete", json!({ "version": version })).await?;
    Ok(())
}

/// Every diary key this phone holds, probing versions 1 to one above the newest row's.
/// Returns None when it holds none.
async fn ring(app: &AppHandle, state: &State<'_, AppState>) -> Result<Option<Keyring>, String> {
    let newest = with_core(state, |c| diary::newest_version(&c.conn))?;
    let mut keys = Vec::new();
    for version in 1..=newest.max(0) + 1 {
        if let Some(k) = key_get(app, version).await? {
            keys.push(k);
        }
    }
    if keys.is_empty() {
        return Ok(None);
    }
    Keyring::new(keys).map(Some).map_err(|e| e.to_string())
}

/// The phone's diary keys, creating key 1 only when the keychain holds none and a
/// fresh pull shows no diary rows (another device would have made one). Errors
/// `locked` when rows exist but the key has not arrived through iCloud Keychain yet.
async fn ensure_ring(app: &AppHandle, state: &State<'_, AppState>) -> Result<Keyring, String> {
    with_core(state, |c| diary::account(&c.conn).map(|_| ()))?;
    if let Some(r) = ring(app, state).await? {
        return Ok(r);
    }
    sync_now(app, state).await?;
    if let Some(r) = ring(app, state).await? {
        return Ok(r);
    }
    if with_core(state, |c| diary::newest_version(&c.conn))? > 0 {
        return Err("locked: the diary key has not reached this phone yet; check iCloud Keychain".into());
    }
    key_put(app, &DiaryKey::generate(1).map_err(|e| e.to_string())?).await?;
    ring(app, state).await?.ok_or_else(|| "the diary key could not be kept".to_string())
}

fn claude_key() -> Option<String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, CLAUDE_DEVICE_KEY).ok()?.get_password().ok()
}

/// Keeps a device copy of the owner's Claude key for structuring the diary, from the key
/// save path after the engine accepted it. Never returned or logged. Off iPhone it does nothing.
pub fn keep_claude_key(key: &str) {
    if !cfg!(target_os = "ios") {
        return;
    }
    let saved = keyring::Entry::new(KEYCHAIN_SERVICE, CLAUDE_DEVICE_KEY).and_then(|e| e.set_password(key.trim()));
    if saved.is_err() {
        eprintln!("aroh: the device copy of the Claude key was not kept");
    }
}

/// Deletes the device copy of the Claude key, on sign-out and on a device wipe.
/// A missing copy is fine; a keychain failure is logged without detail.
pub fn forget_claude_key() {
    let gone = keyring::Entry::new(KEYCHAIN_SERVICE, CLAUDE_DEVICE_KEY).and_then(|e| match e.delete_credential() {
        Err(keyring::Error::NoEntry) => Ok(()),
        other => other,
    });
    if gone.is_err() {
        eprintln!("aroh: the device copy of the Claude key was not deleted");
    }
}

async fn structure(state: &State<'_, AppState>, keys: &Keyring, day: &str) -> Result<String, String> {
    let (view, prompt) = with_core(state, |c| {
        let view = diary::read_day(&c.conn, keys, day)?;
        let prompt = if view.stale { diary::prompt_for(&c.conn, &view)? } else { None };
        Ok((view, prompt))
    })?;
    let Some((prompt, fields)) = prompt else { return Ok(if view.entries.is_empty() { "empty" } else { "ok" }.into()) };
    let Some(api_key) = claude_key() else { return Ok("no_key".into()) };
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).build().map_err(|_| "no HTTP client".to_string())?;
    let response = http
        .post(diary::ANTHROPIC_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", diary::ANTHROPIC_VERSION)
        .json(&diary::structure_request(&prompt))
        .send()
        .await
        .map_err(|_| "the model could not be reached".to_string())?;
    let status = response.status().as_u16();
    if status != 200 {
        return Ok(format!("the model answered {status}"));
    }
    let reply: serde_json::Value = response.json().await.map_err(|_| "the model reply was not JSON".to_string())?;
    let text = match diary::reply_text(&reply) {
        Ok(t) => t,
        Err(e) => return Ok(e.to_string()),
    };
    with_core(state, |c| diary::save_structure(&c.conn, &mut c.clock, keys, &view, &fields, &text, Utc::now()).map(|_| ()))?;
    Ok("ok".into())
}

async fn day_answer(state: &State<'_, AppState>, keys: &Keyring, day: &str, outcome: String) -> Result<DiaryDay, String> {
    let view = with_core(state, |c| diary::read_day(&c.conn, keys, day))?;
    Ok(DiaryDay { view, structure: outcome })
}

/// Moves the rows of every older key this phone holds to the newest one, syncs, and
/// deletes each old key once no row is left at its version.
async fn finish_rotation(app: &AppHandle, state: &State<'_, AppState>, keys: &Keyring) -> Result<(), String> {
    with_core(state, |c| diary::resume_rotation(&c.conn, &mut c.clock, keys).map(|_| ()))?;
    sync_now(app, state).await?;
    for old in keys.all().iter().filter(|k| k.version() < keys.newest().version()) {
        if with_core(state, |c| diary::rows_at(&c.conn, old.version()))? == 0 {
            key_delete(app, old.version()).await?;
        }
    }
    Ok(())
}

/// At start and on resume, on iPhone and signed in: finishes a key rotation that a
/// crash or a lost network left half done. Failures are logged without detail.
pub fn resume_rotation_if_unfinished(app: &AppHandle) {
    if !cfg!(target_os = "ios") {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if with_core(&state, |c| diary::account(&c.conn)).is_err() {
            return;
        }
        let Ok(Some(keys)) = ring(&app, &state).await else { return };
        if !matches!(with_core(&state, |c| diary::rotation_unfinished(&c.conn, &keys)), Ok(true)) {
            return;
        }
        if finish_rotation(&app, &state, &keys).await.is_err() {
            eprintln!("aroh: the diary key rotation is not finished yet");
        }
    });
}

/// A diary run's proposal opened for display: the real summary and the diff.
#[derive(serde::Serialize)]
pub struct DiaryProposal {
    pub summary: Option<String>,
    pub diff: Option<serde_json::Value>,
}

/// The opened diff of proposal `id` when it is sealed; None for an ordinary one.
/// Errors `unavailable` off iPhone for a sealed diff.
pub(crate) async fn opened_diff(app: &AppHandle, state: &State<'_, AppState>, id: &str) -> Result<Option<serde_json::Value>, String> {
    let row: Proposal = with_core(state, |c| aroh_store::repo::get(&c.conn, id).map_err(Into::into))?;
    if row.diff.get("sealed").is_none() {
        return Ok(None);
    }
    ios_only()?;
    let keys = ensure_ring(app, state).await?;
    diary::proposal_diff(&keys, &row).map(Some).ok_or_else(|| "locked: this change was sealed with a diary key this phone lacks".to_string())
}

/// Opens a diary run's proposal `id` for display: its summary and diff, each None
/// when it is not sealed or this phone lacks the key.
#[tauri::command]
pub async fn app_diary_proposal(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<DiaryProposal, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    let row: Proposal = with_core(&state, |c| aroh_store::repo::get(&c.conn, &id).map_err(Into::into))?;
    Ok(DiaryProposal { summary: diary::proposal_summary(&keys, &row), diff: diary::proposal_diff(&keys, &row) })
}

/// Reads local `day` (YYYY-MM-DD): its entries, the card and whether it is stale.
#[tauri::command]
pub async fn app_diary_day(app: AppHandle, state: State<'_, AppState>, day: String) -> Result<DiaryDay, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    let outcome = if claude_key().is_some() { "ok" } else { "no_key" };
    day_answer(&state, &keys, &day, outcome.into()).await
}

/// Reads up to 31 local `days` at once, for the past days under today. Days that
/// fail to open come back with their rows counted as locked.
#[tauri::command]
pub async fn app_diary_days(app: AppHandle, state: State<'_, AppState>, days: Vec<String>) -> Result<Vec<DayView>, String> {
    ios_only()?;
    if days.len() > 31 {
        return Err("at most 31 days at once".into());
    }
    let keys = ensure_ring(&app, &state).await?;
    with_core(&state, |c| days.iter().map(|d| diary::read_day(&c.conn, &keys, d)).collect())
}

/// Seals `text` as a new entry of `day` right away, then structures the day with the
/// device copy of the Claude key. The entry is kept even when structuring fails.
#[tauri::command]
pub async fn app_diary_append(app: AppHandle, state: State<'_, AppState>, day: String, text: String, voice: bool) -> Result<DiaryDay, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    let source = if voice { Source::Voice } else { Source::Text };
    with_core(&state, |c| diary::append(&c.conn, &mut c.clock, &keys, &day, &text, source, Utc::now()).map(|_| ()))?;
    let outcome = structure(&state, &keys, &day).await.unwrap_or_else(|e| e);
    day_answer(&state, &keys, &day, outcome).await
}

/// Structures `day` again, for the card's retry.
#[tauri::command]
pub async fn app_diary_structure(app: AppHandle, state: State<'_, AppState>, day: String) -> Result<DiaryDay, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    let outcome = structure(&state, &keys, &day).await.unwrap_or_else(|e| e);
    day_answer(&state, &keys, &day, outcome).await
}

/// Reads the agents read toggle from the engine.
#[tauri::command]
pub async fn app_diary_status(app: AppHandle, state: State<'_, AppState>) -> Result<DiaryShare, String> {
    ios_only()?;
    let answer = engine_call(&state, "GET", "/v1/diary/key", None).await?;
    if answer.status != 200 {
        return Err(format!("the engine answered {}", answer.status));
    }
    Ok(DiaryShare { on: answer.body["on"].as_bool().unwrap_or(false), version: answer.body["version"].as_i64(), held: held(&app, &state).await? })
}

/// Turns the agents read toggle on (sends the newest key to the engine vault) or off:
/// deletes the server copy, rotates the rows of every older key this phone holds to a new
/// key, syncs, and deletes each old key only once a pull shows no row left at its version.
#[tauri::command]
pub async fn app_diary_share(app: AppHandle, state: State<'_, AppState>, on: bool) -> Result<DiaryShare, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    let current = keys.newest();
    if on {
        let body = json!({"version": current.version(), "key": current.to_base64().as_str()});
        let answer = engine_call(&state, "PUT", "/v1/diary/key", Some(&body)).await?;
        if answer.status != 200 {
            return Err(format!("the engine answered {}", answer.status));
        }
        return Ok(DiaryShare { on: true, version: Some(current.version()), held: held(&app, &state).await? });
    }
    let answer = engine_call(&state, "DELETE", "/v1/diary/key", None).await?;
    if answer.status != 204 && answer.status != 404 {
        return Err(format!("the engine answered {}", answer.status));
    }
    let next = DiaryKey::generate(current.version() + 1).map_err(|e| e.to_string())?;
    key_put(&app, &next).await?;
    let next = key_get(&app, next.version()).await?.ok_or("the new diary key could not be kept")?;
    let mut old: Vec<&DiaryKey> = keys.all().iter().collect();
    for _ in 0..3 {
        for key in &old {
            with_core(&state, |c| diary::rotate(&c.conn, &mut c.clock, key, &next).map(|_| ()))?;
        }
        sync_now(&app, &state).await?;
        let mut left = Vec::new();
        for key in old {
            if with_core(&state, |c| diary::rows_at(&c.conn, key.version()))? == 0 {
                key_delete(&app, key.version()).await?;
            } else {
                left.push(key);
            }
        }
        if left.is_empty() {
            break;
        }
        old = left;
    }
    Ok(DiaryShare { on: false, version: None, held: held(&app, &state).await? })
}
