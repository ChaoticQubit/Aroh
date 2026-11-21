//! Trackers commands: log food, water and workouts from text or chips, list today's entries,
//! totals against targets, targets, and the daily check-in notification.

use std::collections::HashMap;

use aroh_core::keychain;
use aroh_core::models::TrackerEntry;
use aroh_intake::KindTotal;
use chrono::{FixedOffset, TimeZone, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};

use super::replan::owner_timezone;
use super::with_core;
use crate::{AppState, Core};

/// Settings key holding the local check-in time as `HH:MM`.
pub const CHECKIN_TIME_KEY: &str = "checkin.time";
/// Settings key holding `true` or `false` for the daily check-in.
pub const CHECKIN_ENABLED_KEY: &str = "checkin.enabled";
/// Check-in time used until the owner sets one.
pub const DEFAULT_CHECKIN_TIME: &str = "20:00";
/// Notification id reserved for the daily check-in, far from reminder ids.
#[cfg_attr(not(mobile), allow(dead_code))]
pub const CHECKIN_NOTIFICATION_ID: i32 = 2_000_000_001;

const TARGET_KEYS: [&str; 3] = ["target.water_ml", "target.workout_min", "target.food_kcal"];
const MAX_TARGET: f64 = 100_000.0;
const MAX_OFFSET_MINUTES: i32 = 18 * 60;

const COLUMNS: &str = "id, kind, amount, unit, note, raw_text, logged_at, updated_at, deleted";

fn offset(offset_minutes: i32) -> Result<FixedOffset, String> {
    if offset_minutes.abs() > MAX_OFFSET_MINUTES {
        return Err(format!("time zone offset {offset_minutes} min is out of range"));
    }
    FixedOffset::east_opt(offset_minutes * 60).ok_or_else(|| "invalid time zone offset".to_string())
}

fn insert_entries(core: &mut Core, entries: Vec<TrackerEntry>) -> aroh_core::Result<Vec<TrackerEntry>> {
    let tx = core.conn.transaction()?;
    let mut saved = Vec::with_capacity(entries.len());
    for mut entry in entries {
        entry.updated_at = core.clock.tick();
        entry.deleted = false;
        tx.execute(
            &format!("INSERT INTO tracker_entries ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)"),
            params![entry.id, entry.kind, entry.amount, entry.unit, entry.note, entry.raw_text, entry.logged_at, entry.updated_at],
        )?;
        saved.push(entry);
    }
    tx.commit()?;
    Ok(saved)
}

