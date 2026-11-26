//! One read of every row the screens show, so the UI maps rows to views in one place.

use crate::local::{self, Note, PendingLog};
use crate::Result;
use aroh_domain::rows::{
    Action, ActionSeries, Budget, BusyBlock, Category, Goal, GoalFile, Persona, Preference, Proposal, ProposalStatus, ProviderSetting, SearchSetting, Tracker,
    TrackerEntry,
};
use aroh_store::repo;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::BTreeMap;

/// Live rows of the local user for the screens: actions overlapping the asked range,
/// entries logged since the asked instant, open proposals, unconfirmed preferences, kept
/// profile `facts` (confirmed, blocked topics included, newest first), the
/// device-local notes and the zone the rows were last shifted to.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub user_id: String,
    pub categories: Vec<Category>,
    pub goals: Vec<Goal>,
    pub files: Vec<GoalFile>,
    pub actions: Vec<Action>,
    pub trackers: Vec<Tracker>,
    pub entries: Vec<TrackerEntry>,
    pub proposals: Vec<Proposal>,
    pub preferences: Vec<Preference>,
    pub facts: Vec<Preference>,
    pub personas: Vec<Persona>,
    pub providers: Vec<ProviderSetting>,
    pub search: Vec<SearchSetting>,
    pub budgets: Vec<Budget>,
    pub settings: BTreeMap<String, String>,
    pub pending_logs: Vec<PendingLog>,
    pub busy: Vec<BusyBlock>,
    pub notes: Vec<Note>,
    pub zone: Option<String>,
}

/// Busy blocks of `user_id` overlapping `[from, to)`, in start order.
/// Read for the Calendar only; never mixed into actions.
/// Tombstones are left out.
fn busy_in(conn: &Connection, user_id: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<BusyBlock>> {
    let mut busy: Vec<BusyBlock> = repo::list::<BusyBlock>(conn, &[("user_id", user_id)])?.into_iter().filter(|b| b.start_at < to && b.end_at > from).collect();
    busy.sort_by(|a, b| a.start_at.cmp(&b.start_at).then_with(|| a.id.cmp(&b.id)));
    Ok(busy)
}

/// Whether `user_id`'s Routines goal shows at `now` (Q93): it lists a routine
/// ([`crate::location::listed`]) or holds an action a delete from `now` would remove
/// (`write::closes_from`). Routines holding only history is left out of every goal list.
fn routines_in_use(conn: &Connection, user_id: &str, now: DateTime<Utc>) -> Result<bool> {
    let id = aroh_domain::seed::routines_id();
    let by_goal = [("user_id", user_id), ("goal_id", id.as_str())];
    for a in repo::list::<Action>(conn, &by_goal)? {
        if crate::write::closes_from(conn, &a, now)? {
            return Ok(true);
        }
    }
    for series in repo::list::<ActionSeries>(conn, &by_goal)? {
        if crate::location::listed(conn, &series, now)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Reads the snapshot of `user_id`: actions overlapping `[from, to)` in start order and
/// entries logged at or after `since`, newest first. Every other table is read whole, but the
/// Routines goal is left out while it shows nothing at `now` (see `routines_in_use`).
pub fn snapshot(conn: &Connection, user_id: &str, from: DateTime<Utc>, to: DateTime<Utc>, since: DateTime<Utc>, now: DateTime<Utc>) -> Result<Snapshot> {
    assert!(from < to, "snapshot range must not be empty");
    let mine = [("user_id", user_id)];
    let mut actions: Vec<Action> = repo::list::<Action>(conn, &mine)?.into_iter().filter(|a| a.start_at < to && a.end_at > from).collect();
    actions.sort_by(|a, b| a.start_at.cmp(&b.start_at).then_with(|| a.id.cmp(&b.id)));
    let mut entries: Vec<TrackerEntry> = repo::list::<TrackerEntry>(conn, &mine)?.into_iter().filter(|e| e.logged_at >= since).collect();
    entries.sort_by(|a, b| b.logged_at.cmp(&a.logged_at));
    let mut prefs: Vec<Preference> = repo::list::<Preference>(conn, &mine)?;
    prefs.sort_by(|a, b| b.noted_at.cmp(&a.noted_at));
    let routines = aroh_domain::seed::routines_id();
    let keep_routines = routines_in_use(conn, user_id, now)?;
    let goals: Vec<Goal> = repo::list::<Goal>(conn, &mine)?.into_iter().filter(|g| keep_routines || g.id != routines).collect();
    Ok(Snapshot {
        user_id: user_id.to_string(),
        categories: repo::list(conn, &mine)?,
        goals,
        files: repo::list::<GoalFile>(conn, &mine)?.into_iter().map(|f| GoalFile { extracted_text: None, ..f }).collect(),
        actions,
        trackers: repo::list(conn, &mine)?,
        entries,
        proposals: repo::list::<Proposal>(conn, &mine)?.into_iter().filter(|p| p.status == ProposalStatus::Proposed).collect(),
        preferences: prefs.iter().filter(|p| !p.confirmed).cloned().collect(),
        facts: prefs.into_iter().filter(|p| p.confirmed).collect(),
        personas: repo::list(conn, &mine)?,
        providers: repo::list(conn, &mine)?,
        search: repo::list(conn, &mine)?,
        budgets: repo::list(conn, &mine)?,
        settings: local::settings(conn)?,
        pending_logs: local::pending_logs(conn)?,
        busy: busy_in(conn, user_id, from, to)?,
        notes: local::notes(conn)?,
        zone: local::zone(conn)?,
    })
}
