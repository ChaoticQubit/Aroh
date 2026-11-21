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
const PENDING: &str = "pending_logs";

/// Settings the owner may write, all stored as text.
pub const SETTINGS: [&str; 4] = ["server_url", "goal_budget_usd", "calendars", "health"];

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
/// non-numeric budget, or a switch that is not `true` or `false`.
pub fn set_setting(conn: &Connection, name: &str, value: &str) -> Result<()> {
    if !SETTINGS.contains(&name) {
        return Err(invalid(format!("setting {name}")));
    }
    let value = value.trim();
    if !value.is_empty() {
        match name {
            "server_url" => aroh_domain::rows::check_url("server_url", value)?,
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
