//! Diary commands (Q85, Q94): append, read and structure a day, and the agents read toggle.
//! The diary key lives in the iCloud Keychain through the iOS plugin and stays in Rust; the
//! engine holds a sealed copy and structures days with the owner's engine key, so the phone
//! never calls a model provider. Off iPhone every command answers `unavailable`.

use super::engine::{engine_call, engine_call_model, engine_error, sync_now};
use super::with_core;
use crate::AppState;
use aroh_app::diary::{self, DayView, Keyring};
use aroh_diary::{DiaryKey, Source, StructuredDay};
use aroh_domain::rows::Proposal;
use chrono::Utc;
use serde_json::json;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_aroh_ios::IosExt;

const KEYCHAIN_SERVICE: &str = "aroh";
const OLD_CLAUDE_DEVICE_KEY: &str = "claude_device_key";
const UNAVAILABLE: &str = "unavailable: the diary is on iPhone only";
const ENGINE_UNREACHABLE: &str = "The engine could not be reached. Your words are saved.";

/// A day as the diary screen shows it, with `structure` saying why no fresh card came:
/// `ok`, `no_key` (no provider key on the engine), `empty`, or a plain failure sentence.
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

/// Deletes the Claude key copy that builds before Q94 kept on the phone, on start, on
/// sign-out and on a device wipe. It is never read. A missing copy is fine; a keychain
/// failure is logged without detail.
pub fn forget_old_claude_key() {
    let gone = keyring::Entry::new(KEYCHAIN_SERVICE, OLD_CLAUDE_DEVICE_KEY).and_then(|e| match e.delete_credential() {
        Err(keyring::Error::NoEntry) => Ok(()),
        other => other,
    });
    if gone.is_err() {
        eprintln!("aroh: an old Claude key copy was not deleted");
    }
}

/// Sends the phone's newest diary key to the engine vault, keeping or setting the agents read
/// toggle (`agents` none keeps it), unless the engine already holds that version and no toggle
/// change is asked. When the engine refuses because its rows use a newer version, syncs once and
/// tries again. Returns the engine's status body, or a plain sentence.
async fn send_key(app: &AppHandle, state: &State<'_, AppState>, keys: &Keyring, agents: Option<bool>) -> Result<serde_json::Value, String> {
    let current = keys.newest();
    if agents.is_none() {
        let answer = engine_call(state, "GET", "/v1/diary/key", None).await.map_err(|_| ENGINE_UNREACHABLE.to_string())?;
        if answer.status == 200 && answer.body["version"].as_i64() == Some(current.version()) {
            return Ok(answer.body);
        }
    }
    let mut body = json!({"version": current.version(), "key": current.to_base64().as_str()});
    if let Some(on) = agents {
        body["agents"] = json!(on);
    }
    for attempt in 0..2 {
        let answer = engine_call(state, "PUT", "/v1/diary/key", Some(&body)).await.map_err(|_| ENGINE_UNREACHABLE.to_string())?;
        if answer.status == 200 {
            return Ok(answer.body);
        }
        if answer.status != 409 || attempt == 1 {
            return Err(if answer.status == 409 {
                "Your diary moved to a newer key on another device. Sync, then try again.".to_string()
            } else {
                engine_error(&answer, "The engine did not take your diary key. Try again.")
            });
        }
        sync_now(app, state).await?;
    }
    unreachable!("the loop returns on its second attempt")
}

/// The outcome of a failed structure call `answer`: `no_key` when the engine has no usable
/// provider key, else the engine's plain sentence.
fn structure_failure(answer: &aroh_sync::engine::Answer) -> String {
    if answer.body["error"]["code"] == "setup" {
        return "no_key".into();
    }
    engine_error(answer, "The engine could not structure this day. Your words are saved.")
}

