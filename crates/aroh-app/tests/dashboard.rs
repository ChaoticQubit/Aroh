use aroh_app::{dashboard, local};
use aroh_domain::dashboard::Pace;
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{Action, ActionStatus, Autonomy, Goal, GoalStatus};
use aroh_store::{db, repo};
use chrono::{DateTime, Duration, TimeZone, Utc};
use rusqlite::Connection;
use std::collections::BTreeMap;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap()
}

fn put_goal(conn: &Connection, clock: &mut Hlc, user: &str, title: &str) -> String {
    let mut g = Goal {
        id: aroh_domain::new_id(),
        user_id: user.into(),
        category_id: None,
        title: title.into(),
        context: None,
        deadline: Some("2027-03-01".into()),
        status: GoalStatus::Active,
        autonomy: Autonomy::Ask,
        created_at: now() - Duration::days(200),
        reminders: false,
        use_calendars: false,
        run_limit_minutes: None,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut g).unwrap();
    g.id
}

fn put_action(conn: &Connection, clock: &mut Hlc, user: &str, goal: &str, days_ago: i64, status: ActionStatus) {
    let start = now() - Duration::days(days_ago);
    let mut a = Action {
        id: aroh_domain::new_id(),
        user_id: user.into(),
        goal_id: goal.into(),
        series_id: None,
        occurrence: None,
        title: "Session".into(),
        notes: None,
        start_at: start,
        end_at: start + Duration::minutes(40),
        status,
        locked: false,
        done_at: None,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut a).unwrap();
}

#[test]
fn read_counts_full_history_and_skips_tombstoned_goals() {
    let conn = db::open_in_memory().unwrap();
    let user = local::user_id(&conn).unwrap();
    let mut clock = Hlc::new("00000000000000d1");
    let kept = put_goal(&conn, &mut clock, &user, "Read");
    let gone = put_goal(&conn, &mut clock, &user, "Gone");
    put_action(&conn, &mut clock, &user, &kept, 120, ActionStatus::Done);
    put_action(&conn, &mut clock, &user, &kept, 2, ActionStatus::Done);
    put_action(&conn, &mut clock, &user, &gone, 2, ActionStatus::Done);
    repo::delete::<Goal>(&conn, &mut clock, &gone).unwrap();
    let d = dashboard::read(&conn, &user, &BTreeMap::new(), now(), chrono_tz::Europe::Berlin).unwrap();
    assert_eq!(d.goals.len(), 1);
    let g = &d.goals[0];
    assert_eq!((g.goal_id.as_str(), g.done, g.due, g.pace), (kept.as_str(), 2, 2, Pace::Ahead));
    assert_eq!(g.minutes.iter().sum::<i64>(), 40);
}
