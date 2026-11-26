use aroh_app::diary::{self, Keyring};
use aroh_app::{local, sample};
use aroh_diary::{CardEdit, DiaryKey, Source, Table};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{DiaryDay, Tracker};
use aroh_store::{db, repo, sync};
use chrono::{DateTime, Duration, TimeZone, Utc};
use rusqlite::Connection;
use serde_json::json;

const ACCOUNT: &str = "0190f2a0-0000-7000-8000-0000000000ac";
const DAY: &str = "2026-11-26";
const SECRET_WORDS: &str = "DIARY-SENTINEL tired after the call with my sister";
const SECRET_EDIT: &str = "EDIT-SENTINEL a calmer evening than I wrote";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 26, 20, 15, 0).unwrap()
}

fn store() -> (Connection, Hlc) {
    let mut conn = db::open_in_memory().unwrap();
    let user = local::user_id(&conn).unwrap();
    let mut clock = Hlc::new("00000000000000d1");
    assert!(sample::seed(&conn, &mut clock, &user, now(), chrono_tz::Europe::Berlin).unwrap());
    local::adopt(&mut conn, ACCOUNT).unwrap();
    (conn, clock)
}

fn ring(keys: &[&DiaryKey]) -> Keyring {
    Keyring::new(keys.iter().map(|k| DiaryKey::from_bytes(k.version(), k.bytes()).unwrap()).collect()).unwrap()
}

fn reply(conn: &Connection) -> String {
    let t = repo::list::<Tracker>(conn, &[]).unwrap().into_iter().find(|t| t.name == "Weight").unwrap();
    let field = &t.fields.iter().find(|f| f.kind == aroh_domain::rows::FieldKind::Number).unwrap().key;
    json!({"summary": "A heavy day; you went easy.", "mood": 2, "energy": 2, "highlights": ["Called your sister"], "food": null, "water": null,
        "workout": null, "notes_for_tomorrow": "Sleep early.", "tracker_values": [{"tracker_id": t.id, "field": field, "value": 71.5}]})
    .to_string()
}

/// Structures `DAY` the way the engine does (Q94) with the newest key of `keys`.
fn structure(conn: &Connection, clock: &mut Hlc, keys: &Keyring, at: DateTime<Utc>) -> aroh_diary::Card {
    let view = diary::read_day(conn, keys, DAY).unwrap();
    let request = diary::structure_request(conn, &view).unwrap().unwrap();
    let server = DiaryKey::from_bytes(keys.newest().version(), keys.newest().bytes()).unwrap();
    let answer = request.answer(&server, ACCOUNT, &reply(conn), at).unwrap();
    diary::save_card(conn, clock, keys, &view, answer, at).unwrap()
}

fn edit() -> CardEdit {
    CardEdit {
        summary: SECRET_EDIT.into(),
        mood: 4,
        energy: 3,
        highlights: vec!["Called your sister".into(), "Long walk".into()],
        food: Some("Soup".into()),
        water: Some("2 litres".into()),
        workout: None,
        notes_for_tomorrow: Some("Call back.".into()),
    }
}

fn no_plaintext_queued(conn: &Connection, clock: &Hlc) {
    for table in aroh_domain::schema::SYNCED.iter() {
        let rows = sync::pending(conn, table, ACCOUNT, clock.node(), "", 500).unwrap();
        let text = format!("{rows:?}");
        assert!(!text.contains("DIARY-SENTINEL") && !text.contains("EDIT-SENTINEL"), "{} would sync diary plaintext", table.name);
    }
}

