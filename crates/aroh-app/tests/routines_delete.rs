use aroh_app::{location, remove, tick, write};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{samples, Action, ActionLog, ActionSeries, ActionStatus, Goal};
use aroh_domain::schema::{Kind, SYNCED};
use aroh_domain::seed::routines_id;
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
    remove::delete_goal(&conn, &mut clock, &samples::goal_id(), true).unwrap();
    let series: ActionSeries = repo::get(&conn, &samples::series().id).unwrap();
    assert_eq!(series.goal_id, routines_id(), "keep actions moved the series to Routines");
    (conn, clock, series)
}

fn sessions(conn: &Connection, series: &str) -> Vec<Action> {
    let mut all: Vec<Action> = repo::list(conn, &[("series_id", series)]).unwrap();
    all.sort_by_key(|a| a.occurrence);
    all
}

fn done_one_before_now(conn: &Connection, clock: &mut Hlc, series: &str) -> Action {
    repo::materialize(conn, series, now() - Duration::days(2), 1).unwrap();
    let mut a = sessions(conn, series).into_iter().rev().find(|a| a.end_at < now()).expect("a past session");
    a.status = ActionStatus::Done;
    a.done_at = Some(a.end_at);
    repo::put(conn, clock, &mut a).unwrap();
    a
}

fn open_from_now(conn: &Connection, goal: &str) -> Vec<Action> {
    repo::list::<Action>(conn, &[("goal_id", goal)]).unwrap().into_iter().filter(|a| a.end_at > now() && matches!(a.status, ActionStatus::Planned | ActionStatus::Missed)).collect()
}

fn logged(conn: &Connection, a: &Action) -> bool {
    !repo::list::<ActionLog>(conn, &[("action_id", a.id.as_str())]).unwrap().is_empty()
}

fn routines_shown(conn: &Connection, at: DateTime<Utc>) -> bool {
    let snap = aroh_app::snapshot::snapshot(conn, USER, at, at + Duration::days(1), at, at).unwrap();
    snap.goals.iter().any(|g| g.id == routines_id())
}

#[test]
fn b110_a_routine_whose_sessions_were_all_deleted_is_listed_and_ends_from_now() {
    let (conn, mut clock, series) = store();
    let done = done_one_before_now(&conn, &mut clock, &series.id);
    for a in sessions(&conn, &series.id).into_iter().filter(|a| a.end_at > now()) {
        write::edit_action(&conn, &mut clock, &a.id, None).unwrap();
    }
    assert!(open_from_now(&conn, &routines_id()).iter().all(|a| a.series_id.as_deref() != Some(series.id.as_str())));
    assert!(location::listed(&conn, &series, now()).unwrap(), "rule dates past the window still come");

    let ended = write::end_series(&conn, &mut clock, &series.id, now()).unwrap();

    assert!(ended.rrule.contains("UNTIL="), "{}", ended.rrule);
    assert!(!location::listed(&conn, &ended, now()).unwrap(), "an ended routine is not listed");
    for day in [0, 10, 40] {
        tick::tick(&conn, &mut clock, USER, now() + Duration::days(day), ZONE).unwrap();
    }
    assert!(sessions(&conn, &series.id).iter().all(|a| a.end_at <= now()), "no session comes back after now");
    let kept: Action = repo::get(&conn, &done.id).unwrap();
    assert_eq!(kept.status, ActionStatus::Done, "done history stays");
}

#[test]
fn b110_ending_a_routine_tombstones_its_open_sessions_from_now() {
    let (conn, mut clock, series) = store();
    assert!(sessions(&conn, &series.id).iter().any(|a| a.start_at >= now()), "the window holds future sessions");
    write::end_series(&conn, &mut clock, &series.id, now()).unwrap();
    assert!(sessions(&conn, &series.id).iter().all(|a| a.end_at <= now()), "open sessions from now are tombstones");
}

