use aroh_app::{calendar, local, snapshot};
use aroh_domain::calendar::{rules_for, DeviceEvent, RuleSource};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{BusyBlock, RuleDecision};
use aroh_store::sync::pending;
use aroh_store::{db, repo};
use chrono::{DateTime, Duration, TimeZone, Utc};
use rusqlite::Connection;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 26, 8, 15, 0).unwrap()
}

fn setup() -> (Connection, Hlc, String) {
    let conn = db::open_in_memory().unwrap();
    let user = local::user_id(&conn).unwrap();
    (conn, Hlc::new("00000000000000d1"), user)
}

fn event(id: &str, title: &str, start: DateTime<Utc>) -> DeviceEvent {
    DeviceEvent {
        calendar_id: "work".into(),
        calendar: "Work".into(),
        event_id: id.into(),
        title: title.into(),
        start_at: start,
        end_at: start + Duration::minutes(30),
        all_day: false,
        rrule: None,
    }
}

fn source(id: &str, title: &str, recurring: bool) -> RuleSource {
    RuleSource { calendar_id: "work".into(), event_id: id.into(), title: title.into(), recurring }
}

fn store_rules(conn: &Connection, clock: &mut Hlc, user: &str, group: &str, decision: RuleDecision, sources: &[RuleSource]) {
    for mut rule in rules_for(user, group, decision, sources) {
        repo::put(conn, clock, &mut rule).unwrap();
    }
}

fn window() -> (DateTime<Utc>, DateTime<Utc>) {
    (now() - Duration::days(1), now() + Duration::days(14))
}

#[test]
fn import_keeps_matched_and_returns_unmatched_once_per_series() {
    let (conn, mut clock, user) = setup();
    store_rules(&conn, &mut clock, &user, "Work", RuleDecision::Keep, &[source("standup", "Standup", true)]);
    let day = |d: i64| now() + Duration::days(d);
    let events = vec![
        event("standup", "Standup", day(1)),
        event("standup", "Standup", day(2)),
        event("yoga", "Yoga", day(3)),
        event("yoga", "Yoga", day(1)),
        event("standup", "Standup", day(30)),
    ];
    let (from, to) = window();
    let report = calendar::import(&conn, &mut clock, &user, &events, from, to).unwrap();
    assert_eq!((report.kept, report.ignored, report.removed), (2, 0, 0));
    assert_eq!(report.unmatched.len(), 1);
    assert_eq!(report.unmatched[0].start_at, day(1));
    let blocks: Vec<BusyBlock> = repo::list(&conn, &[]).unwrap();
    assert_eq!(blocks.len(), 2);
    assert!(blocks.iter().all(|b| b.group_name.as_deref() == Some("Work")));
    let snap = snapshot::snapshot(&conn, &user, now(), day(2) - Duration::minutes(1), now()).unwrap();
    assert_eq!(snap.busy.len(), 1);
    assert!(snap.actions.is_empty());
}

#[test]
fn import_tombstones_moved_events() {
    let (conn, mut clock, user) = setup();
    store_rules(&conn, &mut clock, &user, "Work", RuleDecision::Keep, &[source("review", "Design review", false)]);
    let (from, to) = window();
    let first = now() + Duration::days(1);
    calendar::import(&conn, &mut clock, &user, &[event("review", "Design review", first)], from, to).unwrap();
    let later = first + Duration::hours(2);
    let moved = calendar::import(&conn, &mut clock, &user, &[event("review", "Design review", later)], from, to).unwrap();
    assert_eq!((moved.kept, moved.removed), (1, 1));
    let live: Vec<BusyBlock> = repo::list(&conn, &[]).unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].start_at, later);
}

#[test]
fn ignored_and_all_day_events_make_no_blocks() {
    let (conn, mut clock, user) = setup();
    store_rules(&conn, &mut clock, &user, "Noise", RuleDecision::Ignore, &[source("holiday", "Holiday", false)]);
    let mut trip = event("trip", "Trip", now() + Duration::days(2));
    trip.all_day = true;
    let (from, to) = window();
    let events = [event("h2", "Public holiday", now() + Duration::days(1)), trip];
    let report = calendar::import(&conn, &mut clock, &user, &events, from, to).unwrap();
    assert_eq!((report.kept, report.ignored, report.unmatched.len()), (0, 1, 0));
    assert!(repo::list::<BusyBlock>(&conn, &[]).unwrap().is_empty());
    let mut bad = event("x", "x", now());
    bad.end_at = bad.start_at;
    assert!(calendar::import(&conn, &mut clock, &user, &[bad], from, to).is_err());
}

#[test]
fn import_twice_writes_nothing_new() {
    let (conn, mut clock, user) = setup();
    store_rules(&conn, &mut clock, &user, "Work", RuleDecision::Keep, &[source("standup", "Standup", true)]);
    let (from, to) = window();
    let events = [event("standup", "Standup", now() + Duration::days(1))];
    calendar::import(&conn, &mut clock, &user, &events, from, to).unwrap();
    let table = aroh_domain::schema::table("busy_blocks").unwrap();
    let pushed = pending(&conn, table, &user, clock.node(), "", 500).unwrap();
    assert_eq!(pushed.len(), 1);
    let last = pushed[0]["updated_at"].as_str().unwrap().to_string();
    let again = calendar::import(&conn, &mut clock, &user, &events, from, to).unwrap();
    assert_eq!(again.kept, 1);
    assert!(pending(&conn, table, &user, clock.node(), &last, 500).unwrap().is_empty());
}
