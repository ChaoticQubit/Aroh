use aroh_app::diary::{self, Keyring};
use aroh_app::{local, sample};
use aroh_diary::{DiaryKey, Source};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{DiaryDay, DiaryEntry, Proposal, Tracker, TrackerEntry};
use aroh_store::{db, repo, sync};
use chrono::{DateTime, Duration, TimeZone, Utc};
use rusqlite::Connection;
use serde_json::json;

const ACCOUNT: &str = "0190f2a0-0000-7000-8000-0000000000ac";
const DAY: &str = "2026-11-26";
const SECRET_WORDS: &str = "DIARY-SENTINEL tired after the call with my sister";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 26, 20, 15, 0).unwrap()
}

fn store(signed_in: bool) -> (Connection, Hlc) {
    let mut conn = db::open_in_memory().unwrap();
    let user = local::user_id(&conn).unwrap();
    let mut clock = Hlc::new("00000000000000d1");
    assert!(sample::seed(&conn, &mut clock, &user, now(), chrono_tz::Europe::Berlin).unwrap());
    if signed_in {
        local::adopt(&mut conn, ACCOUNT).unwrap();
    }
    (conn, clock)
}

fn ring(versions: &[i64]) -> Keyring {
    Keyring::new(versions.iter().map(|v| DiaryKey::generate(*v).unwrap()).collect()).unwrap()
}

fn weight(conn: &Connection) -> Tracker {
    repo::list::<Tracker>(conn, &[]).unwrap().into_iter().find(|t| t.name == "Weight").unwrap()
}

fn reply(conn: &Connection) -> String {
    let t = weight(conn);
    let field = &t.fields.iter().find(|f| f.kind == aroh_domain::rows::FieldKind::Number).unwrap().key;
    json!({"summary": "A heavy day; you went easy.", "mood": 2, "energy": 2, "highlights": ["Called your sister"], "food": null, "water": null,
        "workout": null, "notes_for_tomorrow": "Sleep early.", "tracker_values": [{"tracker_id": t.id, "field": field, "value": 71.5}]})
    .to_string()
}

#[test]
fn diary_refuses_without_an_account() {
    let (conn, mut clock) = store(false);
    let err = diary::append(&conn, &mut clock, &ring(&[1]), DAY, "hello", Source::Text, now()).unwrap_err();
    assert!(err.to_string().contains("sign in"), "{err}");
    assert!(diary::read_day(&conn, &ring(&[1]), DAY).is_err());
}

#[test]
fn append_keeps_sealed_entries_and_never_queues_a_log() {
    let (conn, mut clock) = store(true);
    let keys = ring(&[1]);
    let first = diary::append(&conn, &mut clock, &keys, DAY, SECRET_WORDS, Source::Voice, now()).unwrap();
    let second = diary::append(&conn, &mut clock, &keys, DAY, "Walked 20 minutes.", Source::Text, now() + Duration::minutes(5)).unwrap();
    let view = diary::read_day(&conn, &keys, DAY).unwrap();
    assert_eq!(view.entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>(), vec![first, second]);
    assert_eq!(view.entries[0].text, SECRET_WORDS);
    assert!(view.card.is_none() && view.stale);
    assert!(local::pending_logs(&conn).unwrap().is_empty(), "diary words never reach the plaintext log queue");
    let rows: Vec<DiaryEntry> = repo::list(&conn, &[]).unwrap();
    assert_eq!(rows.len(), 2);
    for table in aroh_domain::schema::SYNCED.iter() {
        let rows = sync::pending(&conn, table, ACCOUNT, clock.node(), "", 500).unwrap();
        assert!(!format!("{rows:?}").contains("DIARY-SENTINEL"), "{} would sync diary plaintext", table.name);
    }
    assert!(diary::read_day(&conn, &ring(&[2]), DAY).unwrap().locked == 2, "a foreign key opens nothing");
}