#[test]
fn b110_a_rule_date_whose_slot_was_deleted_does_not_list_the_routine() {
    let (conn, mut clock, mut series) = store();
    series.rrule = format!("FREQ=DAILY;UNTIL={}", (now() + Duration::days(3)).format("%Y%m%dT%H%M%SZ"));
    repo::put(&conn, &mut clock, &mut series).unwrap();
    tick::tick(&conn, &mut clock, USER, now(), ZONE).unwrap();
    let future: Vec<Action> = sessions(&conn, &series.id).into_iter().filter(|a| a.end_at > now()).collect();
    assert!(!future.is_empty());
    assert!(location::listed(&conn, &series, now()).unwrap(), "a live future session lists it");
    for a in &future {
        write::edit_action(&conn, &mut clock, &a.id, None).unwrap();
    }
    assert!(!location::listed(&conn, &series, now()).unwrap(), "every rule date left is a deleted slot");
    let view = location::view(&conn, USER, now(), ZONE).unwrap();
    assert!(view.routines.iter().all(|r| r.series_id != series.id), "the Routines card leaves it out");
}

#[test]
fn b110_delete_routines_ends_every_routine_keeps_history_and_stays_deleted_until_a_new_routine() {
    let (conn, mut clock, series) = store();
    let done = done_one_before_now(&conn, &mut clock, &series.id);
    assert!(routines_shown(&conn, now()));

    remove::delete_routines(&conn, &mut clock, USER, now()).unwrap();

    assert!(repo::get::<Goal>(&conn, &routines_id()).is_err(), "Routines is a tombstone");
    let left = open_from_now(&conn, &routines_id());
    assert!(left.iter().all(|a| a.locked && logged(&conn, a)), "only logged open rows stay from now, locked: {left:?}");
    assert!(left.iter().all(|a| a.series_id.is_none()), "the sample logged one-off is the only one");
    for day in [0, 1, 10] {
        tick::tick(&conn, &mut clock, USER, now() + Duration::days(day), ZONE).unwrap();
        assert!(repo::get::<Goal>(&conn, &routines_id()).is_err(), "no tick seeds Routines again");
    }
    let ids = |v: &[Action]| v.iter().map(|a| a.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&open_from_now(&conn, &routines_id())), ids(&left), "no tick materializes a session after the cut");
    assert_eq!(repo::get::<Action>(&conn, &done.id).unwrap().status, ActionStatus::Done, "done history stays");
    assert!(!routines_shown(&conn, now()));

    location::ensure_routines(&conn, &mut clock, USER).unwrap();
    assert!(repo::get::<Goal>(&conn, &routines_id()).is_ok(), "a new routine brings Routines back");
}

#[test]
fn b110_routines_shows_only_while_it_lists_a_routine_or_holds_an_unlogged_open_future_action() {
    let (conn, mut clock, series) = store();
    assert!(routines_shown(&conn, now()), "a routine with rule dates ahead lists Routines");
    write::end_series(&conn, &mut clock, &series.id, now()).unwrap();
    let open = open_from_now(&conn, &routines_id());
    assert!(!open.is_empty() && open.iter().all(|a| a.series_id.is_none() && logged(&conn, a)), "only the logged sample one-off is open");
    assert!(!routines_shown(&conn, now()), "a logged one-off is history: Routines is hidden");
    let mut one_off = Action { id: aroh_domain::new_id(), title: "Buy chalk".into(), ..open[0].clone() };
    repo::put(&conn, &mut clock, &mut one_off).unwrap();
    assert!(routines_shown(&conn, now()), "an unlogged open one-off from now keeps Routines listed");
}

#[test]
fn b110_a_deleted_routine_is_not_listed_while_a_session_runs_or_a_logged_one_is_kept() {
    let (conn, mut clock, series) = store();
    let future = sessions(&conn, &series.id).into_iter().filter(|a| a.start_at > now()).collect::<Vec<_>>();
    let during = future[0].start_at + Duration::minutes(10);
    let mut log = ActionLog { id: aroh_domain::card::log_id(&future[2].id), user_id: USER.into(), action_id: future[2].id.clone(), goal_id: future[2].goal_id.clone(), sets: vec![], checked: vec![], effort: None, note: Some("planned sets".into()), logged_at: during, updated_at: String::new(), deleted: false };
    repo::put(&conn, &mut clock, &mut log).unwrap();

    let ended = write::end_series(&conn, &mut clock, &series.id, during).unwrap();

    assert!(repo::get::<Action>(&conn, &future[2].id).unwrap().locked, "the logged session stays, locked");
    assert!(repo::get::<Action>(&conn, &future[0].id).is_ok(), "the running session stays");
    assert!(!location::listed(&conn, &ended, during).unwrap(), "a second Delete is never needed");
    assert!(location::view(&conn, USER, during, ZONE).unwrap().routines.is_empty());
}
