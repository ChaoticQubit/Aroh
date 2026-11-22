use aroh_app::travel::{follow, Follow};
use aroh_app::{local, sample, snapshot, tick};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{Action, ActionSeries};
use aroh_store::{db, repo};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use chrono_tz::{America::New_York, Europe::Berlin};
use rusqlite::Connection;
use std::collections::BTreeMap;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 26, 8, 15, 0).unwrap()
}

fn seeded() -> (Connection, Hlc, String) {
    let conn = db::open_in_memory().unwrap();
    let user = local::user_id(&conn).unwrap();
    let mut clock = Hlc::new("00000000000000d2");
    assert!(sample::seed(&conn, &mut clock, &user, now(), Berlin).unwrap());
    tick::tick(&conn, &mut clock, &user, now()).unwrap();
    (conn, clock, user)
}

fn actions(conn: &Connection, user: &str) -> Vec<Action> {
    let mut list = repo::list::<Action>(conn, &[("user_id", user)]).unwrap();
    list.sort_by(|a, b| a.id.cmp(&b.id));
    list
}

fn future_series_rows_per_local_day(conn: &Connection, user: &str, zone: Tz) -> BTreeMap<(String, NaiveDate), usize> {
    let mut days = BTreeMap::new();
    for a in actions(conn, user).into_iter().filter(|a| a.series_id.is_some() && a.start_at >= now()) {
        *days.entry((a.series_id.clone().unwrap(), a.occurrence.unwrap().with_timezone(&zone).date_naive())).or_insert(0) += 1;
    }
    days
}

#[test]
fn first_call_records_the_zone_and_changes_nothing() {
    let (conn, mut clock, user) = seeded();
    let before = actions(&conn, &user);
    assert_eq!(follow(&conn, &mut clock, &user, now(), Some(Berlin)).unwrap(), Follow::Recorded);
    assert_eq!(local::zone(&conn).unwrap().as_deref(), Some("Europe/Berlin"));
    assert!(!local::zone_pending(&conn).unwrap());
    assert!(local::notes(&conn).unwrap().is_empty());
    assert_eq!(actions(&conn, &user), before);
    assert_eq!(follow(&conn, &mut clock, &user, now(), Some(Berlin)).unwrap(), Follow::Same);
    assert_eq!(actions(&conn, &user), before);
}

#[test]
fn unknown_zone_does_nothing() {
    let (conn, mut clock, user) = seeded();
    let before = actions(&conn, &user);
    assert_eq!(follow(&conn, &mut clock, &user, now(), None).unwrap(), Follow::Unknown);
    assert_eq!(local::zone(&conn).unwrap(), None);
    follow(&conn, &mut clock, &user, now(), Some(Berlin)).unwrap();
    assert_eq!(follow(&conn, &mut clock, &user, now(), None).unwrap(), Follow::Unknown);
    assert_eq!(local::zone(&conn).unwrap().as_deref(), Some("Europe/Berlin"));
    assert_eq!(actions(&conn, &user), before);
}

#[test]
fn a_zone_change_shifts_rows_series_and_writes_one_note() {
    let (conn, mut clock, user) = seeded();
    follow(&conn, &mut clock, &user, now(), Some(Berlin)).unwrap();
    let before = future_series_rows_per_local_day(&conn, &user, Berlin);
    let Follow::Shifted { moved, .. } = follow(&conn, &mut clock, &user, now(), Some(New_York)).unwrap() else { panic!("expected a shift") };
    assert!(moved > 0);
    assert!(repo::list::<ActionSeries>(&conn, &[("user_id", &user)]).unwrap().iter().all(|s| s.timezone == "America/New_York"));
    assert_eq!(local::zone(&conn).unwrap().as_deref(), Some("America/New_York"));
    assert!(local::zone_pending(&conn).unwrap());
    let notes = local::notes(&conn).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title, "Time zone changed");
    assert!(notes[0].body.contains("America/New_York") && notes[0].body.contains("Europe/Berlin"), "{}", notes[0].body);
    assert_eq!(follow(&conn, &mut clock, &user, now(), Some(New_York)).unwrap(), Follow::Same);
    assert_eq!(local::notes(&conn).unwrap().len(), 1);
    tick::tick(&conn, &mut clock, &user, now()).unwrap();
    let after = future_series_rows_per_local_day(&conn, &user, New_York);
    assert!(after.values().all(|n| *n == 1), "{after:?}");
    assert!(after.len() >= before.len() - 2);
    local::clear_zone_pending(&conn).unwrap();
    assert!(!local::zone_pending(&conn).unwrap());
}

#[test]
fn flying_back_restores_the_rows_without_twins() {
    let (conn, mut clock, user) = seeded();
    follow(&conn, &mut clock, &user, now(), Some(Berlin)).unwrap();
    let home: Vec<(String, DateTime<Utc>)> = actions(&conn, &user).into_iter().filter(|a| a.start_at >= now()).map(|a| (a.id, a.start_at)).collect();
    follow(&conn, &mut clock, &user, now(), Some(New_York)).unwrap();
    tick::tick(&conn, &mut clock, &user, now()).unwrap();
    follow(&conn, &mut clock, &user, now(), Some(Berlin)).unwrap();
    tick::tick(&conn, &mut clock, &user, now()).unwrap();
    let back = future_series_rows_per_local_day(&conn, &user, Berlin);
    assert!(back.values().all(|n| *n == 1), "{back:?}");
    let live: Vec<(String, DateTime<Utc>)> = actions(&conn, &user).into_iter().filter(|a| a.start_at >= now()).map(|a| (a.id, a.start_at)).collect();
    for row in &home {
        assert!(live.contains(row), "{row:?} not restored");
    }
    assert_eq!(local::notes(&conn).unwrap().len(), 2);
}

#[test]
fn snapshot_carries_notes_and_zone() {
    let (conn, mut clock, user) = seeded();
    follow(&conn, &mut clock, &user, now(), Some(Berlin)).unwrap();
    follow(&conn, &mut clock, &user, now(), Some(New_York)).unwrap();
    let snap = snapshot::snapshot(&conn, &user, now(), now() + Duration::days(1), now() - Duration::days(1)).unwrap();
    assert_eq!(snap.zone.as_deref(), Some("America/New_York"));
    assert_eq!(snap.notes.len(), 1);
    let json = serde_json::to_value(&snap).unwrap();
    assert_eq!(json["zone"], "America/New_York");
    assert_eq!(json["notes"][0]["title"], "Time zone changed");
}

#[test]
fn notes_keep_the_newest_thirty() {
    let conn = db::open_in_memory().unwrap();
    for n in 0..35 {
        local::add_note(&conn, "Note", &format!("body {n}"), now() + Duration::minutes(n)).unwrap();
    }
    let notes = local::notes(&conn).unwrap();
    assert_eq!(notes.len(), local::MAX_NOTES);
    assert_eq!(notes[0].body, "body 34");
    assert_eq!(notes[29].body, "body 5");
    for note in &notes {
        aroh_domain::rows::check_uuid("note", &note.id).unwrap();
    }
}