#[test]
fn structuring_seals_a_card_and_writes_numbers_only() {
    let (conn, mut clock) = store(true);
    let keys = ring(&[1]);
    diary::append(&conn, &mut clock, &keys, DAY, SECRET_WORDS, Source::Text, now()).unwrap();
    let view = diary::read_day(&conn, &keys, DAY).unwrap();
    let (prompt, fields) = diary::prompt_for(&conn, &view).unwrap().unwrap();
    assert!(prompt.user.contains("DIARY-SENTINEL"));
    let body = diary::structure_request(&prompt);
    assert_eq!(body["model"], diary::STRUCTURE_MODEL);
    let card = diary::save_structure(&conn, &mut clock, &keys, &view, &fields, &reply(&conn), now()).unwrap();
    assert_eq!(card.mood, 2);
    let again = diary::read_day(&conn, &keys, DAY).unwrap();
    assert!(!again.stale);
    assert_eq!(again.card.unwrap().summary, "A heavy day; you went easy.");
    let entries: Vec<TrackerEntry> = repo::list(&conn, &[("tracker_id", weight(&conn).id.as_str())]).unwrap();
    let from_diary: Vec<_> = entries.iter().filter(|e| e.logged_at == now()).collect();
    assert_eq!(from_diary.len(), 1);
    assert_eq!(from_diary[0].raw_text, None, "a tracker entry from the diary carries numbers only");
    let later = now() + Duration::minutes(30);
    diary::save_structure(&conn, &mut clock, &keys, &view, &fields, &reply(&conn), later).unwrap();
    let count = repo::list::<TrackerEntry>(&conn, &[]).unwrap().iter().filter(|e| e.logged_at == now() || e.logged_at == later).count();
    assert_eq!(count, 1, "restructuring replaces the day's number");
    assert!(diary::save_structure(&conn, &mut clock, &keys, &view, &fields, "Sure! here it is", now()).is_err());
    assert!(local::pending_logs(&conn).unwrap().is_empty());
}

#[test]
fn rotation_moves_every_row_to_the_new_key() {
    let (conn, mut clock) = store(true);
    let old = ring(&[1]);
    diary::append(&conn, &mut clock, &old, DAY, SECRET_WORDS, Source::Text, now()).unwrap();
    let view = diary::read_day(&conn, &old, DAY).unwrap();
    let (_, fields) = diary::prompt_for(&conn, &view).unwrap().unwrap();
    diary::save_structure(&conn, &mut clock, &old, &view, &fields, &reply(&conn), now()).unwrap();
    let mut proposal = repo::list::<Proposal>(&conn, &[]).unwrap().remove(0);
    let sealed = aroh_diary::seal(old.newest(), ACCOUNT, aroh_diary::Table::Proposals, &proposal.id, b"Because you wrote you were tired.").unwrap();
    proposal.sealed_summary = Some(json!(sealed));
    repo::put(&conn, &mut clock, &mut proposal).unwrap();
    assert_eq!(diary::rows_at(&conn, 1).unwrap(), 3);
    assert_eq!(diary::newest_version(&conn).unwrap(), 1);

    let new_key = DiaryKey::generate(2).unwrap();
    assert_eq!(diary::rotate(&conn, &mut clock, old.newest(), &new_key).unwrap(), 3);
    assert_eq!(diary::rows_at(&conn, 1).unwrap(), 0);
    let both = Keyring::new(vec![DiaryKey::from_bytes(2, new_key.bytes()).unwrap()]).unwrap();
    let view = diary::read_day(&conn, &both, DAY).unwrap();
    assert_eq!((view.entries.len(), view.locked), (1, 0));
    assert!(view.card.is_some());
    let row = repo::get::<Proposal>(&conn, &proposal.id).unwrap();
    assert_eq!(diary::proposal_summary(&both, &row).as_deref(), Some("Because you wrote you were tired."));
    assert!(repo::list::<DiaryDay>(&conn, &[]).unwrap().iter().all(|d| d.key_version == 2));
}

#[test]
fn reply_text_needs_a_finished_text_reply() {
    let ok = json!({"stop_reason": "end_turn", "content": [{"type": "thinking", "thinking": ""}, {"type": "text", "text": "{}"}]});
    assert_eq!(diary::reply_text(&ok).unwrap(), "{}");
    for bad in [json!({"stop_reason": "refusal", "content": []}), json!({"stop_reason": "max_tokens", "content": [{"type": "text", "text": "{"}]}), json!({"stop_reason": "end_turn", "content": []})] {
        assert!(diary::reply_text(&bad).is_err());
    }
}

