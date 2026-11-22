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
pub const SETTINGS: [&str; 6] = ["server_url", "goal_budget_usd", "calendars", "health", "supabase_url", "supabase_anon_key"];

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

/// Reads one owner setting; `None` when unset or cleared.
/// `name` must be one of [`SETTINGS`].
pub fn setting(conn: &Connection, name: &str) -> Result<Option<String>> {
    assert!(SETTINGS.contains(&name), "unknown setting {name}");
    Ok(get_state(conn, &key(name))?.filter(|v| !v.is_empty()))
}

/// Every owner setting that has a value, keyed by name.
pub fn settings(conn: &Connection) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for name in SETTINGS {
        if let Some(value) = setting(conn, name)? {
            out.insert(name.to_string(), value);
        }
    }
    Ok(out)
}

/// Validates and stores one owner setting; an empty value clears it.
/// Errors with `Invalid` for an unknown name, a non-https server URL, a negative or
/// non-numeric budget, a bad Supabase URL or anon key, or a switch that is
/// not `true` or `false`.
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
