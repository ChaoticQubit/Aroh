//! Device calendar import: matches the occurrences read from EventKit against the synced
//! calendar rules, keeps busy blocks in step with kept events and returns the unmatched ones.

use crate::Result;
use aroh_domain::calendar::{busy_block, rule_for, DeviceEvent};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{BusyBlock, CalendarRule, RuleDecision};
use aroh_store::repo;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};

/// What one import did: busy blocks kept (written or unchanged), occurrences ignored by a
/// rule, stale blocks removed, and the events no rule decides, one per series, earliest first.
/// `unmatched` is what goes to the triage agent.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Import {
    pub kept: usize,
    pub ignored: usize,
    pub removed: usize,
    pub unmatched: Vec<DeviceEvent>,
}

fn same(a: &BusyBlock, b: &BusyBlock) -> bool {
    a.title == b.title && a.start_at == b.start_at && a.end_at == b.end_at && a.group_name == b.group_name
}

/// Imports the device occurrences of `[from, to)` for `user_id`. All-day events and
/// occurrences starting outside the window are skipped. Kept events become busy blocks,
/// live blocks of the window with no kept occurrence are tombstoned. Errors on a bad event.
pub fn import(conn: &Connection, clock: &mut Hlc, user_id: &str, events: &[DeviceEvent], from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Import> {
    assert!(from < to, "the import window is not empty");
    for e in events {
        e.validate()?;
    }
    let mine = [("user_id", user_id)];
    let rules: Vec<CalendarRule> = repo::list(conn, &mine)?;
    let existing: BTreeMap<String, BusyBlock> = repo::list::<BusyBlock>(conn, &mine)?.into_iter().map(|b| (b.id.clone(), b)).collect();
    let mut inside: Vec<&DeviceEvent> = events.iter().filter(|e| !e.all_day && e.start_at >= from && e.start_at < to).collect();
    inside.sort_by(|a, b| a.start_at.cmp(&b.start_at).then_with(|| a.event_id.cmp(&b.event_id)));
    let mut report = Import::default();
    let mut keep = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for event in inside {
        match rule_for(&rules, event) {
            Some(rule) if rule.decision == RuleDecision::Keep => {
                let mut block = busy_block(user_id, event, rule);
                if !keep.insert(block.id.clone()) {
                    continue;
                }
                report.kept += 1;
                match existing.get(&block.id) {
                    Some(old) if same(old, &block) => {}
                    old => {
                        block.updated_at = old.map(|o| o.updated_at.clone()).unwrap_or_default();
                        repo::put(conn, clock, &mut block)?;
                    }
                }
            }
            Some(_) => report.ignored += 1,
            None => {
                if seen.insert(event.event_id.clone()) {
                    report.unmatched.push(event.clone());
                }
            }
        }
    }
    for block in existing.values() {
        if block.start_at >= from && block.start_at < to && !keep.contains(&block.id) {
            repo::delete::<BusyBlock>(conn, clock, &block.id)?;
            report.removed += 1;
        }
    }
    Ok(report)
}