#[test]
fn an_unfinished_rotation_is_found_and_resumed() {
    let (conn, mut clock) = store(true);
    let v1 = DiaryKey::generate(1).unwrap();
    let v2 = DiaryKey::generate(2).unwrap();
    let copy = |k: &DiaryKey| DiaryKey::from_bytes(k.version(), k.bytes()).unwrap();
    let old = Keyring::new(vec![copy(&v1)]).unwrap();
    diary::append(&conn, &mut clock, &old, DAY, SECRET_WORDS, Source::Text, now()).unwrap();
    diary::append(&conn, &mut clock, &old, DAY, "Second entry.", Source::Text, now() + Duration::minutes(1)).unwrap();
    assert!(!diary::rotation_unfinished(&conn, &old).unwrap());

    let both = Keyring::new(vec![copy(&v1), copy(&v2)]).unwrap();
    let rows: Vec<DiaryEntry> = repo::list(&conn, &[]).unwrap();
    let mut first = rows.into_iter().min_by_key(|r| r.at).unwrap();
    let s = aroh_diary::rotate(&v1, &v2, ACCOUNT, aroh_diary::Table::Entries, &first.id, &aroh_diary::Sealed { key_version: 1, nonce: first.nonce.clone(), ciphertext: first.ciphertext.clone() }).unwrap();
    (first.key_version, first.nonce, first.ciphertext) = (s.key_version, s.nonce, s.ciphertext);
    repo::put(&conn, &mut clock, &mut first).unwrap();
    assert!(diary::rotation_unfinished(&conn, &both).unwrap(), "two keys held after a rotation stopped half way");
    let only_new = Keyring::new(vec![copy(&v2)]).unwrap();
    assert!(diary::rotation_unfinished(&conn, &only_new).unwrap(), "a row is still below the newest key");

    assert_eq!(diary::resume_rotation(&conn, &mut clock, &both).unwrap(), 1);
    assert_eq!(diary::rows_at(&conn, 1).unwrap(), 0);
    assert!(!diary::rotation_unfinished(&conn, &only_new).unwrap());
    assert_eq!(diary::read_day(&conn, &only_new, DAY).unwrap().entries.len(), 2);
}

#[test]
fn a_sealed_diff_applies_only_once_opened() {
    let (conn, mut clock) = store(true);
    let keys = ring(&[1]);
    let mut proposal = repo::list::<Proposal>(&conn, &[]).unwrap().into_iter().find(|p| p.status == aroh_domain::rows::ProposalStatus::Proposed).unwrap();
    let plain = proposal.diff.clone();
    let sealed = aroh_diary::seal(keys.newest(), ACCOUNT, aroh_diary::Table::ProposalDiffs, &proposal.id, &serde_json::to_vec(&plain).unwrap()).unwrap();
    proposal.diff = json!({"sealed": sealed});
    repo::put(&conn, &mut clock, &mut proposal).unwrap();
    assert!(aroh_app::write::decide_proposal(&conn, &mut clock, &proposal.id, true).is_err(), "a sealed diff never applies blind");
    let opened = diary::proposal_diff(&keys, &repo::get::<Proposal>(&conn, &proposal.id).unwrap()).unwrap();
    assert_eq!(opened, plain);
    assert!(diary::proposal_diff(&ring(&[1]), &repo::get::<Proposal>(&conn, &proposal.id).unwrap()).is_none(), "another key opens nothing");
    let done = aroh_app::write::decide_proposal_with(&conn, &mut clock, &proposal.id, true, Some(opened)).unwrap();
    assert_eq!(done.status, aroh_domain::rows::ProposalStatus::Applied);
}

#[test]
fn a_card_chat_opens_its_sealed_parts() {
    let (conn, mut clock) = store(true);
    let keys = ring(&[1]);
    let mut proposal = repo::list::<Proposal>(&conn, &[]).unwrap().into_iter().next().unwrap();
    let seal = |part: &str, v: serde_json::Value| json!({"sealed": aroh_diary::seal(keys.newest(), ACCOUNT, aroh_diary::Table::ProposalDiffs, &format!("{}/{part}", proposal.id), &serde_json::to_vec(&v).unwrap()).unwrap()});
    proposal.diff = json!({"op": "card", "request": seal("request", json!("shorter today")), "after": seal("after", json!({"minutes": 20})), "count": 1});
    repo::put(&conn, &mut clock, &mut proposal).unwrap();
    let row = repo::get::<Proposal>(&conn, &proposal.id).unwrap();
    assert_eq!(diary::proposal_diff(&keys, &row).unwrap(), json!({"op": "card", "request": "shorter today", "after": {"minutes": 20}, "count": 1}));
    assert!(diary::proposal_diff(&ring(&[1]), &row).is_none(), "another key opens nothing");
}
