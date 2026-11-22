//! One read of every row the screens show, so the UI maps rows to views in one place.

use crate::local::{self, PendingLog};
use crate::Result;
use aroh_domain::rows::{
    Action, Budget, BusyBlock, Category, Goal, Persona, Preference, Proposal, ProposalStatus, ProviderSetting, SearchSetting, Tracker,
    TrackerEntry,
};
use aroh_store::repo;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::BTreeMap;

/// Live rows of the local user for the screens: actions overlapping the asked range,
/// entries logged since the asked instant, open proposals and unconfirmed preferences.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub user_id: String,
    pub categories: Vec<Category>,
    pub goals: Vec<Goal>,
    pub actions: Vec<Action>,
    pub trackers: Vec<Tracker>,
    pub entries: Vec<TrackerEntry>,
    pub proposals: Vec<Proposal>,
    pub preferences: Vec<Preference>,
    pub personas: Vec<Persona>,
    pub providers: Vec<ProviderSetting>,
    pub search: Vec<SearchSetting>,
    pub budgets: Vec<Budget>,
    pub settings: BTreeMap<String, String>,
    pub pending_logs: Vec<PendingLog>,
    pub busy: Vec<BusyBlock>,
}

/// Busy blocks of `user_id` overlapping `[from, to)`, in start order.
/// Read for the Calendar only; never mixed into actions.
/// Tombstones are left out.
fn busy_in(conn: &Connection, user_id: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<BusyBlock>> {
    let mut busy: Vec<BusyBlock> = repo::list::<BusyBlock>(conn, &[("user_id", user_id)])?.into_iter().filter(|b| b.start_at < to && b.end_at > from).collect();
    busy.sort_by(|a, b| a.start_at.cmp(&b.start_at).then_with(|| a.id.cmp(&b.id)));
    Ok(busy)
}

/// Reads the snapshot of `user_id`: actions overlapping `[from, to)` in start order and
/// entries logged at or after `since`, newest first. Every other table is read whole.
pub fn snapshot(conn: &Connection, user_id: &str, from: DateTime<Utc>, to: DateTime<Utc>, since: DateTime<Utc>) -> Result<Snapshot> {
    assert!(from < to, "snapshot range must not be empty");
    let mine = [("user_id", user_id)];
    let mut actions: Vec<Action> = repo::list::<Action>(conn, &mine)?.into_iter().filter(|a| a.start_at < to && a.end_at > from).collect();
    actions.sort_by(|a, b| a.start_at.cmp(&b.start_at).then_with(|| a.id.cmp(&b.id)));
    let mut entries: Vec<TrackerEntry> = repo::list::<TrackerEntry>(conn, &mine)?.into_iter().filter(|e| e.logged_at >= since).collect();
    entries.sort_by(|a, b| b.logged_at.cmp(&a.logged_at));
    Ok(Snapshot {
        user_id: user_id.to_string(),
        categories: repo::list(conn, &mine)?,
        goals: repo::list(conn, &mine)?,
        actions,
        trackers: repo::list(conn, &mine)?,
        entries,
        proposals: repo::list::<Proposal>(conn, &mine)?.into_iter().filter(|p| p.status == ProposalStatus::Proposed).collect(),
        preferences: repo::list::<Preference>(conn, &mine)?.into_iter().filter(|p| !p.confirmed).collect(),
        personas: repo::list(conn, &mine)?,
        providers: repo::list(conn, &mine)?,
        search: repo::list(conn, &mine)?,
        budgets: repo::list(conn, &mine)?,
        settings: local::settings(conn)?,
        pending_logs: local::pending_logs(conn)?,
        busy: busy_in(conn, user_id, from, to)?,
    })
}