/// Structures local `day` on the engine (Q94) when its card is stale: sends the sealed entries,
/// saves the sealed card that comes back. Returns `ok`, `empty`, `no_key` or a plain sentence;
/// the entries stay saved either way.
async fn structure(app: &AppHandle, state: &State<'_, AppState>, keys: &Keyring, day: &str) -> Result<String, String> {
    let (view, request) = with_core(state, |c| {
        let view = diary::read_day(&c.conn, keys, day)?;
        let request = if view.stale { diary::structure_request(&c.conn, &view)? } else { None };
        Ok((view, request))
    })?;
    let Some(request) = request else { return Ok(if view.entries.is_empty() { "empty" } else { "ok" }.into()) };
    send_key(app, state, keys, None).await?;
    let body = serde_json::to_value(&request).expect("a structure request serializes");
    let answer = engine_call_model(state, "/v1/diary/structure", &body).await.map_err(|_| ENGINE_UNREACHABLE.to_string())?;
    if answer.status != 200 {
        return Ok(structure_failure(&answer));
    }
    let Ok(structured) = serde_json::from_value::<StructuredDay>(answer.body) else {
        return Ok("The engine's answer could not be read. Your words are saved.".into());
    };
    with_core(state, |c| diary::save_card(&c.conn, &mut c.clock, keys, &view, structured, Utc::now()).map(|_| ()))?;
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

/// At start and on resume, on iPhone: deletes an old Claude key copy and, signed in,
/// finishes a key rotation that a crash or a lost network left half done. Failures are
/// logged without detail.
pub fn resume_rotation_if_unfinished(app: &AppHandle) {
    if !cfg!(target_os = "ios") {
        return;
    }
    forget_old_claude_key();
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
    day_answer(&state, &keys, &day, "ok".into()).await
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

/// Seals `text` as a new entry of `day` right away, then has the engine structure the day.
/// The entry is kept even when structuring fails.
#[tauri::command]
pub async fn app_diary_append(app: AppHandle, state: State<'_, AppState>, day: String, text: String, voice: bool) -> Result<DiaryDay, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    let source = if voice { Source::Voice } else { Source::Text };
    with_core(&state, |c| diary::append(&c.conn, &mut c.clock, &keys, &day, &text, source, Utc::now()).map(|_| ()))?;
    let outcome = structure(&app, &state, &keys, &day).await.unwrap_or_else(|e| e);
    day_answer(&state, &keys, &day, outcome).await
}

/// Structures `day` again, for the card's retry.
#[tauri::command]
pub async fn app_diary_structure(app: AppHandle, state: State<'_, AppState>, day: String) -> Result<DiaryDay, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    let outcome = structure(&app, &state, &keys, &day).await.unwrap_or_else(|e| e);
    day_answer(&state, &keys, &day, outcome).await
}

/// Reads the agents read toggle from the engine.
#[tauri::command]
pub async fn app_diary_status(app: AppHandle, state: State<'_, AppState>) -> Result<DiaryShare, String> {
    ios_only()?;
    let answer = engine_call(&state, "GET", "/v1/diary/key", None).await.map_err(|_| "The engine could not be reached. Try again.".to_string())?;
    if answer.status != 200 {
        return Err(engine_error(&answer, "The engine did not say whether agents read your diary. Try again."));
    }
    Ok(DiaryShare { on: answer.body["on"].as_bool().unwrap_or(false), version: answer.body["version"].as_i64(), held: held(&app, &state).await? })
}

/// Turns the agents read toggle on or off (Q94): sends the newest key with the choice to the
/// engine vault. The engine keeps its sealed copy either way, to structure days; the toggle
/// only decides whether planning agents read the diary. No key is rotated.
#[tauri::command]
pub async fn app_diary_share(app: AppHandle, state: State<'_, AppState>, on: bool) -> Result<DiaryShare, String> {
    ios_only()?;
    let keys = ensure_ring(&app, &state).await?;
    let status = send_key(&app, &state, &keys, Some(on)).await?;
    Ok(DiaryShare { on: status["on"].as_bool().unwrap_or(false), version: status["version"].as_i64(), held: held(&app, &state).await? })
}
