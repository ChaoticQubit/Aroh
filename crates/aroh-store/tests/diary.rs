use aroh_diary::{day_id, seal, Card, DiaryKey, Entry, Source, Table};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{samples, DiaryDay, DiaryEntry, DIARY_DAYS, DIARY_ENTRIES};
use aroh_store::sync::pending;
use aroh_store::{db, repo};

const SENTINEL: &str = "PLAINTEXT-SENTINEL-7f3a";

/// The wire rows a device pushes for both diary tables carry only the day,
/// times, key version and base64 nonce and ciphertext, never the words.
#[test]
fn pending_diary_rows_carry_no_plaintext() {
    let conn = db::open_in_memory().unwrap();
    let mut clock = Hlc::with_wall_clock("00000000000000aa", 1_763_683_200_000);
    let key = DiaryKey::generate(1).unwrap();
    let user = samples::USER;
    let entry_id = uuid::Uuid::now_v7().to_string();
    let entry = Entry { at: "2026-11-26T07:45:00Z".parse().unwrap(), text: format!("I felt {SENTINEL} today"), source: Source::Text };
    let sealed = seal(&key, user, Table::Entries, &entry_id, &entry.to_bytes()).unwrap();
    let mut row = DiaryEntry { id: entry_id.clone(), user_id: user.into(), day: "2026-11-26".into(), at: entry.at, key_version: 1, nonce: sealed.nonce, ciphertext: sealed.ciphertext, updated_at: String::new(), deleted: false };
    repo::put(&conn, &mut clock, &mut row).unwrap();
    let card = Card {
        v: 1,
        day: "2026-11-26".into(),
        entry_ids: vec![entry_id],
        summary: format!("Summary {SENTINEL}"),
        mood: 3,
        energy: 3,
        highlights: vec![SENTINEL.into()],
        food: Some(SENTINEL.into()),
        water: None,
        workout: None,
        notes_for_tomorrow: Some(SENTINEL.into()),
        structured_at: "2026-11-26T08:00:00Z".parse().unwrap(),
        edited_at: None,
    };
    let id = day_id(user, "2026-11-26");
    let sealed = seal(&key, user, Table::Days, &id, &card.to_bytes()).unwrap();
    let mut day = DiaryDay { id, user_id: user.into(), day: "2026-11-26".into(), key_version: 1, nonce: sealed.nonce, ciphertext: sealed.ciphertext, updated_at: String::new(), deleted: false };
    repo::put(&conn, &mut clock, &mut day).unwrap();

    for table in [&DIARY_ENTRIES, &DIARY_DAYS] {
        let rows = pending(&conn, table, user, clock.node(), "", 500).unwrap();
        assert_eq!(rows.len(), 1, "{}", table.name);
        let wire = serde_json::to_string(&rows).unwrap();
        assert!(!wire.contains(SENTINEL), "{} leaked plaintext", table.name);
        assert!(!wire.to_lowercase().contains("felt"), "{}", table.name);
    }
}
