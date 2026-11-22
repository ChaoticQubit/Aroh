//! Owner writes from the screens. Each validates its input, stamps the device HLC and
//! leaves the store unchanged when it fails.

use crate::local::{self, MAX_LOG_CHARS};
use crate::{invalid, Result};
use aroh_domain::calendar::{day_rule, local_days};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{
    validate_entry, Action, ActionStatus, Autonomy, Budget, BudgetScope, BusyBlock, CalendarRule, DayKind, EntrySource, FieldKind, Goal,
    Preference, Proposal, ProposalStatus, ProviderSetting, Tracker, TrackerEntry,
};
use aroh_store::repo;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

/// Largest monthly budget the owner may set, in US dollars.
pub const MAX_BUDGET_USD: f64 = 1_000_000.0;

/// What a free-text log became: a tracker entry, or a text queued for the engine.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "kind", content = "entry", rename_all = "snake_case")]
pub enum Logged {
    Entry(TrackerEntry),
    Queued,
}

/// Sets an action to planned, done or skipped; `done_at` is `now` only for done.
/// Errors with `Invalid` for missed (only the tick marks missed) and `NotFound` for a gone row.
pub fn set_action_status(conn: &Connection, clock: &mut Hlc, id: &str, status: ActionStatus, now: DateTime<Utc>) -> Result<Action> {
    if status == ActionStatus::Missed {
        return Err(invalid("status missed is set by the app, not by hand"));
    }
    let mut action: Action = repo::get(conn, id)?;
    action.status = status;
    action.done_at = (status == ActionStatus::Done).then_some(now);
    repo::put(conn, clock, &mut action)?;
    Ok(action)
}

fn entry(
    conn: &Connection,
    clock: &mut Hlc,
    user_id: &str,
    tracker: &Tracker,
    data: Map<String, Value>,
    source: EntrySource,
    raw_text: Option<String>,
    now: DateTime<Utc>,
) -> Result<TrackerEntry> {
    let mut row = TrackerEntry {
        id: aroh_domain::new_id(),
        user_id: user_id.to_string(),
        tracker_id: tracker.id.clone(),
        data,
        source,
        raw_text,
        logged_at: now,
        updated_at: String::new(),
        deleted: false,
    };
    validate_entry(tracker, &row)?;
    repo::put(conn, clock, &mut row)?;
    Ok(row)
}

/// Writes a check-in answer for tracker `tracker_id`: `data` holds values keyed by field key.
/// Errors with `Invalid` when a value names no field of the tracker or has the wrong type.
pub fn add_entry(conn: &Connection, clock: &mut Hlc, user_id: &str, tracker_id: &str, data: Map<String, Value>, now: DateTime<Utc>) -> Result<TrackerEntry> {
    let tracker: Tracker = repo::get(conn, tracker_id)?;
    entry(conn, clock, user_id, &tracker, data, EntrySource::CheckIn, None, now)
}

/// The one number in `text` with the unit written after it, for example "82.4 kg" or "40g".
/// Output: `None` when the text holds no number, several numbers, or a number without a unit.
fn measure(text: &str) -> Option<(f64, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || (matches!(chars[i], '.' | ',') && chars.get(i + 1).is_some_and(char::is_ascii_digit))) {
            i += 1;
        }
        let number: String = chars[start..i].iter().collect::<String>().replace(',', ".");
        let mut j = i;
        while chars.get(j) == Some(&' ') {
            j += 1;
        }
        let unit_start = j;
        while chars.get(j).is_some_and(|c| c.is_alphabetic()) {
            j += 1;
        }
        found.push((number.parse::<f64>().ok()?, chars[unit_start..j].iter().collect::<String>().to_lowercase()));
    }
    match found.as_slice() {
        [(value, unit)] if !unit.is_empty() && value.is_finite() => Some((*value, unit.clone())),
        _ => None,
    }
}

/// Logs free text: one number with a unit that matches the unit of exactly one number field
/// of a live tracker becomes a `log` entry carrying the raw text; anything else is queued for
/// the engine. Errors with `Invalid` for empty text, text over 2000 chars or a full queue.
pub fn log_text(conn: &Connection, clock: &mut Hlc, user_id: &str, text: &str, now: DateTime<Utc>) -> Result<Logged> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_LOG_CHARS {
        return Err(invalid("log text must be 1 to 2000 characters"));
    }
    if let Some((value, unit)) = measure(text) {
        let trackers: Vec<Tracker> = repo::list(conn, &[("user_id", user_id)])?;
        let matches: Vec<(&Tracker, &str)> = trackers
            .iter()
            .flat_map(|t| t.fields.iter().map(move |f| (t, f)))
            .filter(|(_, f)| f.kind == FieldKind::Number && f.unit.as_deref().is_some_and(|u| u.to_lowercase() == unit))
            .map(|(t, f)| (t, f.key.as_str()))
            .collect();
        if let [(tracker, key)] = matches.as_slice() {
            let mut data = Map::new();
            data.insert((*key).to_string(), json!(value));
            return Ok(Logged::Entry(entry(conn, clock, user_id, tracker, data, EntrySource::Log, Some(text.to_string()), now)?));
        }
    }
    local::queue_log(conn, text, now)?;
    Ok(Logged::Queued)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Op {
    Move,
    Resize,
    Create,
    Delete,
}

