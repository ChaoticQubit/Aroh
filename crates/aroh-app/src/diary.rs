//! The device side of the encrypted diary (Q85): appending sealed entries,
//! reading a day back, saving the structured card and its tracker numbers,
//! key rotation, and the Messages request and reply that structure a day.
//! Keys are passed in and nothing here stores them or calls the network.

use crate::{invalid, local, Result};
use aroh_diary::{day_id, Card, DiaryKey, Entry, NumericField, Prompt, Sealed, Source, Structure, Table};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{DiaryDay, DiaryEntry, EntrySource, FieldKind, Proposal, Tracker, TrackerEntry};
use aroh_store::repo;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Map, Value};

/// Every diary key this device holds, oldest first. Rows are opened by their
/// own version; new rows are sealed with the newest.
pub struct Keyring(Vec<DiaryKey>);

impl Keyring {
    /// Builds a ring from `keys`; they must have distinct versions.
    /// Returns `Invalid` for an empty ring or a repeated version.
    pub fn new(mut keys: Vec<DiaryKey>) -> Result<Keyring> {
        keys.sort_by_key(DiaryKey::version);
        if keys.is_empty() || keys.windows(2).any(|w| w[0].version() == w[1].version()) {
            return Err(invalid("diary keys must be at least one, each version once"));
        }
        Ok(Keyring(keys))
    }

    /// The key new rows are sealed with: the highest version.
    pub fn newest(&self) -> &DiaryKey {
        self.0.last().expect("a ring holds at least one key")
    }

    /// Every key, oldest first.
    pub fn all(&self) -> &[DiaryKey] {
        &self.0
    }

    /// The key of `version`, if this device holds it.
    pub fn get(&self, version: i64) -> Option<&DiaryKey> {
        self.0.iter().find(|k| k.version() == version)
    }
}

/// One entry as the card view shows it: id, time, words and how they came.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EntryView {
    pub id: String,
    pub at: DateTime<Utc>,
    pub text: String,
    pub source: Source,
}

/// One diary day for the webview: its verbatim entries, the card when one
/// opens, and `stale` when an entry is not covered by the card yet.
/// `locked` counts rows sealed with a key this device does not hold.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayView {
    pub day: String,
    pub entries: Vec<EntryView>,
    pub card: Option<Card>,
    pub stale: bool,
    pub locked: usize,
}

/// The signed-in account the diary belongs to. Errors with `Invalid` before
/// sign-in: the key's associated data and the card id use the account id.
pub fn account(conn: &Connection) -> Result<String> {
    local::account(conn)?.ok_or_else(|| invalid("sign in to keep a diary"))
}

/// Seals `text` as a new immutable entry of local `day`, logged `at` via
/// `source`, with the newest key. The words are durable before any network
/// call and never reach the plaintext log queue. Returns the entry id.
pub fn append(conn: &Connection, clock: &mut Hlc, ring: &Keyring, day: &str, text: &str, source: Source, at: DateTime<Utc>) -> Result<String> {
    let user = account(conn)?;
    aroh_diary::card::check_day(day).map_err(|e| invalid(e.to_string()))?;
    let entry = Entry { at, text: text.trim().to_string(), source };
    entry.validate().map_err(|e| invalid(e.to_string()))?;
    let id = aroh_domain::new_id();
    let key = ring.newest();
    let sealed = aroh_diary::seal(key, &user, Table::Entries, &id, &entry.to_bytes()).map_err(|e| invalid(e.to_string()))?;
    let mut row = DiaryEntry { id: id.clone(), user_id: user, day: day.to_string(), at, key_version: sealed.key_version, nonce: sealed.nonce, ciphertext: sealed.ciphertext, updated_at: String::new(), deleted: false };
    repo::put(conn, clock, &mut row)?;
    Ok(id)
}

fn sealed_of(version: i64, nonce: &str, ciphertext: &str) -> Sealed {
    Sealed { key_version: version, nonce: nonce.to_string(), ciphertext: ciphertext.to_string() }
}

