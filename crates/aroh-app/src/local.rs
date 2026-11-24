//! Device-only values kept in the store's `sync_state` table under `local:<name>` keys:
//! the local user id, owner settings and the free-text logs waiting for the engine.

use crate::{invalid, Result};
use aroh_store::sync::{get_state, set_state};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const PREFIX: &str = "local:";
const USER_ID: &str = "user_id";
const ACCOUNT: &str = "account";
const PENDING: &str = "pending_logs";

/// Settings the owner may write, all stored as text.
pub const SETTINGS: [&str; 13] = [
    "server_url",
    "goal_budget_usd",
    "calendars",
    "health",
    "supabase_url",
    "supabase_anon_key",
    "dismissed_insights",
    "setup_done",
    "day_suggestions",
    "location",
    "location_paused",
    "location_nudges",
    "agent_defaults",
];

/// The server this build connects to while the owner's setting is empty, read at compile time
/// from `AROH_DEFAULT_SUPABASE_URL`, `AROH_DEFAULT_SUPABASE_ANON_KEY` and `AROH_DEFAULT_SERVER_URL`.
/// Only the official build sets them, to the hosted project; a source build has none, so first
/// launch asks for the owner's own server. Settings still edits them; clearing one returns here.
pub const DEFAULTS: [(&str, Option<&str>); 3] = [
    ("supabase_url", option_env!("AROH_DEFAULT_SUPABASE_URL")),
    ("supabase_anon_key", option_env!("AROH_DEFAULT_SUPABASE_ANON_KEY")),
    ("server_url", option_env!("AROH_DEFAULT_SERVER_URL")),
];

/// The default for setting `name` in `defaults`: its value, or `None` when the name has no
/// entry or its value is unset or blank.
pub fn default_in(defaults: &[(&str, Option<&str>)], name: &str) -> Option<String> {
    let value = defaults.iter().find(|(n, _)| *n == name).and_then(|(_, v)| *v)?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// This build's default for setting `name`, from [`DEFAULTS`]; `None` when it has none.
pub fn default_value(name: &str) -> Option<String> {
    default_in(&DEFAULTS, name)
}

/// Most insight ids `dismissed_insights` holds: the Inbox shows the 30 newest insights.
pub const MAX_DISMISSED: usize = 30;

/// Most all-day events `day_suggestions` holds a triage suggestion for, as a triage run sends.
pub const MAX_DAY_SUGGESTIONS: usize = 300;

/// Most free-text logs that may wait for the engine at once.
pub const MAX_PENDING: usize = 100;

/// Longest free-text log, in characters.
pub const MAX_LOG_CHARS: usize = 2000;

/// A free-text log the app could not map to a tracker, waiting for the intake agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingLog {
    pub text: String,
    pub at: DateTime<Utc>,
}

fn key(name: &str) -> String {
    format!("{PREFIX}{name}")
}

/// Returns this device's local user id, creating a uuid v7 on first call.
/// Every local row and the seeded categories use it until an account is adopted.
pub fn user_id(conn: &Connection) -> Result<String> {
    if let Some(id) = get_state(conn, &key(USER_ID))? {
        return Ok(id);
    }
    let id = aroh_domain::new_id();
    set_state(conn, &key(USER_ID), &id)?;
    Ok(id)
}

/// Reads one owner setting; when unset or cleared, its entry in [`DEFAULTS`] or `None`.
/// `name` must be one of [`SETTINGS`].
pub fn setting(conn: &Connection, name: &str) -> Result<Option<String>> {
    assert!(SETTINGS.contains(&name), "unknown setting {name}");
    let stored = get_state(conn, &key(name))?.filter(|v| !v.is_empty());
    Ok(stored.or_else(|| default_value(name)))
}

/// Every owner setting that has a value or a default, keyed by name.
pub fn settings(conn: &Connection) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for name in SETTINGS {
        if let Some(value) = setting(conn, name)? {
            out.insert(name.to_string(), value);
        }
    }
    Ok(out)
}

/// Checks `day_suggestions`: a JSON object of at most [`MAX_DAY_SUGGESTIONS`] device event ids
/// (1 to 255 characters) to a day kind, `reminder`, `busy` or `away`.
/// Errors with `Invalid` for anything else.
fn check_day_suggestions(value: &str) -> Result<()> {
    let map: BTreeMap<String, aroh_domain::rows::DayKind> = serde_json::from_str(value).map_err(|_| invalid("day_suggestions"))?;
    if map.len() > MAX_DAY_SUGGESTIONS || map.keys().any(|id| id.is_empty() || id.chars().count() > aroh_domain::calendar::MAX_ID) {
        return Err(invalid("day_suggestions"));
    }
    Ok(())
}