#[derive(Debug, Deserialize)]
struct Change {
    op: Op,
    #[serde(default)]
    action_id: Option<String>,
    title: String,
    #[serde(default)]
    start_at: Option<DateTime<Utc>>,
    #[serde(default)]
    end_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct Diff {
    changes: Vec<Change>,
}

fn span(change: &Change) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    match (change.start_at, change.end_at) {
        (Some(start), Some(end)) => Ok((start, end)),
        _ => Err(invalid(format!("change to {} needs start_at and end_at", change.title))),
    }
}

fn target(conn: &Connection, change: &Change, user_id: &str) -> Result<Action> {
    let id = change.action_id.as_deref().ok_or_else(|| invalid(format!("change to {} needs action_id", change.title)))?;
    let action: Action = repo::get(conn, id)?;
    if action.user_id != user_id {
        return Err(invalid("change targets another user's action"));
    }
    Ok(action)
}

fn apply_change(conn: &Connection, clock: &mut Hlc, proposal: &Proposal, change: &Change) -> Result<Value> {
    match change.op {
        Op::Move | Op::Resize => {
            let mut action = target(conn, change, &proposal.user_id)?;
            let before = json!(action);
            (action.start_at, action.end_at) = span(change)?;
            if action.status == ActionStatus::Missed {
                action.status = ActionStatus::Planned;
            }
            repo::put(conn, clock, &mut action)?;
            Ok(json!({ "id": action.id, "before": before }))
        }
        Op::Create => {
            let (start_at, end_at) = span(change)?;
            let mut action = Action {
                id: aroh_domain::new_id(),
                user_id: proposal.user_id.clone(),
                goal_id: proposal.goal_id.clone(),
                series_id: None,
                occurrence: None,
                title: change.title.clone(),
                notes: None,
                start_at,
                end_at,
                status: ActionStatus::Planned,
                locked: false,
                done_at: None,
                updated_at: String::new(),
                deleted: false,
            };
            repo::put(conn, clock, &mut action)?;
            Ok(json!({ "id": action.id, "before": null }))
        }
        Op::Delete => {
            let action = target(conn, change, &proposal.user_id)?;
            repo::delete::<Action>(conn, clock, &action.id)?;
            Ok(json!({ "id": action.id, "before": action }))
        }
    }
}

/// Applies or rejects an open proposal. Apply runs every change of its diff on `actions`
/// in one transaction and stores the replaced rows in `undo`; reject only sets the status.
/// Errors with `Invalid` for a decided proposal or a bad diff; nothing changes then.
pub fn decide_proposal(conn: &Connection, clock: &mut Hlc, id: &str, apply: bool) -> Result<Proposal> {
    let mut proposal: Proposal = repo::get(conn, id)?;
    if proposal.status != ProposalStatus::Proposed {
        return Err(invalid("proposal was already decided"));
    }
    let tx = conn.unchecked_transaction()?;
    if apply {
        let diff: Diff = serde_json::from_value(proposal.diff.clone()).map_err(|e| invalid(format!("proposal diff: {e}")))?;
        let mut undo = Vec::with_capacity(diff.changes.len());
        for change in &diff.changes {
            undo.push(apply_change(&tx, clock, &proposal, change)?);
        }
        assert_eq!(undo.len(), diff.changes.len());
        proposal.undo = Some(json!({ "actions": undo }));
        proposal.status = ProposalStatus::Applied;
    } else {
        proposal.status = ProposalStatus::Rejected;
    }
    repo::put(&tx, clock, &mut proposal)?;
    tx.commit()?;
    Ok(proposal)
}

/// Confirms a suggested preference, or dismisses it by tombstoning the row.
pub fn decide_preference(conn: &Connection, clock: &mut Hlc, id: &str, confirm: bool) -> Result<()> {
    if !confirm {
        return Ok(repo::delete::<Preference>(conn, clock, id)?);
    }
    let mut preference: Preference = repo::get(conn, id)?;
    preference.confirmed = true;
    repo::put(conn, clock, &mut preference)?;
    Ok(())
}

