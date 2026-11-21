use aroh_app::{local, remind, sample, snapshot, tick, write};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{Action, ActionSeries, ActionStatus, Category, EntrySource, Proposal, ProposalStatus, Tracker, TrackerEntry};
use aroh_store::{db, repo};
use chrono::{DateTime, Duration, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::Connection;
use serde_json::{json, Map};

const ZONE: Tz = chrono_tz::Europe::Berlin;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 26, 8, 15, 0).unwrap()
}

fn setup() -> (Connection, Hlc, String) {
    let conn = db::open_in_memory().unwrap();
    let user = local::user_id(&conn).unwrap();
    (conn, Hlc::new("00000000000000d1"), user)
}

fn seeded() -> (Connection, Hlc, String) {
    let (conn, mut clock, user) = setup();
    assert!(sample::seed(&conn, &mut clock, &user, now(), ZONE).unwrap());
    (conn, clock, user)
}

#[test]
fn local_user_id_is_created_once() {
    let (conn, _, user) = setup();
    assert_eq!(local::user_id(&conn).unwrap(), user);
    assert_eq!(user.len(), 36);
}

#[test]
fn settings_are_whitelisted_and_validated() {
    let (conn, _, _) = setup();
    local::set_setting(&conn, "server_url", "https://engine.example.com").unwrap();
    assert_eq!(local::setting(&conn, "server_url").unwrap().as_deref(), Some("https://engine.example.com"));
    local::set_setting(&conn, "server_url", "").unwrap();
    assert_eq!(local::setting(&conn, "server_url").unwrap(), None);
    assert!(local::set_setting(&conn, "server_url", "http://engine.example.com").is_err());
    assert!(local::set_setting(&conn, "user_id", "x").is_err());
    assert!(local::set_setting(&conn, "calendars", "maybe").is_err());
    assert!(local::set_setting(&conn, "goal_budget_usd", "-1").is_err());
    local::set_setting(&conn, "goal_budget_usd", "8").unwrap();
}

#[test]
fn tick_seeds_materializes_and_is_idempotent() {
    let (conn, mut clock, user) = seeded();
    let first = tick::tick(&conn, &mut clock, &user, now()).unwrap();
    assert_eq!(repo::list::<Category>(&conn, &[]).unwrap().len(), 6);
    assert!(first.materialized > 0);
    let series: Vec<ActionSeries> = repo::list(&conn, &[]).unwrap();
    let horizon = now() + Duration::days(i64::from(aroh_domain::recur::DEFAULT_WINDOW_DAYS));
    let daily = series.iter().find(|s| s.rrule == "FREQ=DAILY").unwrap();
    let ahead: Vec<Action> = repo::list::<Action>(&conn, &[("series_id", &daily.id)])
        .unwrap()
        .into_iter()
        .filter(|a| a.start_at >= now() && a.start_at < horizon)
        .collect();
    assert_eq!(ahead.len(), 14);
    let second = tick::tick(&conn, &mut clock, &user, now()).unwrap();
    assert_eq!((second.seeded, second.materialized, second.missed), (0, 0, 0));
}

#[test]
fn tick_marks_overdue_planned_actions_missed() {
    let (conn, mut clock, user) = seeded();
    tick::tick(&conn, &mut clock, &user, now()).unwrap();
    let later = now() + Duration::days(2);
    let report = tick::tick(&conn, &mut clock, &user, later).unwrap();
    assert!(report.missed > 0);
    let actions: Vec<Action> = repo::list(&conn, &[]).unwrap();
    let grace = Duration::minutes(tick::MISSED_GRACE_MIN);
    assert!(actions.iter().all(|a| !(a.status == ActionStatus::Planned && a.end_at + grace < later)));
    assert!(actions.iter().any(|a| a.status == ActionStatus::Done));
}

#[test]
fn snapshot_returns_live_rows_of_the_range() {
    let (conn, mut clock, user) = seeded();
    tick::tick(&conn, &mut clock, &user, now()).unwrap();
    let from = now() - Duration::days(1);
    let to = now() + Duration::days(1);
    let snap = snapshot::snapshot(&conn, &user, from, to, now() - Duration::days(30)).unwrap();
    assert!(!snap.actions.is_empty());
    assert!(snap.actions.iter().all(|a| a.start_at < to && a.end_at > from && !a.deleted));
    assert_eq!(snap.goals.len(), 3);
    assert_eq!(snap.user_id, user);
    let gone = snap.actions[0].id.clone();
    repo::delete::<Action>(&conn, &mut clock, &gone).unwrap();
    let again = snapshot::snapshot(&conn, &user, from, to, now() - Duration::days(30)).unwrap();
    assert!(again.actions.iter().all(|a| a.id != gone));
}

fn weight_tracker(conn: &Connection) -> Tracker {
    repo::list::<Tracker>(conn, &[]).unwrap().into_iter().find(|t| t.name == "Weight").unwrap()
}

fn entries(conn: &Connection) -> Vec<TrackerEntry> {
    repo::list(conn, &[]).unwrap()
}

fn open_proposal(conn: &Connection) -> Proposal {
    repo::list::<Proposal>(conn, &[]).unwrap().remove(0)
}