#[test]
fn an_edited_card_is_resealed_keeps_its_entries_and_is_not_stale() {
    let (conn, mut clock) = store();
    let key = DiaryKey::generate(1).unwrap();
    let keys = ring(&[&key]);
    diary::append(&conn, &mut clock, &keys, DAY, SECRET_WORDS, Source::Text, now()).unwrap();
    let structured = structure(&conn, &mut clock, &keys, now());
    let id = aroh_diary::day_id(ACCOUNT, DAY);
    let before: DiaryDay = repo::get(&conn, &id).unwrap();
    let at = now() + Duration::minutes(10);
    let card = diary::edit_card(&conn, &mut clock, &keys, DAY, edit(), at).unwrap();
    assert_eq!(card.summary, SECRET_EDIT);
    assert_eq!(card.entry_ids, structured.entry_ids);
    assert_eq!(card.edited_at, Some(at));
    let after: DiaryDay = repo::get(&conn, &id).unwrap();
    assert_ne!(after.ciphertext, before.ciphertext);
    assert!(after.updated_at > before.updated_at, "the edit wins last-writer-wins against the structured card");
    let opened = aroh_diary::open(&key, ACCOUNT, Table::Days, &id, &aroh_diary::Sealed { key_version: 1, nonce: after.nonce, ciphertext: after.ciphertext }).unwrap();
    assert_eq!(aroh_diary::Card::from_bytes(&opened).unwrap(), card);
    let view = diary::read_day(&conn, &keys, DAY).unwrap();
    assert!(!view.stale);
    assert_eq!(view.card.unwrap(), card);
    no_plaintext_queued(&conn, &clock);
    assert_eq!(diary::read_day(&conn, &ring(&[&DiaryKey::generate(1).unwrap()]), DAY).unwrap().locked, 2, "a foreign key opens nothing");
}

#[test]
fn an_edit_is_refused_without_a_card_or_out_of_bounds() {
    let (conn, mut clock) = store();
    let keys = ring(&[&DiaryKey::generate(1).unwrap()]);
    diary::append(&conn, &mut clock, &keys, DAY, SECRET_WORDS, Source::Text, now()).unwrap();
    assert!(diary::edit_card(&conn, &mut clock, &keys, DAY, edit(), now()).is_err(), "nothing to edit before a card exists");
    let structured = structure(&conn, &mut clock, &keys, now());
    let err = diary::edit_card(&conn, &mut clock, &keys, DAY, CardEdit { mood: 6, ..edit() }, now()).unwrap_err().to_string();
    assert!(err.contains("mood") && !err.contains("SENTINEL"), "{err}");
    assert!(diary::edit_card(&conn, &mut clock, &keys, DAY, CardEdit { summary: " ".into(), ..edit() }, now()).is_err());
    assert_eq!(diary::read_day(&conn, &keys, DAY).unwrap().card.unwrap(), structured, "a refused edit changes nothing");
}

#[test]
fn an_edit_seals_with_the_newest_key() {
    let (conn, mut clock) = store();
    let old = DiaryKey::generate(1).unwrap();
    let keys = ring(&[&old]);
    diary::append(&conn, &mut clock, &keys, DAY, SECRET_WORDS, Source::Text, now()).unwrap();
    structure(&conn, &mut clock, &keys, now());
    let both = ring(&[&old, &DiaryKey::generate(2).unwrap()]);
    diary::edit_card(&conn, &mut clock, &both, DAY, edit(), now()).unwrap();
    assert_eq!(repo::get::<DiaryDay>(&conn, &aroh_diary::day_id(ACCOUNT, DAY)).unwrap().key_version, 2);
    assert_eq!(diary::read_day(&conn, &both, DAY).unwrap().card.unwrap().summary, SECRET_EDIT);
}

#[test]
fn an_edited_card_is_restructured_only_when_forced() {
    let (conn, mut clock) = store();
    let keys = ring(&[&DiaryKey::generate(1).unwrap()]);
    assert!(!diary::structure_due(&diary::read_day(&conn, &keys, DAY).unwrap(), true), "an empty day is never sent");
    diary::append(&conn, &mut clock, &keys, DAY, SECRET_WORDS, Source::Text, now()).unwrap();
    assert!(diary::structure_due(&diary::read_day(&conn, &keys, DAY).unwrap(), false));
    structure(&conn, &mut clock, &keys, now());
    let fresh = diary::read_day(&conn, &keys, DAY).unwrap();
    assert!(!diary::structure_due(&fresh, false));
    assert!(diary::structure_due(&fresh, true), "Redo summary structures a fresh card again");
    diary::append(&conn, &mut clock, &keys, DAY, "Second entry.", Source::Text, now() + Duration::minutes(1)).unwrap();
    assert!(diary::structure_due(&diary::read_day(&conn, &keys, DAY).unwrap(), false), "an unedited stale card is rebuilt");
    structure(&conn, &mut clock, &keys, now() + Duration::minutes(2));
    diary::edit_card(&conn, &mut clock, &keys, DAY, edit(), now() + Duration::minutes(3)).unwrap();
    diary::append(&conn, &mut clock, &keys, DAY, "Third entry.", Source::Text, now() + Duration::minutes(4)).unwrap();
    let edited = diary::read_day(&conn, &keys, DAY).unwrap();
    assert!(edited.stale);
    assert!(!diary::structure_due(&edited, false), "a new entry never silently replaces a manual edit");
    assert!(diary::structure_due(&edited, true));
    let redone = structure(&conn, &mut clock, &keys, now() + Duration::minutes(5));
    assert_eq!(redone.edited_at, None, "a confirmed redo is the engine's card again");
}