/// The agent controls a new goal starts with, as `agent_defaults` stores them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDefaults {
    pub autonomy: aroh_domain::rows::Autonomy,
    pub reminders: bool,
    pub use_calendars: bool,
}

/// Validates and stores one owner setting; an empty value clears it. Errors with `Invalid`
/// for an unknown name, a non-https server URL, a bad budget, Supabase URL or anon key, a
/// dismissed list that is not at most [`MAX_DISMISSED`] uuids, bad day suggestions, or a
/// switch that is not `true` or `false`.
pub fn set_setting(conn: &Connection, name: &str, value: &str) -> Result<()> {
    if !SETTINGS.contains(&name) {
        return Err(invalid(format!("setting {name}")));
    }
    let value = value.trim();
    if !value.is_empty() {
        match name {
            "server_url" => aroh_domain::rows::check_url("server_url", value)?,
            "supabase_url" => aroh_domain::rows::check_url("supabase_url", value)?,
            "supabase_anon_key" => aroh_domain::rows::check_public_key("supabase_anon_key", value)?,
            "dismissed_insights" => {
                let ids: Vec<&str> = value.split(',').collect();
                if ids.len() > MAX_DISMISSED {
                    return Err(invalid("dismissed_insights"));
                }
                for id in ids {
                    aroh_domain::rows::check_uuid("dismissed_insights", id)?;
                }
            }
            "day_suggestions" => check_day_suggestions(value)?,
            "agent_defaults" => drop(serde_json::from_str::<AgentDefaults>(value).map_err(|_| invalid("agent_defaults"))?),
            "goal_budget_usd" => {
                let usd: f64 = value.parse().map_err(|_| invalid("goal_budget_usd"))?;
                if !usd.is_finite() || !(0.0..=1_000_000.0).contains(&usd) {
                    return Err(invalid("goal_budget_usd"));
                }
            }
            _ if value != "true" && value != "false" => return Err(invalid(name.to_string())),
            _ => {}
        }
    }
    set_state(conn, &key(name), value)?;
    Ok(())
}

/// The free-text logs waiting for the engine, oldest first.
pub fn pending_logs(conn: &Connection) -> Result<Vec<PendingLog>> {
    match get_state(conn, &key(PENDING))? {
        None => Ok(Vec::new()),
        Some(json) => serde_json::from_str(&json).map_err(|_| invalid("stored pending logs")),
    }
}

/// Queues one free-text log for the engine. Errors with `Invalid` when the queue
/// already holds [`MAX_PENDING`] logs; nothing is dropped then.
pub fn queue_log(conn: &Connection, text: &str, at: DateTime<Utc>) -> Result<()> {
    let mut logs = pending_logs(conn)?;
    if logs.len() >= MAX_PENDING {
        return Err(invalid("log queue is full until the engine is connected"));
    }
    logs.push(PendingLog { text: text.to_string(), at });
    set_state(conn, &key(PENDING), &serde_json::to_string(&logs).expect("pending logs serialize"))?;
    assert!(logs.len() <= MAX_PENDING);
    Ok(())
}

/// Removes each of `sent` once from the queue (oldest match first) and keeps
/// every other entry, including logs queued while they were being sent.
/// Output: how many entries were removed.
pub fn remove_sent(conn: &Connection, sent: &[PendingLog]) -> Result<usize> {
    let mut logs = pending_logs(conn)?;
    let before = logs.len();
    for done in sent {
        if let Some(i) = logs.iter().position(|l| l == done) {
            logs.remove(i);
        }
    }
    set_state(conn, &key(PENDING), &serde_json::to_string(&logs).expect("pending logs serialize"))?;
    assert!(logs.len() <= before);
    Ok(before - logs.len())
}

const REMOVALS: &str = "pending_file_removals";

/// Most goal file objects that may wait for removal from the owner's bucket at once.
pub const MAX_REMOVALS: usize = 10_000;

/// Storage paths of deleted goal files whose objects still wait for removal, oldest first.
pub fn pending_removals(conn: &Connection) -> Result<Vec<String>> {
    match get_state(conn, &key(REMOVALS))? {
        None => Ok(Vec::new()),
        Some(json) => serde_json::from_str(&json).map_err(|_| invalid("stored file removals")),
    }
}