/// Opens local `day` with `ring`: entries oldest first, the card if one
/// exists and opens, whether it is stale, and how many rows stayed locked.
/// Returns `Invalid` before sign-in.
pub fn read_day(conn: &Connection, ring: &Keyring, day: &str) -> Result<DayView> {
    let user = account(conn)?;
    let rows: Vec<DiaryEntry> = repo::list(conn, &[("user_id", user.as_str()), ("day", day)])?;
    let mut entries = Vec::new();
    let mut locked = 0;
    for row in rows.into_iter().filter(|r| !r.deleted) {
        let opened = ring.get(row.key_version).and_then(|k| aroh_diary::open(k, &user, Table::Entries, &row.id, &sealed_of(row.key_version, &row.nonce, &row.ciphertext)).ok());
        match opened.and_then(|p| Entry::from_bytes(&p).ok()) {
            Some(e) => entries.push(EntryView { id: row.id, at: e.at, text: e.text, source: e.source }),
            None => locked += 1,
        }
    }
    entries.sort_by(|a, b| (a.at, &a.id).cmp(&(b.at, &b.id)));
    let id = day_id(&user, day);
    let card = match repo::get::<DiaryDay>(conn, &id) {
        Ok(row) if !row.deleted => {
            let opened = ring.get(row.key_version).and_then(|k| aroh_diary::open(k, &user, Table::Days, &row.id, &sealed_of(row.key_version, &row.nonce, &row.ciphertext)).ok());
            let card = opened.and_then(|p| Card::from_bytes(&p).ok());
            if card.is_none() {
                locked += 1;
            }
            card
        }
        _ => None,
    };
    let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    let stale = !entries.is_empty() && card.as_ref().is_none_or(|c| c.is_stale(&ids));
    Ok(DayView { day: day.to_string(), entries, card, stale, locked })
}

/// The number fields of the account's live trackers, the only places a
/// structuring may write a value to. Returns them in tracker order.
pub fn numeric_fields(conn: &Connection) -> Result<Vec<NumericField>> {
    let user = account(conn)?;
    let trackers: Vec<Tracker> = repo::list(conn, &[("user_id", user.as_str())])?;
    Ok(trackers
        .iter()
        .filter(|t| !t.deleted)
        .flat_map(|t| t.fields.iter().filter(|f| f.kind == FieldKind::Number).map(move |f| NumericField { tracker_id: t.id.clone(), key: f.key.clone(), label: f.label.clone(), unit: f.unit.clone() }))
        .collect())
}

/// The structuring prompt for `view`, or None when it has no entries.
pub fn prompt_for(conn: &Connection, view: &DayView) -> Result<Option<(Prompt, Vec<NumericField>)>> {
    if view.entries.is_empty() {
        return Ok(None);
    }
    let fields = numeric_fields(conn)?;
    let entries: Vec<Entry> = view.entries.iter().map(|e| Entry { at: e.at, text: e.text.clone(), source: e.source }).collect();
    let prompt = aroh_diary::prompt(&view.day, &entries, &fields).map_err(|e| invalid(e.to_string()))?;
    Ok(Some((prompt, fields)))
}

/// Saves the model's `reply` for `view` as the day's sealed card covering its
/// entries, and its tracker numbers as `log` entries with numbers only and no
/// raw text, one row per day and field so a restructure replaces it.
/// Returns the card, or `Invalid` when the reply does not parse.
pub fn save_structure(conn: &Connection, clock: &mut Hlc, ring: &Keyring, view: &DayView, fields: &[NumericField], reply: &str, now: DateTime<Utc>) -> Result<Card> {
    let user = account(conn)?;
    let structure: Structure = aroh_diary::parse(reply, fields).map_err(|e| invalid(e.to_string()))?;
    let values = structure.tracker_values.clone();
    let ids = view.entries.iter().map(|e| e.id.clone()).collect();
    let card = structure.into_card(&view.day, ids, now).map_err(|e| invalid(e.to_string()))?;
    let id = day_id(&user, &view.day);
    let sealed = aroh_diary::seal(ring.newest(), &user, Table::Days, &id, &card.to_bytes()).map_err(|e| invalid(e.to_string()))?;
    let mut row = DiaryDay { id, user_id: user.clone(), day: view.day.clone(), key_version: sealed.key_version, nonce: sealed.nonce, ciphertext: sealed.ciphertext, updated_at: String::new(), deleted: false };
    repo::put(conn, clock, &mut row)?;
    for v in values {
        let mut data = Map::new();
        data.insert(v.field.clone(), json!(v.value));
        let mut entry = TrackerEntry {
            id: aroh_domain::derived_id(&user, &format!("diary:{}:{}:{}", view.day, v.tracker_id, v.field), 0),
            user_id: user.clone(),
            tracker_id: v.tracker_id,
            data,
            source: EntrySource::Log,
            raw_text: None,
            logged_at: now,
            updated_at: String::new(),
            deleted: false,
        };
        if let Ok(old) = repo::get::<TrackerEntry>(conn, &entry.id) {
            entry.updated_at = old.updated_at;
        }
        repo::put(conn, clock, &mut entry)?;
    }
    Ok(card)
}

