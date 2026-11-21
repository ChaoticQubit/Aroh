//! Replan commands: detect missed actions, propose and store a replan, apply, reject and undo it.

use super::with_core;
use crate::{AppState, Core};
use aroh_core::models::{Action, ActionStatus, ReplanSize, ReplanStatus};
use aroh_core::{keychain, repo};
use aroh_replan::{
    apply, classify, detect_missed, undo, validate, Availability, ClaudeAgent, DeterministicAgent, HhMm, PlanInput,
    Proposal, ReplanAgent, Size, Window,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::{AppHandle, Emitter, Manager, State};

/// Event emitted after a small replan auto-applied; payload is a [`StoredReplan`].
pub const EVENT_APPLIED: &str = "replan-applied";
/// Event emitted after a big replan was stored for review; payload is a [`StoredReplan`].
pub const EVENT_PROPOSED: &str = "replan-proposed";

const GRACE_DEFAULT_MIN: i64 = 15;
const GRACE_MAX_MIN: i64 = 24 * 60;

/// One stored replan as the UI sees it.
#[derive(Debug, Clone, Serialize)]
pub struct StoredReplan {
    pub id: String,
    pub size: ReplanSize,
    pub status: ReplanStatus,
    pub created_at: String,
    pub proposal: Proposal,
}

fn setting(conn: &rusqlite::Connection, key: &str) -> aroh_core::Result<Option<String>> {
    Ok(conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

/// The owner's IANA zone from the `timezone` setting, or None when it is
/// missing or not a known zone name.
pub(crate) fn owner_timezone(conn: &rusqlite::Connection) -> aroh_core::Result<Option<chrono_tz::Tz>> {
    Ok(setting(conn, "timezone")?.and_then(|z| z.trim().parse::<chrono_tz::Tz>().ok()))
}

/// Availability from settings `availability` and `timezone`. Missing or bad
/// windows fall back to every day 08:00-20:00; a missing zone uses the
/// device's IANA zone, then UTC.
fn availability(conn: &rusqlite::Connection) -> aroh_core::Result<Availability> {
    let windows: Vec<Window> = setting(conn, "availability")?
        .and_then(|v| serde_json::from_str(&v).ok())
        .filter(|w: &Vec<Window>| !w.is_empty())
        .unwrap_or_else(|| {
            let (start, end) = (HhMm::parse("08:00").expect("valid"), HhMm::parse("20:00").expect("valid"));
            (0..7).map(|weekday| Window { weekday, start, end }).collect()
        });
    let timezone = owner_timezone(conn)?
        .or_else(|| iana_time_zone::get_timezone().ok()?.parse().ok())
        .unwrap_or(chrono_tz::UTC);
    Ok(Availability { windows, timezone })
}

fn grace_minutes(conn: &rusqlite::Connection) -> aroh_core::Result<i64> {
    let grace = setting(conn, "reminders.missed_grace_min")?.and_then(|v| v.trim().parse().ok());
    Ok(grace.unwrap_or(GRACE_DEFAULT_MIN).clamp(0, GRACE_MAX_MIN))
}

fn plan_input(core: &Core, missed_ids: Vec<String>) -> aroh_core::Result<PlanInput> {
    Ok(PlanInput {
        goals: repo::list_goals(&core.conn)?,
        actions: repo::list_actions(&core.conn, None)?,
        availability: availability(&core.conn)?,
        now: Utc::now(),
        missed_ids,
    })
}

/// Writes every row that differs between `before` and `after` in one transaction.
fn write_rows(core: &mut Core, before: &[Action], after: &[Action]) -> aroh_core::Result<()> {
    let old: BTreeMap<&str, &Action> = before.iter().map(|a| (a.id.as_str(), a)).collect();
    let new: BTreeMap<&str, &Action> = after.iter().map(|a| (a.id.as_str(), a)).collect();
    let tx = core.conn.transaction()?;
    for (id, row) in &new {
        match old.get(id) {
            None => repo::insert_action(&tx, &mut core.clock, &mut (*row).clone())?,
            Some(prev) if prev == row => {}
            Some(_) if row.deleted => repo::soft_delete_action(&tx, &mut core.clock, id)?,
            Some(_) => repo::update_action(&tx, &mut core.clock, &mut (*row).clone())?,
        }
    }
    for id in old.keys().filter(|id| !new.contains_key(*id)) {
        repo::soft_delete_action(&tx, &mut core.clock, id)?;
    }
    tx.commit()?;
    Ok(())
}

fn insert_replan(core: &mut Core, trigger: Option<&str>, proposal: &Proposal, size: ReplanSize, status: ReplanStatus) -> aroh_core::Result<StoredReplan> {
    let stored = StoredReplan {
        id: aroh_core::new_id(),
        size,
        status,
        created_at: now_text(),
        proposal: proposal.clone(),
    };
    core.conn.execute(
        "INSERT INTO replans (id, trigger_action_id, diff_json, size, status, created_at, updated_at, deleted)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
        params![stored.id, trigger, to_json(proposal), size, status, stored.created_at, core.clock.tick()],
    )?;
    Ok(stored)
}

fn now_text() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn to_json(proposal: &Proposal) -> String {
    serde_json::to_string(proposal).expect("proposal serializes")
}

fn load_replan(core: &Core, id: &str) -> aroh_core::Result<StoredReplan> {
    let row = core
        .conn
        .query_row(
            "SELECT id, size, status, created_at, diff_json FROM replans WHERE id = ?1 AND deleted = 0",
            [id],
            |r| Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?)),
        )
        .optional()?
        .ok_or_else(|| aroh_core::Error::NotFound(id.to_string()))?;
    let proposal = parse_stored(&row.4);
    Ok(StoredReplan { id: row.0, size: row.1, status: row.2, created_at: row.3, proposal })
}

/// Reads `diff_json`: a `{diff, infeasible}` object, or a bare diff array per the data model.
fn parse_stored(json: &str) -> Proposal {
    serde_json::from_str(json)
        .or_else(|_| serde_json::from_str(json).map(|diff| Proposal { diff, infeasible: vec![] }))
        .unwrap_or(Proposal { diff: vec![], infeasible: vec![] })
}

fn set_status(core: &mut Core, id: &str, status: ReplanStatus) -> aroh_core::Result<()> {
    let tick = core.clock.tick();
    core.conn.execute("UPDATE replans SET status = ?2, updated_at = ?3 WHERE id = ?1", params![id, status, tick])?;
    Ok(())
}

fn stale(id: &str) -> aroh_core::Error {
    aroh_core::Error::NotFound(format!("replan {id} no longer matches the calendar"))
}

/// Marks overdue planned actions missed, replans them across all goals and
/// stores the result. Small replans auto-apply and emit `replan-applied`;
/// big ones are stored proposed and emit `replan-proposed`. Returns the newly missed ids.
#[tauri::command]
pub async fn check_missed(app: AppHandle) -> Result<Vec<String>, String> {
    let missed = replan_missed(&app).await?;
    if !missed.is_empty() {
        crate::commands::calendar::sync_reminders(&app)?;
    }
    Ok(missed)
}

/// Marks overdue planned actions missed and proposes or auto-applies a replan for them.
/// Input: the app handle. Output: the ids marked missed, empty when nothing was overdue.
async fn replan_missed(app: &AppHandle) -> Result<Vec<String>, String> {
    let state = app.state::<AppState>();
    let (missed, input) = with_core(&state, |core| {
        let actions = repo::list_actions(&core.conn, None)?;
        let missed = detect_missed(&actions, Utc::now(), grace_minutes(&core.conn)?);
        let tx = core.conn.transaction()?;
        for action in actions.iter().filter(|a| missed.contains(&a.id)) {
            let mut row = action.clone();
            row.status = ActionStatus::Missed;
            repo::update_action(&tx, &mut core.clock, &mut row)?;
        }
        tx.commit()?;
        Ok((missed.clone(), plan_input(core, missed)?))
    })?;
    if missed.is_empty() {
        return Ok(missed);
    }
    let deterministic = aroh_replan::plan(&input);
    let key = keychain::claude_api_key().ok().flatten();
    let proposal = match key.map(|k| aroh_agents::Claude::new(&k)) {
        Some(Ok(claude)) => ClaudeAgent { claude }.propose(&input, deterministic).await,
        _ => DeterministicAgent.propose(&input, deterministic).await,
    };
    if proposal.diff.is_empty() && proposal.infeasible.is_empty() {
        return Ok(missed);
    }
    let stored = with_core(&state, |core| {
        let fresh = plan_input(core, input.missed_ids.clone())?;
        let small = classify(&proposal.diff) == Size::Small && proposal.infeasible.is_empty();
        if small && validate(&fresh, &proposal.diff).is_ok() {
            write_rows(core, &fresh.actions, &apply(&fresh.actions, &proposal.diff))?;
            insert_replan(core, missed.first().map(String::as_str), &proposal, ReplanSize::Small, ReplanStatus::Applied)
        } else {
            let size = if classify(&proposal.diff) == Size::Small { ReplanSize::Small } else { ReplanSize::Big };
            insert_replan(core, missed.first().map(String::as_str), &proposal, size, ReplanStatus::Proposed)
        }
    })?;
    let event = if stored.status == ReplanStatus::Applied { EVENT_APPLIED } else { EVENT_PROPOSED };
    app.emit(event, &stored).map_err(|e| e.to_string())?;
    Ok(missed)
}

/// Lists replans waiting for review, newest first.
#[tauri::command]
pub fn replan_list_proposed(state: State<'_, AppState>) -> Result<Vec<StoredReplan>, String> {
    with_core(&state, |core| {
        let mut stmt = core
            .conn
            .prepare("SELECT id FROM replans WHERE status = 'proposed' AND deleted = 0 ORDER BY created_at DESC, id DESC")?;
        let ids = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        ids.iter().map(|id| load_replan(core, id)).collect()
    })
}

/// Applies a proposed replan after checking it still fits the current calendar.
#[tauri::command]
pub fn replan_apply(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_core(&state, |core| {
        let stored = load_replan(core, &id)?;
        if stored.status != ReplanStatus::Proposed {
            return Err(stale(&id));
        }
        let missed = repo::list_actions(&core.conn, None)?
            .into_iter()
            .filter(|a| a.status == ActionStatus::Missed)
            .map(|a| a.id)
            .collect();
        let input = plan_input(core, missed)?;
        validate(&input, &stored.proposal.diff).map_err(|_| stale(&id))?;
        write_rows(core, &input.actions, &apply(&input.actions, &stored.proposal.diff))?;
        set_status(core, &id, ReplanStatus::Applied)
    })?;
    crate::commands::calendar::sync_reminders(&app)
}

/// Rejects a proposed replan; the calendar is left as is.
#[tauri::command]
pub fn replan_reject(state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_core(&state, |core| {
        if load_replan(core, &id)?.status != ReplanStatus::Proposed {
            return Err(stale(&id));
        }
        set_status(core, &id, ReplanStatus::Rejected)
    })
}

/// Reverts an applied replan: moved rows go back, created rows are removed.
#[tauri::command]
pub fn replan_undo(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_core(&state, |core| {
        let stored = load_replan(core, &id)?;
        if stored.status != ReplanStatus::Applied {
            return Err(stale(&id));
        }
        let actions = repo::list_actions(&core.conn, None)?;
        write_rows(core, &actions, &undo(&actions, &stored.proposal.diff))?;
        set_status(core, &id, ReplanStatus::Undone)
    })?;
    crate::commands::calendar::sync_reminders(&app)
}