fn store_removals(conn: &Connection, paths: &[String]) -> Result<()> {
    assert!(paths.len() <= MAX_REMOVALS);
    set_state(conn, &key(REMOVALS), &serde_json::to_string(paths).expect("paths serialize"))?;
    Ok(())
}

/// Queues `paths` for removal from the owner's bucket, each once. Errors with `Invalid`
/// past [`MAX_REMOVALS`]; nothing is queued then.
pub fn queue_removals(conn: &Connection, paths: &[String]) -> Result<()> {
    let mut queued = pending_removals(conn)?;
    for path in paths {
        if !queued.contains(path) {
            queued.push(path.clone());
        }
    }
    if queued.len() > MAX_REMOVALS {
        return Err(invalid("too many files wait for removal"));
    }
    store_removals(conn, &queued)
}

/// Drops `removed` from the removal queue, keeping paths queued meanwhile.
pub fn clear_removals(conn: &Connection, removed: &[String]) -> Result<()> {
    let left: Vec<String> = pending_removals(conn)?.into_iter().filter(|p| !removed.contains(p)).collect();
    store_removals(conn, &left)
}

const ZONE: &str = "zone";
const ZONE_PENDING: &str = "zone_pending";
const ZONE_BEFORE: &str = "zone_before";
const NOTES: &str = "notes";

/// Most device-local Inbox notes kept; the oldest beyond it are dropped.
pub const MAX_NOTES: usize = 30;

/// A device-local Inbox note (for example a time zone change), shown as an insight card.
/// Its uuid v7 id is what `dismissed_insights` records when the owner dismisses it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

/// The IANA zone the store's rows were last shifted to; `None` before the first record.
pub fn zone(conn: &Connection) -> Result<Option<String>> {
    Ok(get_state(conn, &key(ZONE))?)
}

/// Stores `zone` as the zone the rows follow. With `pending` (a shift) the zone it
/// replaces becomes [`previous_zone`] and the engine's stored zone still has to follow
/// (see [`zone_pending`]).
pub fn set_zone(conn: &Connection, zone: &str, pending: bool) -> Result<()> {
    assert!(zone.parse::<chrono_tz::Tz>().is_ok(), "zone must be a known IANA name");
    if pending {
        if let Some(before) = self::zone(conn)? {
            set_state(conn, &key(ZONE_BEFORE), &before)?;
        }
        set_state(conn, &key(ZONE_PENDING), "true")?;
    }
    set_state(conn, &key(ZONE), zone)?;
    Ok(())
}

/// The zone the rows followed before the last shift; `None` before the first shift.
pub fn previous_zone(conn: &Connection) -> Result<Option<String>> {
    Ok(get_state(conn, &key(ZONE_BEFORE))?)
}

/// Whether a zone change still has to reach the engine.
pub fn zone_pending(conn: &Connection) -> Result<bool> {
    Ok(get_state(conn, &key(ZONE_PENDING))?.as_deref() == Some("true"))
}

/// Marks the stored zone as sent to the engine.
pub fn clear_zone_pending(conn: &Connection) -> Result<()> {
    set_state(conn, &key(ZONE_PENDING), "")?;
    Ok(())
}

/// The device-local Inbox notes, newest first, at most [`MAX_NOTES`].
pub fn notes(conn: &Connection) -> Result<Vec<Note>> {
    match get_state(conn, &key(NOTES))? {
        None => Ok(Vec::new()),
        Some(json) => serde_json::from_str(&json).map_err(|_| invalid("stored notes")),
    }
}

/// Adds a note with a fresh uuid v7 id in front, dropping the oldest beyond [`MAX_NOTES`].
/// Output: the note stored.
pub fn add_note(conn: &Connection, title: &str, body: &str, at: DateTime<Utc>) -> Result<Note> {
    let note = Note { id: aroh_domain::new_id(), title: title.to_string(), body: body.to_string(), created_at: at };
    let mut list = notes(conn)?;
    list.insert(0, note.clone());
    list.truncate(MAX_NOTES);
    set_state(conn, &key(NOTES), &serde_json::to_string(&list).expect("notes serialize"))?;
    assert!(list.len() <= MAX_NOTES);
    Ok(note)
}

