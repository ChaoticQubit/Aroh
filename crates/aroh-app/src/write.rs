//! Owner writes from the screens. Each validates its input, stamps the device HLC and
//! leaves the store unchanged when it fails.

use crate::local::{self, MAX_LOG_CHARS};
use crate::{invalid, Result};
use aroh_domain::calendar::{day_rule, local_days, DAY_GROUP};
use aroh_domain::hlc::Hlc;
use aroh_domain::recur::{self, DEFAULT_WINDOW_DAYS};
use aroh_domain::rows::{
    validate_entry, Action, ActionSeries, ActionStatus, Autonomy, Budget, BudgetScope, BusyBlock, CalendarRule, Category, DayKind, EntrySource, FieldKind, FileKind, Goal,
    GoalFile, IngestStatus, Preference, Proposal, ProposalStatus, ProviderSetting, Tracker, TrackerEntry,
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

/// Sets an action in any status to planned, done or skipped, so a missed action can be
/// logged done later (Q78); `done_at` is `now` only for done. Errors with `Invalid` for
/// missed (only the tick marks missed) and `NotFound` for a gone row.
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
                card: None,
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
    decide_proposal_with(conn, clock, id, apply, None)
}

/// Like [`decide_proposal`], applying `opened` instead of the stored diff: the diff of a
/// diary run is sealed, so the caller opens it with the diary key first. A sealed diff
/// with no `opened` fails to parse and nothing changes.
pub fn decide_proposal_with(conn: &Connection, clock: &mut Hlc, id: &str, apply: bool, opened: Option<Value>) -> Result<Proposal> {
    let mut proposal: Proposal = repo::get(conn, id)?;
    if proposal.status != ProposalStatus::Proposed {
        return Err(invalid("proposal was already decided"));
    }
    let tx = conn.unchecked_transaction()?;
    if apply {
        let diff: Diff = serde_json::from_value(opened.unwrap_or_else(|| proposal.diff.clone())).map_err(|e| invalid(format!("proposal diff: {e}")))?;
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

/// Owner edit of profile fact `id` on "What Aroh knows about you": the trimmed `fact`
/// text, dated `now`. Errors with `Invalid` for a blank fact or a blocked topic, which
/// holds no fact. Output: the stored row.
pub fn edit_fact(conn: &Connection, clock: &mut Hlc, id: &str, fact: &str, now: DateTime<Utc>) -> Result<Preference> {
    let mut p: Preference = repo::get(conn, id)?;
    if p.blocked {
        return Err(invalid("a blocked topic holds no fact"));
    }
    p.fact = fact.trim().to_string();
    p.noted_at = now;
    p.confirmed = true;
    repo::put(conn, clock, &mut p)?;
    Ok(p)
}

/// "Never learn this" on fact `id`: the row becomes a blocked topic and keeps only the
/// label of "<label>: <answer>" (else the topic) as its fact (D6), so agents never keep
/// that topic again. Errors with `Invalid` for a fact without a topic. Output: the row.
pub fn block_topic(conn: &Connection, clock: &mut Hlc, id: &str) -> Result<Preference> {
    let mut p: Preference = repo::get(conn, id)?;
    let topic = p.topic.clone().ok_or_else(|| invalid("only a fact with a topic can be blocked"))?;
    p.fact = p.fact.split_once(": ").map(|(label, _)| label.trim().to_string()).filter(|l| !l.is_empty()).unwrap_or(topic);
    p.blocked = true;
    p.confirmed = true;
    repo::put(conn, clock, &mut p)?;
    assert!(p.blocked && p.topic.is_some());
    Ok(p)
}

/// "Allow again" on blocked topic `id`: deletes the row, so the topic may be learned
/// again. Errors with `Invalid` for a row that is not blocked.
pub fn allow_topic(conn: &Connection, clock: &mut Hlc, id: &str) -> Result<()> {
    let p: Preference = repo::get(conn, id)?;
    if !p.blocked {
        return Err(invalid("only a blocked topic is allowed again"));
    }
    Ok(repo::delete::<Preference>(conn, clock, id)?)
}

/// Owner edit of action `id` in any status (Q78): moves or resizes it to `span`
/// (start, end), or deletes it when `span` is `None`. A moved missed action is
/// planned again; done and skipped keep their status. The action is locked, so
/// agents never move it without asking (Q19, Q37). Output: the stored row.
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
            if action.status == ActionStatus::Missed {
                action.status = ActionStatus::Planned;
            }
        }
        None => action.deleted = true,
    }
    action.locked = true;
    repo::put(conn, clock, &mut action)?;
    Ok(action)
}