/// Owner edit of action `id`: moves or resizes it to `span` (start, end),
/// or deletes it when `span` is `None`. The action is locked, so agents
/// never move it without asking (Q19, Q37). Output: the stored row.
pub fn edit_action(conn: &Connection, clock: &mut Hlc, id: &str, span: Option<(DateTime<Utc>, DateTime<Utc>)>) -> Result<Action> {
    let mut action: Action = repo::get(conn, id)?;
    if action.deleted {
        return Err(invalid("action was deleted"));
    }
    match span {
        Some((start_at, end_at)) => {
            if end_at <= start_at || end_at - start_at > chrono::Duration::days(1) {
                return Err(invalid("an action lasts more than 0 minutes and at most a day"));
            }
            (action.start_at, action.end_at) = (start_at, end_at);
        }
        None => action.deleted = true,
    }
    action.locked = true;
    repo::put(conn, clock, &mut action)?;
    Ok(action)
}

/// Days ahead a Busy day's action may move to.
pub const BUSY_MOVE_DAYS: i64 = 14;

const DEFAULT_DAY_GROUP: &str = "All day";

/// What confirming an all-day event's kind did: the rule written, how many of the event's
/// blocks carry the kind now, the actions moved off a Busy day and the titles of those
/// with no free slot in the next 14 days, which stay where they are.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DaySet {
    pub rule: CalendarRule,
    pub blocks: usize,
    pub moved: Vec<Action>,
    pub stuck: Vec<String>,
}