/// Re-encrypts every diary entry, card and sealed proposal summary of the
/// account still at `old`'s version under `new` (toggle off, or a straggler
/// another device wrote). Returns how many rows moved.
pub fn rotate(conn: &Connection, clock: &mut Hlc, old: &DiaryKey, new: &DiaryKey) -> Result<usize> {
    assert!(new.version() > old.version(), "rotation moves to a newer key");
    let user = account(conn)?;
    let version = old.version().to_string();
    let filter = [("user_id", user.as_str()), ("key_version", version.as_str())];
    let move_one = |table: Table, id: &str, s: &Sealed| aroh_diary::rotate(old, new, &user, table, id, s).map_err(|e| invalid(e.to_string()));
    let mut moved = 0;
    for mut row in repo::list::<DiaryEntry>(conn, &filter)? {
        let s = move_one(Table::Entries, &row.id, &sealed_of(row.key_version, &row.nonce, &row.ciphertext))?;
        (row.key_version, row.nonce, row.ciphertext) = (s.key_version, s.nonce, s.ciphertext);
        repo::put(conn, clock, &mut row)?;
        moved += 1;
    }
    for mut row in repo::list::<DiaryDay>(conn, &filter)? {
        let s = move_one(Table::Days, &row.id, &sealed_of(row.key_version, &row.nonce, &row.ciphertext))?;
        (row.key_version, row.nonce, row.ciphertext) = (s.key_version, s.nonce, s.ciphertext);
        repo::put(conn, clock, &mut row)?;
        moved += 1;
    }
    for mut row in repo::list::<Proposal>(conn, &[("user_id", user.as_str())])? {
        let Some(stored) = row.sealed_summary.clone() else { continue };
        let Ok(s) = serde_json::from_value::<Sealed>(stored) else { continue };
        if s.key_version != old.version() {
            continue;
        }
        row.sealed_summary = Some(json!(move_one(Table::Proposals, &row.id, &s)?));
        repo::put(conn, clock, &mut row)?;
        moved += 1;
    }
    Ok(moved)
}