#[test]
fn entries_are_edited_in_place_and_deleted_with_their_words() {
    let (conn, mut clock) = store();
    let key = DiaryKey::generate(1).unwrap();
    let keys = ring(&[&key]);
    let first = diary::append(&conn, &mut clock, &keys, DAY, SECRET_WORDS, Source::Voice, now()).unwrap();
    let second = diary::append(&conn, &mut clock, &keys, DAY, "Walked 20 minutes.", Source::Text, now() + Duration::minutes(5)).unwrap();
    structure(&conn, &mut clock, &keys, now());
    diary::edit_entry(&conn, &mut clock, &keys, &first, &format!("  {SECRET_EDIT}  ")).unwrap();
    let view = diary::read_day(&conn, &keys, DAY).unwrap();
    assert_eq!(view.entries[0].id, first);
    assert_eq!(view.entries[0].text, SECRET_EDIT);
    assert_eq!((view.entries[0].at, view.entries[0].source), (now(), Source::Voice), "an edit keeps when and how the words came");
    assert!(!view.stale);
    assert!(diary::edit_entry(&conn, &mut clock, &keys, &first, "   ").is_err());
    assert!(diary::edit_entry(&conn, &mut clock, &keys, "0190f2a0-0000-7000-8000-000000000999", "x").is_err());
    no_plaintext_queued(&conn, &clock);

    diary::delete_entry(&conn, &mut clock, &keys, &first).unwrap();
    let view = diary::read_day(&conn, &keys, DAY).unwrap();
    assert_eq!(view.entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>(), vec![second.clone()]);
    assert!(view.card.is_some(), "the card stays while words remain");
    let (version, nonce, ciphertext): (i64, String, String) =
        conn.query_row("SELECT key_version, nonce, ciphertext FROM diary_entries WHERE id = ?1 AND deleted = 1", [&first], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    let left = aroh_diary::open(&key, ACCOUNT, Table::Entries, &first, &aroh_diary::Sealed { key_version: version, nonce, ciphertext }).unwrap();
    assert_eq!(left.as_slice(), b"{}", "a deleted entry keeps no words, not even sealed");
    assert!(diary::delete_entry(&conn, &mut clock, &keys, &first).is_err(), "already deleted");

    diary::delete_entry(&conn, &mut clock, &keys, &second).unwrap();
    let empty = diary::read_day(&conn, &keys, DAY).unwrap();
    assert!(empty.entries.is_empty() && empty.card.is_none(), "deleting the last entry deletes the day's card");
    let (version, nonce, ciphertext): (i64, String, String) = conn
        .query_row("SELECT key_version, nonce, ciphertext FROM diary_days WHERE id = ?1 AND deleted = 1", [aroh_diary::day_id(ACCOUNT, DAY)], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    let left = aroh_diary::open(&key, ACCOUNT, Table::Days, &aroh_diary::day_id(ACCOUNT, DAY), &aroh_diary::Sealed { key_version: version, nonce, ciphertext }).unwrap();
    assert_eq!(left.as_slice(), b"{}");
}

#[test]
fn a_moved_entry_opens_on_its_new_day_and_both_cards_go_stale() {
    let (conn, mut clock) = store();
    let key = DiaryKey::generate(1).unwrap();
    let keys = ring(&[&key]);
    let late = diary::append(&conn, &mut clock, &keys, DAY, SECRET_WORDS, Source::Voice, now()).unwrap();
    let kept = diary::append(&conn, &mut clock, &keys, DAY, "Walked 20 minutes.", Source::Text, now() + Duration::minutes(5)).unwrap();
    structure(&conn, &mut clock, &keys, now());
    assert!(diary::move_entry(&conn, &mut clock, &keys, &late, DAY).is_err(), "already on that day");
    assert!(diary::move_entry(&conn, &mut clock, &keys, &late, "2026-11-31").is_err());
    let from = diary::move_entry(&conn, &mut clock, &keys, &late, "2026-11-25").unwrap();
    assert_eq!(from, DAY);
    let yesterday = diary::read_day(&conn, &keys, "2026-11-25").unwrap();
    assert_eq!(yesterday.entries.len(), 1);
    assert_eq!((yesterday.entries[0].id.as_str(), yesterday.entries[0].text.as_str(), yesterday.entries[0].at), (late.as_str(), SECRET_WORDS, now()));
    assert!(yesterday.stale && diary::structure_due(&yesterday, false));
    let today = diary::read_day(&conn, &keys, DAY).unwrap();
    assert_eq!(today.entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>(), vec![kept.clone()]);
    assert!(today.stale, "a card covering an entry that left the day is rebuilt");
    assert!(diary::structure_due(&today, false));
    let row: aroh_domain::rows::DiaryEntry = repo::get(&conn, &late).unwrap();
    assert_eq!(row.day, "2026-11-25");
    let opened = aroh_diary::open(&key, ACCOUNT, Table::Entries, &late, &aroh_diary::Sealed { key_version: row.key_version, nonce: row.nonce, ciphertext: row.ciphertext }).unwrap();
    assert_eq!(aroh_diary::Entry::from_bytes(&opened).unwrap().text, SECRET_WORDS, "the moved row opens with the diary key");
    no_plaintext_queued(&conn, &clock);
    diary::move_entry(&conn, &mut clock, &keys, &kept, "2026-11-25").unwrap();
    let emptied = diary::read_day(&conn, &keys, DAY).unwrap();
    assert!(emptied.entries.is_empty() && emptied.card.is_none(), "moving the last entry away deletes the day's card");
}

#[test]
fn engine_moves_apply_only_inside_the_window_and_to_this_days_entries() {
    let (conn, mut clock) = store();
    let keys = ring(&[&DiaryKey::generate(1).unwrap()]);
    let first = diary::append(&conn, &mut clock, &keys, DAY, SECRET_WORDS, Source::Voice, now()).unwrap();
    let second = diary::append(&conn, &mut clock, &keys, DAY, "Today was fine.", Source::Text, now() + Duration::minutes(5)).unwrap();
    let view = diary::read_day(&conn, &keys, DAY).unwrap();
    let mv = |id: &str, day: &str| aroh_diary::EntryMove { id: id.into(), day: day.into() };
    let moves = vec![mv(&first, "2026-11-25"), mv(&second, "2026-11-27"), mv(&second, "2026-11-18"), mv("0190f2a0-0000-7000-8000-000000000999", "2026-11-25")];
    assert_eq!(diary::apply_moves(&conn, &mut clock, &keys, &view, &moves).unwrap(), vec!["2026-11-25".to_string()]);
    assert_eq!(diary::read_day(&conn, &keys, "2026-11-25").unwrap().entries[0].id, first);
    assert_eq!(diary::read_day(&conn, &keys, DAY).unwrap().entries[0].id, second, "out-of-window moves are dropped");
}

#[test]
fn a_deleted_entry_makes_the_card_stale() {
    let (conn, mut clock) = store();
    let keys = ring(&[&DiaryKey::generate(1).unwrap()]);
    let first = diary::append(&conn, &mut clock, &keys, DAY, SECRET_WORDS, Source::Text, now()).unwrap();
    diary::append(&conn, &mut clock, &keys, DAY, "Walked.", Source::Text, now() + Duration::minutes(5)).unwrap();
    structure(&conn, &mut clock, &keys, now());
    diary::delete_entry(&conn, &mut clock, &keys, &first).unwrap();
    assert!(diary::read_day(&conn, &keys, DAY).unwrap().stale, "the summary still holds the deleted words until it is rebuilt");
}