const HEALTH_ANCHORS: &str = "health_anchors";
const DEVICE_ID: &str = "device_id";

/// The stored Apple Health query anchors, one per HealthKit type, as Swift archived them.
/// Empty before the first import.
pub fn health_anchors(conn: &Connection) -> Result<BTreeMap<crate::health::HealthType, String>> {
    match get_state(conn, &key(HEALTH_ANCHORS))? {
        None => Ok(BTreeMap::new()),
        Some(json) => serde_json::from_str(&json).map_err(|_| invalid("stored health anchors")),
    }
}

/// Stores `anchors`, replacing the stored set. Call inside the import transaction so
/// anchors never run ahead of the entries they cover.
pub fn set_health_anchors(conn: &Connection, anchors: &BTreeMap<crate::health::HealthType, String>) -> Result<()> {
    set_state(conn, &key(HEALTH_ANCHORS), &serde_json::to_string(anchors).expect("anchors serialize"))?;
    Ok(())
}

/// This install's device id for push registration, a uuid v7 created on first call.
/// It names the device to the engine; the push token itself is never stored here.
pub fn device_id(conn: &Connection) -> Result<String> {
    if let Some(id) = get_state(conn, &key(DEVICE_ID))? {
        return Ok(id);
    }
    let id = aroh_domain::new_id();
    set_state(conn, &key(DEVICE_ID), &id)?;
    Ok(id)
}

/// The account this device adopted at sign-in, if any.
pub fn account(conn: &Connection) -> Result<Option<String>> {
    Ok(get_state(conn, &key(ACCOUNT))?)
}

/// The settings that say which server this device talks to. A wipe keeps them, so a user
/// who deleted their account can sign back in within the grace period and cancel it.
pub const CONNECTION_SETTINGS: [&str; 3] = ["server_url", "supabase_url", "supabase_anon_key"];

/// Deletes every local row and device value (account, settings, queues, sync cursors) in
/// one transaction, without tombstones, so nothing syncs out and the store is as new; for
/// account deletion. The schema and the `CONNECTION_SETTINGS` stay. Returns how many rows
/// were deleted, not counting the kept settings.
pub fn wipe(conn: &mut Connection) -> Result<usize> {
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut kept = Vec::new();
    for name in CONNECTION_SETTINGS {
        if let Some(value) = get_state(conn, &key(name))? {
            kept.push((key(name), value));
        }
    }
    let tx = conn.transaction()?;
    let mut removed = 0;
    for table in &tables {
        removed += tx.execute(&format!("DELETE FROM \"{}\"", table.replace('"', "\"\"")), [])?;
    }
    for (k, v) in &kept {
        set_state(&tx, k, v)?;
    }
    tx.commit()?;
    let removed = removed - kept.len();
    assert!(account(conn)?.is_none(), "a wiped store has no account");
    Ok(removed)
}

/// Makes `account` (the Supabase user id) the owner of every local row, once
/// per device: rows move from the local id, goals point at the account's
/// seeded categories, and the local seeds are replaced. A device already
/// adopted by another account is refused, so no account sees another's data.
pub fn adopt(conn: &mut Connection, account: &str) -> Result<()> {
    aroh_domain::rows::check_uuid("account", account)?;
    match self::account(conn)? {
        Some(current) if current == account => return Ok(()),
        Some(_) => return Err(invalid("this device holds another account's data; sign in with that account")),
        None => {}
    }
    let old = user_id(conn)?;
    let tx = conn.transaction()?;
    let old_seeds = aroh_domain::seed::categories(&old);
    for (from, to) in old_seeds.iter().zip(aroh_domain::seed::categories(account)) {
        tx.execute("UPDATE goals SET category_id = ?1 WHERE category_id = ?2", [&to.id, &from.id])?;
        tx.execute("DELETE FROM categories WHERE id = ?1", [&from.id])?;
    }
    for table in aroh_domain::schema::SYNCED.iter() {
        tx.execute(&format!("UPDATE {} SET user_id = ?1 WHERE user_id = ?2", table.name), [account, old.as_str()])?;
    }
    for category in aroh_domain::seed::categories(account) {
        aroh_store::repo::insert_derived(&tx, &category)?;
    }
    set_state(&tx, &key(USER_ID), account)?;
    set_state(&tx, &key(ACCOUNT), account)?;
    tx.commit()?;
    assert_eq!(user_id(conn)?, account);
    Ok(())
}