fn overlaps(a: (DateTime<Utc>, DateTime<Utc>), b: (DateTime<Utc>, DateTime<Utc>)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

fn busy_days(blocks: &[BusyBlock], zone: Tz) -> BTreeSet<NaiveDate> {
    blocks.iter().filter(|b| b.all_day && b.day_kind == Some(DayKind::Busy)).flat_map(|b| local_days(b.start_at, b.end_at, zone)).collect()
}

fn move_off(conn: &Connection, clock: &mut Hlc, user_id: &str, days: &BTreeSet<NaiveDate>, zone: Tz) -> Result<(Vec<Action>, Vec<String>)> {
    let blocks: Vec<BusyBlock> = repo::list(conn, &[("user_id", user_id)])?;
    let blocked = busy_days(&blocks, zone);
    assert!(days.is_subset(&blocked), "the event's days are Busy days");
    let mut actions: Vec<Action> = repo::list(conn, &[("user_id", user_id)])?;
    actions.sort_by(|a, b| a.start_at.cmp(&b.start_at).then_with(|| a.id.cmp(&b.id)));
    let mut taken: Vec<(String, (DateTime<Utc>, DateTime<Utc>))> = actions.iter().map(|a| (a.id.clone(), (a.start_at, a.end_at))).collect();
    taken.extend(blocks.iter().filter(|b| !b.all_day).map(|b| (b.id.clone(), (b.start_at, b.end_at))));
    let (mut moved, mut stuck) = (Vec::new(), Vec::new());
    for mut action in actions.into_iter().filter(|a| a.status == ActionStatus::Planned && days.contains(&a.start_at.with_timezone(&zone).date_naive())) {
        let local = action.start_at.with_timezone(&zone);
        let length = action.end_at - action.start_at;
        let slot = (1..=BUSY_MOVE_DAYS).find_map(|ahead| {
            let date = local.date_naive() + chrono::Duration::days(ahead);
            if blocked.contains(&date) {
                return None;
            }
            let start = zone.from_local_datetime(&date.and_time(local.time())).earliest()?.with_timezone(&Utc);
            let span = (start, start + length);
            (!taken.iter().any(|(id, other)| *id != action.id && overlaps(span, *other))).then_some(span)
        });
        match slot {
            Some(span) => {
                (action.start_at, action.end_at) = span;
                repo::put(conn, clock, &mut action)?;
                taken.retain(|(id, _)| *id != action.id);
                taken.push((action.id.clone(), span));
                moved.push(action);
            }
            None => stuck.push(action.title.clone()),
        }
    }
    Ok((moved, stuck))
}

/// Confirms the kind of the all-day event behind busy block `block_id` in one transaction:
/// writes the event's series rule with `kind` (keeping an existing rule's group), sets the
/// kind on every block of the event, and for `Busy` moves each planned action of its local
/// days (in `zone`) to the same time on the next day that is no Busy day and has no overlap.
/// Errors with `Invalid` for a timed or another user's block, `NotFound` for a gone one.
pub fn set_day_kind(conn: &Connection, clock: &mut Hlc, user_id: &str, block_id: &str, kind: DayKind, zone: Tz) -> Result<DaySet> {
    let block: BusyBlock = repo::get(conn, block_id)?;
    if block.user_id != user_id {
        return Err(invalid("block belongs to another user"));
    }
    if !block.all_day {
        return Err(invalid("only all-day events have a kind"));
    }
    let tx = conn.unchecked_transaction()?;
    let group = block.group_name.clone().unwrap_or_else(|| DEFAULT_DAY_GROUP.to_string());
    let mut rule = day_rule(user_id, &block.calendar_id, &block.event_id, &block.title, &group, kind);
    if let Some(old) = repo::list::<CalendarRule>(&tx, &[("id", rule.id.as_str())])?.into_iter().next() {
        rule = CalendarRule { day_kind: Some(kind), ..old };
    }
    repo::put(&tx, clock, &mut rule)?;
    let event: Vec<BusyBlock> = repo::list(&tx, &[("user_id", user_id), ("calendar_id", block.calendar_id.as_str()), ("event_id", block.event_id.as_str())])?;
    let mut days = BTreeSet::new();
    for mut b in event.iter().filter(|b| b.all_day).cloned() {
        days.extend(local_days(b.start_at, b.end_at, zone));
        if b.day_kind != Some(kind) {
            b.day_kind = Some(kind);
            repo::put(&tx, clock, &mut b)?;
        }
    }
    let blocks = event.iter().filter(|b| b.all_day).count();
    assert!(blocks >= 1, "the chosen block is one of the event's blocks");
    let (moved, stuck) = if kind == DayKind::Busy { move_off(&tx, clock, user_id, &days, zone)? } else { (Vec::new(), Vec::new()) };
    tx.commit()?;
    Ok(DaySet { rule, blocks, moved, stuck })
}

/// Sets the autonomy level agents follow for goal `goal_id`.
pub fn set_autonomy(conn: &Connection, clock: &mut Hlc, goal_id: &str, autonomy: Autonomy) -> Result<Goal> {
    let mut goal: Goal = repo::get(conn, goal_id)?;
    goal.autonomy = autonomy;
    repo::put(conn, clock, &mut goal)?;
    Ok(goal)
}

/// Makes `vendor` and `model` the default provider of `user_id`: one row per vendor
/// (updated or created), the old default loses its flag. The key never passes here.
/// Errors with the row's validation error before anything else is written.
pub fn save_provider(conn: &Connection, clock: &mut Hlc, user_id: &str, vendor: &str, model: &str) -> Result<ProviderSetting> {
    let rows = repo::list::<ProviderSetting>(conn, &[("user_id", user_id)])?;
    let mut row = rows.iter().find(|p| p.vendor == vendor).cloned().unwrap_or_else(|| ProviderSetting {
        id: aroh_domain::new_id(),
        user_id: user_id.to_string(),
        label: vendor.to_string(),
        vendor: vendor.to_string(),
        model: String::new(),
        base_url: None,
        is_default: true,
        updated_at: String::new(),
        deleted: false,
    });
    row.model = model.to_string();
    row.is_default = true;
    repo::put(conn, clock, &mut row)?;
    for mut other in rows.into_iter().filter(|p| p.is_default && p.id != row.id) {
        other.is_default = false;
        repo::put(conn, clock, &mut other)?;
    }
    Ok(row)
}

/// Sets the monthly budget over all goals of `user_id` in US dollars; `None` or 0 removes it.
/// Errors with `Invalid` for a negative, non-finite or too large amount.
pub fn save_budget(conn: &Connection, clock: &mut Hlc, user_id: &str, usd: Option<f64>) -> Result<()> {
    if usd.is_some_and(|v| !v.is_finite() || !(0.0..=MAX_BUDGET_USD).contains(&v)) {
        return Err(invalid("budget must be 0 to 1000000 US dollars"));
    }
    let current = repo::list::<Budget>(conn, &[("user_id", user_id), ("scope", "user_month")])?.into_iter().next();
    let micros = usd.map_or(0, |v| (v * 1_000_000.0).round() as i64);
    if micros <= 0 {
        if let Some(row) = current {
            repo::delete::<Budget>(conn, clock, &row.id)?;
        }
        return Ok(());
    }
    let mut row = current.unwrap_or_else(|| Budget {
        id: aroh_domain::new_id(),
        user_id: user_id.to_string(),
        scope: BudgetScope::UserMonth,
        goal_id: None,
        limit_usd_micros: 0,
        updated_at: String::new(),
        deleted: false,
    });
    row.limit_usd_micros = micros;
    repo::put(conn, clock, &mut row)?;
    Ok(())
}