#[test]
fn check_in_writes_one_entry_and_refuses_unknown_fields() {
    let (conn, mut clock, user) = seeded();
    let tracker = weight_tracker(&conn);
    let before = entries(&conn).len();
    let mut data = Map::new();
    data.insert("weight".into(), json!(82.4));
    let entry = write::add_entry(&conn, &mut clock, &user, &tracker.id, data, now()).unwrap();
    assert_eq!(entry.source, EntrySource::CheckIn);
    assert_eq!(entry.data["weight"], json!(82.4));
    assert_eq!(entries(&conn).len(), before + 1);
    let mut bad = Map::new();
    bad.insert("height".into(), json!(180));
    assert!(write::add_entry(&conn, &mut clock, &user, &tracker.id, bad, now()).is_err());
    assert_eq!(entries(&conn).len(), before + 1);
}

#[test]
fn free_log_maps_a_unit_or_queues_the_text() {
    let (conn, mut clock, user) = seeded();
    let before = entries(&conn).len();
    match write::log_text(&conn, &mut clock, &user, "weighed 82.4 kg", now()).unwrap() {
        write::Logged::Entry(e) => {
            assert_eq!(e.source, EntrySource::Log);
            assert_eq!(e.data["weight"], json!(82.4));
            assert_eq!(e.raw_text.as_deref(), Some("weighed 82.4 kg"));
        }
        other => panic!("expected an entry, got {other:?}"),
    }
    assert!(matches!(write::log_text(&conn, &mut clock, &user, "40g protein shake", now()).unwrap(), write::Logged::Entry(_)));
    assert!(matches!(write::log_text(&conn, &mut clock, &user, "had pasta", now()).unwrap(), write::Logged::Queued));
    assert!(matches!(write::log_text(&conn, &mut clock, &user, "2 kg and 40 g", now()).unwrap(), write::Logged::Queued));
    assert_eq!(entries(&conn).len(), before + 2);
    assert_eq!(local::pending_logs(&conn).unwrap()[0].text, "had pasta");
    assert!(write::log_text(&conn, &mut clock, &user, "  ", now()).is_err());
}

#[test]
fn applying_a_proposal_moves_its_action_and_keeps_undo() {
    let (conn, mut clock, _) = seeded();
    let proposal = open_proposal(&conn);
    let change = proposal.diff["changes"][0].clone();
    let id = change["action_id"].as_str().unwrap().to_string();
    let applied = write::decide_proposal(&conn, &mut clock, &proposal.id, true).unwrap();
    assert_eq!(applied.status, ProposalStatus::Applied);
    let moved: Action = repo::get(&conn, &id).unwrap();
    assert_eq!(json!(moved.start_at), change["start_at"]);
    assert_eq!(moved.status, ActionStatus::Planned);
    assert_eq!(applied.undo.unwrap()["actions"][0]["before"]["status"], "missed");
    assert!(write::decide_proposal(&conn, &mut clock, &proposal.id, true).is_err());
}

#[test]
fn a_bad_diff_changes_nothing_and_reject_only_sets_status() {
    let (conn, mut clock, _) = seeded();
    let mut proposal = open_proposal(&conn);
    let first = proposal.diff["changes"][0].clone();
    let id = first["action_id"].as_str().unwrap().to_string();
    let before: Action = repo::get(&conn, &id).unwrap();
    proposal.diff = json!({ "changes": [first, { "op": "teleport", "title": "x" }] });
    repo::put(&conn, &mut clock, &mut proposal).unwrap();
    assert!(write::decide_proposal(&conn, &mut clock, &proposal.id, true).is_err());
    assert_eq!(repo::get::<Action>(&conn, &id).unwrap(), before);
    let rejected = write::decide_proposal(&conn, &mut clock, &proposal.id, false).unwrap();
    assert_eq!(rejected.status, ProposalStatus::Rejected);
    assert_eq!(repo::get::<Action>(&conn, &id).unwrap(), before);
}

#[test]
fn done_status_stamps_done_at() {
    let (conn, mut clock, user) = seeded();
    tick::tick(&conn, &mut clock, &user, now()).unwrap();
    let planned = repo::list::<Action>(&conn, &[("status", "planned")]).unwrap().remove(0);
    let done = write::set_action_status(&conn, &mut clock, &planned.id, ActionStatus::Done, now()).unwrap();
    assert_eq!(done.done_at, Some(now()));
    let undone = write::set_action_status(&conn, &mut clock, &planned.id, ActionStatus::Planned, now()).unwrap();
    assert_eq!(undone.done_at, None);
    assert!(write::set_action_status(&conn, &mut clock, &planned.id, ActionStatus::Missed, now()).is_err());
}

#[test]
fn reminders_cover_actions_and_check_ins() {
    let (conn, mut clock, user) = seeded();
    tick::tick(&conn, &mut clock, &user, now()).unwrap();
    let planned = remind::plan(&conn, &user, now(), ZONE).unwrap();
    assert!(!planned.is_empty() && planned.len() <= remind::CAP);
    assert!(planned.windows(2).all(|w| w[0].fire_at <= w[1].fire_at));
    assert!(planned.iter().all(|p| p.fire_at > now() && p.fire_at <= now() + Duration::hours(48)));
    let next = repo::list::<Action>(&conn, &[("status", "planned")])
        .unwrap()
        .into_iter()
        .filter(|a| a.start_at > now() + Duration::minutes(10))
        .min_by_key(|a| a.start_at)
        .unwrap();
    assert_eq!(planned.iter().filter(|p| p.action_id.as_deref() == Some(next.id.as_str())).count(), 3);
    let check_in = planned.iter().find(|p| p.action_id.is_none()).unwrap();
    assert_eq!(check_in.fire_at.with_timezone(&ZONE).format("%H:%M").to_string(), "07:00");
    assert!(check_in.fire_at <= now() + Duration::hours(24));
    assert!(remind::all_ids(&conn).unwrap().contains(&check_in.id));
}