fn entries_today<Z: TimeZone>(conn: &Connection, tz: Z) -> aroh_core::Result<Vec<TrackerEntry>> {
    let day = Utc::now().with_timezone(&tz).date_naive();
    let (start, end) = aroh_intake::local_day_bounds(day, tz);
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM tracker_entries WHERE deleted = 0 AND logged_at >= ?1 AND logged_at < ?2 ORDER BY logged_at DESC"
    ))?;
    let rows = stmt.query_map(params![start, end], |row| {
        Ok(TrackerEntry {
            id: row.get(0)?,
            kind: row.get(1)?,
            amount: row.get(2)?,
            unit: row.get(3)?,
            note: row.get(4)?,
            raw_text: row.get(5)?,
            logged_at: row.get(6)?,
            updated_at: row.get(7)?,
            deleted: row.get(8)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn settings(conn: &Connection) -> aroh_core::Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
}

fn set_setting(core: &mut Core, key: &str, value: &str) -> aroh_core::Result<()> {
    let stamp = core.clock.tick();
    core.conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, stamp],
    )?;
    Ok(())
}

fn totals<Z: TimeZone>(core: &Core, tz: Z) -> aroh_core::Result<Vec<KindTotal>> {
    let day = Utc::now().with_timezone(&tz).date_naive();
    Ok(aroh_intake::daily_totals(&entries_today(&core.conn, tz.clone())?, day, tz, &settings(&core.conn)?))
}

/// Today's totals in the `timezone` setting's IANA zone, or at the device
/// offset `fallback` when the setting is missing.
fn zoned_totals(core: &Core, fallback: FixedOffset) -> aroh_core::Result<Vec<KindTotal>> {
    match owner_timezone(&core.conn)? {
        Some(tz) => totals(core, tz),
        None => totals(core, fallback),
    }
}

/// Logs free text: Claude turns it into entries, which are validated before they are saved.
/// Needs a Claude API key in the keychain. Returns the saved entries.
#[tauri::command]
pub async fn intake_log_text(state: State<'_, AppState>, text: String) -> Result<Vec<TrackerEntry>, String> {
    if text.trim().is_empty() || text.len() > aroh_intake::MAX_RAW_TEXT_BYTES {
        return Err(aroh_intake::IntakeError::TooLarge("raw text").to_string());
    }
    let key = keychain::claude_api_key()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Add your Claude API key in Settings to log by text. Chips work without it.".to_string())?;
    let claude = aroh_agents::Claude::new(&key).map_err(|e| e.to_string())?;
    let schema = aroh_intake::output_schema();
    let json = claude
        .complete_json(aroh_intake::SYSTEM_PROMPT, &aroh_intake::build_user_prompt(&text), Some(&schema))
        .await
        .map_err(|e| e.to_string())?;
    let entries = aroh_intake::parse_model_output(&json, &text, Utc::now()).map_err(|e| e.to_string())?;
    with_core(&state, |c| insert_entries(c, entries))
}

/// Logs a quick-tap chip such as `water` or `workout 30min` with the deterministic parser.
#[tauri::command]
pub fn intake_log_chip(state: State<'_, AppState>, chip: String) -> Result<Vec<TrackerEntry>, String> {
    let entries = aroh_intake::parse_chip(&chip, Utc::now()).map_err(|e| e.to_string())?;
    with_core(&state, |c| insert_entries(c, entries))
}

/// Lists today's live entries, newest first. Today is local to the `timezone`
/// setting; `offset_minutes`, the device's UTC offset, is used when it is missing.
#[tauri::command]
pub fn intake_list_today(state: State<'_, AppState>, offset_minutes: i32) -> Result<Vec<TrackerEntry>, String> {
    let fallback = offset(offset_minutes)?;
    with_core(&state, |c| match owner_timezone(&c.conn)? {
        Some(tz) => entries_today(&c.conn, tz),
        None => entries_today(&c.conn, fallback),
    })
}

/// Today's per-kind totals against the targets in settings.
#[tauri::command]
pub fn intake_totals(state: State<'_, AppState>, offset_minutes: i32) -> Result<Vec<KindTotal>, String> {
    let fallback = offset(offset_minutes)?;
    with_core(&state, |c| zoned_totals(c, fallback))
}

/// Daily targets; a `None` field leaves that target unchanged.
#[derive(Debug, Deserialize)]
pub struct Targets {
    water_ml: Option<f64>,
    workout_min: Option<f64>,
    food_kcal: Option<f64>,
}

/// Saves daily targets. Each must be a finite number from 0 to 100000.
#[tauri::command]
pub fn intake_set_targets(state: State<'_, AppState>, targets: Targets) -> Result<(), String> {
    let values = [targets.water_ml, targets.workout_min, targets.food_kcal];
    if let Some(bad) = values.iter().flatten().find(|v| !v.is_finite() || **v < 0.0 || **v > MAX_TARGET) {
        return Err(format!("target {bad} must be between 0 and {MAX_TARGET}"));
    }
    with_core(&state, |c| {
        for (key, value) in TARGET_KEYS.iter().zip(values) {
            if let Some(value) = value {
                set_setting(c, key, &value.to_string())?;
            }
        }
        Ok(())
    })
}

/// The daily check-in settings as the settings screen shows them.
#[derive(Debug, Serialize)]
pub struct Checkin {
    enabled: bool,
    time: String,
}

fn read_checkin(conn: &Connection) -> aroh_core::Result<Checkin> {
    let map = settings(conn)?;
    let time = map
        .get(CHECKIN_TIME_KEY)
        .filter(|t| aroh_intake::parse_checkin_time(t).is_ok())
        .cloned()
        .unwrap_or_else(|| DEFAULT_CHECKIN_TIME.to_string());
    Ok(Checkin { enabled: map.get(CHECKIN_ENABLED_KEY).is_none_or(|v| v == "true"), time })
}

/// Replaces the pending check-in notification with one that repeats daily at `checkin`.
/// Only mobile schedules; desktop keeps the setting and schedules nothing.
#[cfg(mobile)]
fn schedule_checkin<R: Runtime>(app: &AppHandle<R>, checkin: &Checkin, body: &str) -> Result<(), String> {
    use tauri_plugin_notification::{NotificationExt, Schedule, ScheduleInterval};
    let notification = app.notification();
    notification.cancel(vec![CHECKIN_NOTIFICATION_ID]).map_err(|e| e.to_string())?;
    if !checkin.enabled {
        return Ok(());
    }
    let (hour, minute) = aroh_intake::parse_checkin_time(&checkin.time).map_err(|e| e.to_string())?;
    let interval = ScheduleInterval { year: None, month: None, day: None, weekday: None, hour: Some(hour), minute: Some(minute), second: Some(0) };
    notification
        .builder()
        .id(CHECKIN_NOTIFICATION_ID)
        .title("Daily check-in")
        .body(body)
        .extra("route", "/log?checkin=1")
        .schedule(Schedule::Interval { interval, allow_while_idle: true })
        .show()
        .map_err(|e| e.to_string())
}

#[cfg(not(mobile))]
fn schedule_checkin<R: Runtime>(_app: &AppHandle<R>, _checkin: &Checkin, _body: &str) -> Result<(), String> {
    Ok(())
}

/// Re-schedules the check-in from saved settings. Called at app start and after each change.
pub fn sync_checkin<R: Runtime>(app: &AppHandle<R>, state: &State<'_, AppState>) -> Result<Checkin, String> {
    let checkin = with_core(state, |c| read_checkin(&c.conn))?;
    let body = "How is today going? Tap to log food, water and workouts.";
    schedule_checkin(app, &checkin, body)?;
    Ok(checkin)
}

/// Reads the daily check-in settings.
#[tauri::command]
pub fn intake_get_checkin(state: State<'_, AppState>) -> Result<Checkin, String> {
    with_core(&state, |c| read_checkin(&c.conn))
}

/// Saves the check-in time (`HH:MM`, local) and switch, then re-schedules the notification.
#[tauri::command]
pub fn intake_set_checkin<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    enabled: bool,
    time: String,
) -> Result<Checkin, String> {
    aroh_intake::parse_checkin_time(&time).map_err(|e| e.to_string())?;
    with_core(&state, |c| {
        set_setting(c, CHECKIN_TIME_KEY, time.trim())?;
        set_setting(c, CHECKIN_ENABLED_KEY, if enabled { "true" } else { "false" })
    })?;
    sync_checkin(&app, &state)
}

