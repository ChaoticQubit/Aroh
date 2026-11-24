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
    a.title == b.title && a.start_at == b.start_at && a.end_at == b.end_at && a.all_day == b.all_day && a.group_name == b.group_name && a.day_kind == b.day_kind
}

fn in_window(all_day: bool, start: DateTime<Utc>, end: DateTime<Utc>, from: DateTime<Utc>, to: DateTime<Utc>) -> bool {
    if all_day {
        end > from && start < to
    } else {
        start >= from && start < to
    }
}

/// Imports the device occurrences of `[from, to)` for `user_id`. Timed events starting in the
/// window are matched against the rules and kept ones become busy blocks. All-day events
/// overlapping the window always become blocks with their rule's day kind; holiday ones with
/// no rule are Reminders, other ones no rule decides wait for a kind and go to triage.
/// Stale live blocks of the window are tombstoned.
pub fn import(conn: &Connection, clock: &mut Hlc, user_id: &str, events: &[DeviceEvent], from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Import> {
    assert!(from < to, "the import window is not empty");
    for e in events {
        e.validate()?;
    }
    let mine = [("user_id", user_id)];
    let rules: Vec<CalendarRule> = repo::list(conn, &mine)?;
    let existing: BTreeMap<String, BusyBlock> = repo::list::<BusyBlock>(conn, &mine)?.into_iter().map(|b| (b.id.clone(), b)).collect();
    let mut inside: Vec<&DeviceEvent> = events.iter().filter(|e| in_window(e.all_day, e.start_at, e.end_at, from, to)).collect();
    inside.sort_by(|a, b| a.start_at.cmp(&b.start_at).then_with(|| a.event_id.cmp(&b.event_id)));
    let mut report = Import::default();
    let mut keep = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for event in inside {
        let rule = rule_for(&rules, event);
        if rule.is_none() && !(event.all_day && event.holiday) && seen.insert(event.event_id.clone()) {
            report.unmatched.push(event.clone());
        }
        if !event.all_day && rule.is_none_or(|r| r.decision != RuleDecision::Keep) {
            report.ignored += usize::from(rule.is_some());
            continue;
        }
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
    for block in existing.values() {
        if in_window(block.all_day, block.start_at, block.end_at, from, to) && !keep.contains(&block.id) {
            repo::delete::<BusyBlock>(conn, clock, &block.id)?;
            report.removed += 1;
        }
    }
    Ok(report)
}