/// The owner's Fixed time switch on action `id`: sets `locked` and keeps the
/// time as it is. A fixed action is never moved by agents or the Busy-day
/// sweep (Q53). Output: the stored row; a deleted action is refused.
pub fn set_locked(conn: &Connection, clock: &mut Hlc, id: &str, locked: bool) -> Result<Action> {
    let mut action: Action = repo::get(conn, id)?;
    if action.deleted {
        return Err(invalid("action was deleted"));
    }
    if action.locked != locked {
        action.locked = locked;
        repo::put(conn, clock, &mut action)?;
    }
    Ok(action)
}

/// Days ahead a Busy day's action may move to.
pub const BUSY_MOVE_DAYS: i64 = 14;

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

fn midnight_utc(date: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_time(chrono::NaiveTime::MIN))
}

fn series_ahead(conn: &Connection, user_id: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<(String, (DateTime<Utc>, DateTime<Utc>))>> {
    assert!(from < to, "the look-ahead is not empty");
    let mut out = Vec::new();
    for series in repo::list::<ActionSeries>(conn, &[("user_id", user_id)])? {
        let stored = repo::occurrences(conn, &series.id)?;
        let mut at = from;
        while at < to {
            out.extend(recur::materialize(&series, at, DEFAULT_WINDOW_DAYS, &stored)?.into_iter().map(|a| (a.id, (a.start_at, a.end_at))));
            at += chrono::Duration::days(i64::from(DEFAULT_WINDOW_DAYS));
        }
    }
    Ok(out)
}

/// Keeps planned, unlocked actions of `user_id` off Busy days from `today` on (local days in
/// `zone`): each moves to the same local time on the next of the following 14 days that is no
/// Busy day and overlaps no action, timed busy block or coming occurrence of a series. Locked,
/// done and past actions stay. Opens no transaction. Output: the moved actions and the titles
/// of those with no free day, which stay where they are.
pub fn keep_off_busy_days(conn: &Connection, clock: &mut Hlc, user_id: &str, today: NaiveDate, zone: Tz) -> Result<(Vec<Action>, Vec<String>)> {
    let blocks: Vec<BusyBlock> = repo::list(conn, &[("user_id", user_id)])?;
    let blocked = busy_days(&blocks, zone);
    let day_of = |a: &Action| a.start_at.with_timezone(&zone).date_naive();
    let mut actions: Vec<Action> = repo::list(conn, &[("user_id", user_id)])?;
    actions.sort_by(|a, b| a.start_at.cmp(&b.start_at).then_with(|| a.id.cmp(&b.id)));
    let stays_off = |a: &Action| a.status == ActionStatus::Planned && !a.locked && day_of(a) >= today && blocked.contains(&day_of(a));
    let Some(last) = actions.iter().filter(|a| stays_off(a)).map(day_of).max() else {
        return Ok((Vec::new(), Vec::new()));
    };
    let mut taken: Vec<(String, (DateTime<Utc>, DateTime<Utc>))> = actions.iter().map(|a| (a.id.clone(), (a.start_at, a.end_at))).collect();
    taken.extend(blocks.iter().filter(|b| !b.all_day).map(|b| (b.id.clone(), (b.start_at, b.end_at))));
    let horizon = midnight_utc(last + chrono::Duration::days(BUSY_MOVE_DAYS + 3));
    taken.extend(series_ahead(conn, user_id, midnight_utc(today - chrono::Duration::days(1)), horizon)?);
    let (mut moved, mut stuck) = (Vec::new(), Vec::new());
    let candidates: Vec<Action> = actions.iter().filter(|a| stays_off(a)).cloned().collect();
    for mut action in candidates {
        let local = action.start_at.with_timezone(&zone);
        let length = action.end_at - action.start_at;
        let slot = (1..=BUSY_MOVE_DAYS).find_map(|ahead| {
            let date = local.date_naive() + chrono::Duration::days(ahead);
            if blocked.contains(&date) {
                return None;
            }
            let start = zone.from_local_datetime(&date.and_time(local.time())).earliest()?.with_timezone(&Utc);
            let span = (start, start + length);
            assert!(span.1 <= horizon, "the look-ahead covers every day a move may land on");
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
/// kind on every block of the event, then keeps actions off every Busy day from `today` on
/// with [`keep_off_busy_days`] (local days in `zone`).
/// Errors with `Invalid` for a timed or another user's block, `NotFound` for a gone one.
pub fn set_day_kind(conn: &Connection, clock: &mut Hlc, user_id: &str, block_id: &str, kind: DayKind, today: NaiveDate, zone: Tz) -> Result<DaySet> {
    let block: BusyBlock = repo::get(conn, block_id)?;
    if block.user_id != user_id {
        return Err(invalid("block belongs to another user"));
    }
    if !block.all_day {
        return Err(invalid("only all-day events have a kind"));
    }
    let tx = conn.unchecked_transaction()?;
    let group = block.group_name.clone().unwrap_or_else(|| DAY_GROUP.to_string());
    let mut rule = day_rule(user_id, &block.calendar_id, &block.event_id, &block.title, &group, kind);
    if let Some(old) = repo::list::<CalendarRule>(&tx, &[("id", rule.id.as_str())])?.into_iter().next() {
        rule = CalendarRule { day_kind: Some(kind), ..old };
    }
    repo::put(&tx, clock, &mut rule)?;
    let event: Vec<BusyBlock> = repo::list(&tx, &[("user_id", user_id), ("calendar_id", block.calendar_id.as_str()), ("event_id", block.event_id.as_str())])?;
    for mut b in event.iter().filter(|b| b.all_day).cloned() {
        if b.day_kind != Some(kind) {
            b.day_kind = Some(kind);
            repo::put(&tx, clock, &mut b)?;
        }
    }
    let blocks = event.iter().filter(|b| b.all_day).count();
    assert!(blocks >= 1, "the chosen block is one of the event's blocks");
    let (moved, stuck) = keep_off_busy_days(&tx, clock, user_id, today, zone)?;
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

/// Sets the agent controls of goal `goal_id`: local reminders, calendar reads and the
/// run time limit in minutes (`None` uses the Settings limit). The goal row validates
/// the limit (1 to 120), so a refused write leaves the store unchanged.
pub fn set_goal_controls(conn: &Connection, clock: &mut Hlc, goal_id: &str, reminders: bool, use_calendars: bool, run_limit_minutes: Option<i64>) -> Result<Goal> {
    let mut goal: Goal = repo::get(conn, goal_id)?;
    goal.reminders = reminders;
    goal.use_calendars = use_calendars;
    goal.run_limit_minutes = run_limit_minutes;
    repo::put(conn, clock, &mut goal)?;
    Ok(goal)
}

/// Sets whether replans of goal `goal_id` keep its sessions in sequence (Q79, "Keep
/// sessions in order"). The next replan reads it from the synced goal row.
/// Output: the stored goal.
pub fn set_goal_ordered(conn: &Connection, clock: &mut Hlc, goal_id: &str, ordered: bool) -> Result<Goal> {
    let mut goal: Goal = repo::get(conn, goal_id)?;
    goal.ordered = ordered;
    repo::put(conn, clock, &mut goal)?;
    Ok(goal)
}

/// Most characters of a file name kept in its storage path.
const STORED_NAME_CHARS: usize = 80;

/// Builds the row of a file the device stored for goal `goal_id`: its kind from `mime`
/// (image, audio, video, else document) and a storage path
/// `goals/<goal_id>/<id>-<name>` with every character outside `[A-Za-z0-9._-]` as `-`.
/// `read` is the ingest status and the text the device extracted. Output: the unsaved row.
pub fn stored_file(user_id: &str, goal_id: &str, name: &str, mime: &str, size_bytes: i64, read: (IngestStatus, Option<String>)) -> GoalFile {
    let id = aroh_domain::new_id();
    let safe: String = name.trim().chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' }).take(STORED_NAME_CHARS).collect();
    let safe = if safe.trim_matches('.').is_empty() { "file".to_string() } else { safe };
    let kind = match mime.split('/').next() {
        Some("image") => FileKind::Image,
        Some("audio") => FileKind::Audio,
        Some("video") => FileKind::Video,
        _ => FileKind::Document,
    };
    let storage_path = format!("goals/{goal_id}/{id}-{safe}");
    GoalFile {
        id,
        user_id: user_id.to_string(),
        goal_id: goal_id.to_string(),
        name: name.trim().chars().take(255).collect(),
        kind,
        storage_path: Some(storage_path),
        url: None,
        mime: Some(mime.chars().take(255).collect()),
        size_bytes,
        ingest_status: read.0,
        extracted_text: read.1,
        updated_at: String::new(),
        deleted: false,
    }
}

/// Stores goal file `file`. Its goal may not be synced yet (the New Goal form attaches files
/// before Create), but a deleted goal takes no files. Output: the stored row.
pub fn add_goal_file(conn: &Connection, clock: &mut Hlc, mut file: GoalFile) -> Result<GoalFile> {
    let mut query = conn.prepare("SELECT deleted FROM goals WHERE id = ?1")?;
    let deleted = query.query_map([&file.goal_id], |r| r.get::<_, i64>(0))?.next().transpose()?;
    if deleted == Some(1) {
        return Err(invalid(format!("goal {} was deleted", file.goal_id)));
    }
    repo::put(conn, clock, &mut file)?;
    Ok(file)
}

/// Adds the `https` link `url` (trimmed) to goal `goal_id`, pending until the server
/// fetches it. Output: the stored row; a non-https link is refused.
pub fn add_goal_link(conn: &Connection, clock: &mut Hlc, user_id: &str, goal_id: &str, url: &str) -> Result<GoalFile> {
    let url = url.trim();
    let name: String = url.trim_start_matches("https://").chars().take(255).collect();
    let file = GoalFile {
        id: aroh_domain::new_id(),
        user_id: user_id.to_string(),
        goal_id: goal_id.to_string(),
        name,
        kind: FileKind::Link,
        storage_path: None,
        url: Some(url.to_string()),
        mime: None,
        size_bytes: 0,
        ingest_status: IngestStatus::Pending,
        extracted_text: None,
        updated_at: String::new(),
        deleted: false,
    };
    add_goal_file(conn, clock, file)
}

/// Removes goal file `id` from its goal. The stored bytes stay in the owner's bucket.
pub fn remove_goal_file(conn: &Connection, clock: &mut Hlc, id: &str) -> Result<()> {
    repo::delete::<GoalFile>(conn, clock, id)?;
    Ok(())
}

/// Guesses the category of a new goal from its title and description, offline:
/// the id of `user_id`'s seeded category whose keywords match, else `None`.
pub fn guess_category(user_id: &str, title: &str, description: Option<&str>) -> Option<String> {
    aroh_domain::category::guess(title, description).and_then(|slug| aroh_domain::seed::category_id(user_id, slug))
}

/// Adds an owner-defined category `name` (trimmed, unique ignoring case among
/// live categories) on `color`, one of the 6 hues; without `color` it takes the
/// next free hue, and past 6 categories the owner must pick one. Output: the row.
pub fn add_category(conn: &Connection, clock: &mut Hlc, user_id: &str, name: &str, color: Option<&str>) -> Result<Category> {
    let name = name.trim();
    let live = repo::list::<Category>(conn, &[("user_id", user_id)])?;
    if live.iter().any(|c| c.name.to_lowercase() == name.to_lowercase()) {
        return Err(invalid(format!("a category called {name} exists")));
    }
    let hues = aroh_domain::category::hues();
    let color = match color {
        Some(c) => aroh_domain::category::hue_slot(c).map(|i| hues[i]).ok_or_else(|| invalid("color must be one of the 6 category hues"))?,
        None => {
            let used: Vec<&str> = live.iter().map(|c| c.color.as_str()).collect();
            aroh_domain::category::next_hue(&used).ok_or_else(|| invalid("every hue is taken: pick one of the 6"))?
        }
    };
    let mut row = Category {
        id: aroh_domain::new_id(),
        user_id: user_id.to_string(),
        name: name.to_string(),
        color: color.to_string(),
        seeded: false,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut row)?;
    assert!(aroh_domain::category::hue_slot(&row.color).is_some());
    Ok(row)
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
