use aroh_app::{tick, write};
use aroh_domain::card::log_id;
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{samples, Action, ActionLog, ActionSeries, ActionStatus};
use aroh_domain::schema::{Kind, SYNCED};
use aroh_store::sync::apply_page;
use aroh_store::{db, repo};
use chrono::{DateTime, Duration, TimeZone, Utc};
use rusqlite::Connection;

const ZONE: chrono_tz::Tz = chrono_tz::Europe::Berlin;
const USER: &str = samples::USER;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 23, 5, 0, 0).unwrap()
}

fn store() -> (Connection, Hlc, ActionSeries) {
    let mut conn = db::open_in_memory().unwrap();
    let mut clock = Hlc::new("00000000000000d1");
    for (table, row) in SYNCED.iter().zip(samples::all()) {
        let serde_json::Value::Object(mut row) = row else { unreachable!() };
        for column in table.columns.iter().filter(|c| c.kind == Kind::Time) {
            if let Some(text) = row[column.name].as_str() {
                row.insert(column.name.into(), text.replace('Z', "+00:00").into());
            }
        }
        row.insert("server_seq".into(), 1.into());
        apply_page(&mut conn, table, USER, &[row], &mut clock).unwrap();
    }
    tick::tick(&conn, &mut clock, USER, now(), ZONE).unwrap();
    (conn, clock, samples::series())
}

fn occurrences(conn: &Connection, series: &str) -> Vec<Action> {
    let mut all: Vec<Action> = repo::list(conn, &[("series_id", series)]).unwrap();
    all.sort_by_key(|a| a.occurrence);
    all
}

fn live_after(conn: &Connection, series: &str, cut: DateTime<Utc>) -> Vec<(DateTime<Utc>, ActionStatus)> {
    occurrences(conn, series).into_iter().filter(|a| a.occurrence.unwrap() >= cut).map(|a| (a.occurrence.unwrap(), a.status)).collect()
}

fn set_status(conn: &Connection, clock: &mut Hlc, mut a: Action, status: ActionStatus) {
    a.status = status;
    if status == ActionStatus::Done {
        a.done_at = Some(a.end_at);
    }
    repo::put(conn, clock, &mut a).unwrap();
}

#[test]
fn b100_all_future_bounds_the_series_and_keeps_history_and_logged_sessions() {
    let (conn, mut clock, series) = store();
    let o = occurrences(&conn, &series.id);
    assert!(o.len() > 8, "a window of daily occurrences");
    set_status(&conn, &mut clock, o[0].clone(), ActionStatus::Done);
    set_status(&conn, &mut clock, o[3].clone(), ActionStatus::Skipped);
    set_status(&conn, &mut clock, o[4].clone(), ActionStatus::Done);
    let mut log = ActionLog { id: log_id(&o[5].id), user_id: USER.into(), action_id: o[5].id.clone(), goal_id: o[5].goal_id.clone(), sets: vec![], checked: vec![], effort: None, note: Some("half done".into()), logged_at: o[5].end_at, updated_at: String::new(), deleted: false };
    repo::put(&conn, &mut clock, &mut log).unwrap();
    let moved = write::edit_action(&conn, &mut clock, &o[6].id, Some((o[6].start_at + Duration::hours(3), o[6].end_at + Duration::hours(3)))).unwrap();
    assert_ne!(moved.start_at, moved.occurrence.unwrap());
    let cut = o[2].occurrence.unwrap();

    write::delete_series_from(&conn, &mut clock, &o[2].id).unwrap();

    let bounded: ActionSeries = repo::get(&conn, &series.id).unwrap();
    assert!(bounded.rrule.contains(&format!("UNTIL={}", (cut - Duration::seconds(1)).format("%Y%m%dT%H%M%SZ"))), "{}", bounded.rrule);
    assert_eq!(occurrences(&conn, &series.id)[..2].iter().map(|a| a.id.clone()).collect::<Vec<_>>(), vec![o[0].id.clone(), o[1].id.clone()], "earlier sessions stay");
    let kept = live_after(&conn, &series.id, cut);
    assert_eq!(kept, vec![(o[3].occurrence.unwrap(), ActionStatus::Skipped), (o[4].occurrence.unwrap(), ActionStatus::Done), (o[5].occurrence.unwrap(), ActionStatus::Planned)], "skipped, done and logged sessions stay; the moved one goes");
    for day in [0, 10, 40] {
        tick::tick(&conn, &mut clock, USER, now() + Duration::days(day), ZONE).unwrap();
    }
    let slots = |v: Vec<(DateTime<Utc>, ActionStatus)>| v.into_iter().map(|(at, _)| at).collect::<Vec<_>>();
    assert_eq!(slots(live_after(&conn, &series.id, cut)), slots(kept), "no tick materializes after the cut and the logged session stays");
}

#[test]
fn b100_all_future_from_a_moved_session_cuts_at_its_series_slot() {
    let (conn, mut clock, series) = store();
    let o = occurrences(&conn, &series.id);
    let moved = write::edit_action(&conn, &mut clock, &o[3].id, Some((o[3].start_at - Duration::hours(30), o[3].end_at - Duration::hours(30)))).unwrap();
    write::delete_series_from(&conn, &mut clock, &moved.id).unwrap();
    let slot = o[3].occurrence.unwrap();
    let bounded: ActionSeries = repo::get(&conn, &series.id).unwrap();
    assert!(bounded.rrule.contains(&format!("UNTIL={}", (slot - Duration::seconds(1)).format("%Y%m%dT%H%M%SZ"))), "{}", bounded.rrule);
    assert_eq!(occurrences(&conn, &series.id).len(), 3, "the three sessions before the slot stay, the moved one goes");
}

#[test]
fn b100_all_future_from_the_first_session_tombstones_the_series() {
    let (conn, mut clock, series) = store();
    let start = aroh_domain::recur::materialize(&series, Utc.with_ymd_and_hms(2026, 10, 20, 0, 0, 0).unwrap(), 1, &Default::default()).unwrap().remove(0);
    repo::materialize(&conn, &series.id, Utc.with_ymd_and_hms(2026, 10, 20, 0, 0, 0).unwrap(), 1).unwrap();
    write::delete_series_from(&conn, &mut clock, &start.id).unwrap();
    assert!(repo::get::<ActionSeries>(&conn, &series.id).is_err(), "the series is a tombstone");
    tick::tick(&conn, &mut clock, USER, now() + Duration::days(3), ZONE).unwrap();
    assert!(occurrences(&conn, &series.id).is_empty(), "no session of the series is left or made again");
}

#[test]
fn b100_a_one_off_action_has_no_series_to_end() {
    let (conn, mut clock, _) = store();
    let err = write::delete_series_from(&conn, &mut clock, &samples::action().id).unwrap_err();
    assert!(err.to_string().contains("series"), "{err}");
}