/// Whether a key rotation was left unfinished: this device holds more than one
/// key, or diary rows or sealed summaries use a version below the newest key.
pub fn rotation_unfinished(conn: &Connection, ring: &Keyring) -> Result<bool> {
    if ring.all().len() > 1 {
        return Ok(true);
    }
    let newest = ring.newest().version();
    for v in 1..newest {
        if rows_at(conn, v)? > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Finishes an interrupted rotation: re-encrypts the rows of every older key in
/// `ring` under its newest key. Rows of a version this device lacks stay as
/// they are. Returns how many rows moved.
pub fn resume_rotation(conn: &Connection, clock: &mut Hlc, ring: &Keyring) -> Result<usize> {
    let newest = ring.newest();
    let mut moved = 0;
    for old in ring.all().iter().filter(|k| k.version() < newest.version()) {
        moved += rotate(conn, clock, old, newest)?;
    }
    Ok(moved)
}

/// How many of the account's diary rows and sealed summaries are still at
/// `version`. Key `version` may be deleted only when a full pull leaves none.
pub fn rows_at(conn: &Connection, version: i64) -> Result<usize> {
    let user = account(conn)?;
    let mut n: i64 = 0;
    for table in ["diary_entries", "diary_days"] {
        n += conn.query_row(&format!("SELECT count(*) FROM {table} WHERE user_id = ?1 AND key_version = ?2"), rusqlite::params![user, version], |r| r.get::<_, i64>(0))?;
    }
    n += conn.query_row(
        "SELECT count(*) FROM proposals WHERE user_id = ?1 AND sealed_summary IS NOT NULL AND json_extract(sealed_summary, '$.key_version') = ?2",
        rusqlite::params![user, version],
        |r| r.get::<_, i64>(0),
    )?;
    Ok(usize::try_from(n).expect("counts are non-negative"))
}

/// The newest key version any diary row of the account uses, 0 for none.
pub fn newest_version(conn: &Connection) -> Result<i64> {
    let user = account(conn)?;
    let mut top: i64 = 0;
    for table in ["diary_entries", "diary_days"] {
        let v: Option<i64> = conn.query_row(&format!("SELECT max(key_version) FROM {table} WHERE user_id = ?1"), [&user], |r| r.get(0))?;
        top = top.max(v.unwrap_or(0));
    }
    Ok(top)
}

/// Opens the sealed diff of proposal `row` (a diary run's) with `ring`: a whole
/// sealed diff, or a card chat's sealed parts each under `{id}/{part}`. None for
/// an ordinary diff, a withheld one or a key this device lacks.
pub fn proposal_diff(ring: &Keyring, row: &Proposal) -> Option<Value> {
    let open = |id: &str, v: &Value| -> Option<Value> {
        let s: Sealed = serde_json::from_value(v.get("sealed")?.clone()).ok()?;
        let plain = aroh_diary::open(ring.get(s.key_version)?, &row.user_id, Table::ProposalDiffs, id, &s).ok()?;
        serde_json::from_slice(&plain).ok()
    };
    if row.diff.get("sealed").is_some() {
        return open(&row.id, &row.diff);
    }
    let mut diff = row.diff.as_object()?.clone();
    let parts: Vec<String> = diff.iter().filter(|(_, v)| v.get("sealed").is_some()).map(|(k, _)| k.clone()).collect();
    if parts.is_empty() {
        return None;
    }
    for part in parts {
        let opened = open(&format!("{}/{part}", row.id), &diff[&part])?;
        diff.insert(part, opened);
    }
    Some(Value::Object(diff))
}

/// Opens the sealed summary of proposal `row` with `ring`, None for an
/// ordinary proposal or one sealed with a key this device lacks.
pub fn proposal_summary(ring: &Keyring, row: &Proposal) -> Option<String> {
    let s: Sealed = serde_json::from_value(row.sealed_summary.clone()?).ok()?;
    let key = ring.get(s.key_version)?;
    let plain = aroh_diary::open(key, &row.user_id, Table::Proposals, &row.id, &s).ok()?;
    String::from_utf8(plain.to_vec()).ok()
}

/// The Anthropic Messages endpoint the device structures a day with.
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
/// The Messages API version the request is written for.
pub const ANTHROPIC_VERSION: &str = "2026-10-21";
/// The model the device structures a day with.
pub const STRUCTURE_MODEL: &str = "claude-opus-5-5";

/// The Messages API body for `prompt`: one user turn at low effort, sent
/// from the device with the owner's own key. Holds diary text, so the caller
/// sends it and drops it without logging.
pub fn structure_request(prompt: &Prompt) -> Value {
    json!({
        "model": STRUCTURE_MODEL,
        "max_tokens": 16000,
        "output_config": {"effort": "low"},
        "system": prompt.system,
        "messages": [{"role": "user", "content": prompt.user.as_str()}],
    })
}

/// The text of a Messages API `reply`, its text blocks joined. Returns
/// `Invalid` for a refusal, a cut-off reply or one with no text; the message
/// names only the reason, never the reply.
pub fn reply_text(reply: &Value) -> Result<String> {
    match reply["stop_reason"].as_str() {
        Some("end_turn") => {}
        Some("refusal") => return Err(invalid("the model declined to structure this day")),
        Some("max_tokens") => return Err(invalid("the model reply was cut off")),
        _ => return Err(invalid("the model reply did not finish")),
    }
    let text: String = reply["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect();
    if text.trim().is_empty() {
        return Err(invalid("the model reply had no text"));
    }
    Ok(text)
}