const CHECKIN_SYSTEM: &str = "You are a warm, brief coach inside a goal calendar app. \
You get today's tracker totals as JSON data. Ask the owner one short check-in question (at most 25 words) \
about how today is going, naming the target they are furthest behind on, if any. \
Reply only with JSON {\"question\": string}.";
const MAX_QUESTION_BYTES: usize = 300;

async fn agent_question(totals: &[KindTotal]) -> Option<String> {
    let key = keychain::claude_api_key().ok().flatten()?;
    let claude = aroh_agents::Claude::new(&key).ok()?;
    let schema = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["question"],
        "properties": {"question": {"type": "string"}}
    });
    let user = serde_json::json!({ "totals": totals }).to_string();
    let json = claude.complete_json(CHECKIN_SYSTEM, &user, Some(&schema)).await.ok()?;
    let question = serde_json::from_str::<serde_json::Value>(&json).ok()?["question"].as_str()?.trim().to_string();
    (!question.is_empty() && question.len() <= MAX_QUESTION_BYTES).then_some(question)
}

/// The short question /log shows when opened from the check-in. Claude writes it from today's
/// totals when a key is saved; otherwise, or on any failure, a question built without a model.
#[tauri::command]
pub async fn intake_checkin_question(state: State<'_, AppState>, offset_minutes: i32) -> Result<String, String> {
    let fallback = offset(offset_minutes)?;
    let totals = with_core(&state, |c| zoned_totals(c, fallback))?;
    match agent_question(&totals).await {
        Some(question) => Ok(question),
        None => Ok(aroh_intake::checkin_question(&totals)),
    }
}
