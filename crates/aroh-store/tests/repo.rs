use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{samples, Action, ActionSeries, ActionStatus, BusyBlock, CalendarRule, Category, DayKind, Goal, Preference, PreferenceOrigin, Record, Tracker};
use aroh_domain::schema::SYNCED;
use aroh_store::{db, repo, Error};
use chrono::{TimeZone, Utc};

fn clock() -> Hlc {
    Hlc::with_wall_clock("00000000000000aa", 1_763_683_300_000)
}

#[test]
fn migrations_create_every_table_once_and_refuse_newer_files() {
    let conn = db::open_in_memory().unwrap();
    for table in SYNCED.iter().map(|t| t.name).chain(["sync_state"]) {
        let n: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [table], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "missing table {table}");
    }
    db::migrate(&conn).unwrap();
    conn.pragma_update(None, "user_version", db::SCHEMA_VERSION + 1).unwrap();
    assert!(matches!(db::migrate(&conn), Err(Error::SchemaTooNew(_))));
}

#[test]
fn rows_round_trip_with_json_bool_and_time_columns() {
    let conn = db::open_in_memory().unwrap();
    let mut clock = clock();
    let mut tracker = samples::tracker();
    repo::put(&conn, &mut clock, &mut tracker).unwrap();
    assert!(tracker.updated_at.as_str() > samples::STAMP);
    assert_eq!(repo::get::<Tracker>(&conn, &tracker.id).unwrap(), tracker);

    let mut action = samples::action();
    action.locked = true;
    repo::put(&conn, &mut clock, &mut action).unwrap();
    let before = action.updated_at.clone();
    action.status = ActionStatus::Done;
    action.done_at = Some(action.end_at);
    repo::put(&conn, &mut clock, &mut action).unwrap();
    assert!(action.updated_at > before);
    let listed: Vec<Action> = repo::list(&conn, &[("goal_id", &samples::goal_id()), ("status", "done")]).unwrap();
    assert_eq!(listed, vec![action.clone()]);
    let raw: (String, i64) = conn
        .query_row("SELECT start_at, locked FROM actions WHERE id = ?1", [&action.id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!(raw, ("2026-11-23T09:00:00Z".to_string(), 1));

    repo::delete::<Action>(&conn, &mut clock, &action.id).unwrap();
    assert!(matches!(repo::get::<Action>(&conn, &action.id), Err(Error::NotFound(_))));
    assert!(repo::list::<Action>(&conn, &[]).unwrap().is_empty());
    assert!(matches!(repo::delete::<Action>(&conn, &mut clock, &action.id), Err(Error::NotFound(_))));
    let (deleted, stamp): (i64, String) = conn
        .query_row("SELECT deleted, updated_at FROM actions WHERE id = ?1", [&action.id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!(deleted, 1);
    assert!(stamp > action.updated_at);
}

#[test]
fn invalid_rows_are_not_written_and_writes_beat_future_stamps() {
    let conn = db::open_in_memory().unwrap();
    let mut clock = clock();
    let mut goal = samples::goal();
    goal.title = String::new();
    assert!(matches!(repo::put(&conn, &mut clock, &mut goal), Err(Error::Domain(_))));
    assert!(repo::list::<Goal>(&conn, &[]).unwrap().is_empty());

    let mut goal = samples::goal();
    goal.updated_at = "9999999999999-0000-00000000000000bb".into();
    repo::insert_derived(&conn, &goal).unwrap();
    let mut edited = repo::get::<Goal>(&conn, &goal.id).unwrap();
    edited.title = "Edited".into();
    repo::put(&conn, &mut clock, &mut edited).unwrap();
    assert!(edited.updated_at > goal.updated_at);
}

#[test]
fn seeds_insert_once() {
    let conn = db::open_in_memory().unwrap();
    for _ in 0..2 {
        for category in aroh_domain::seed::categories(samples::USER) {
            repo::insert_derived(&conn, &category).unwrap();
        }
    }
    assert_eq!(repo::list::<Category>(&conn, &[]).unwrap().len(), 6);
}

#[test]
fn materialize_is_idempotent_and_never_resurrects() {
    let conn = db::open_in_memory().unwrap();
    let mut clock = clock();
    let mut series = samples::series();
    repo::put(&conn, &mut clock, &mut series).unwrap();
    let from = Utc.with_ymd_and_hms(2026, 11, 23, 0, 0, 0).unwrap();
    let first = repo::materialize(&conn, &series.id, from, 14).unwrap();
    assert_eq!(first.len(), 14);
    assert!(repo::materialize(&conn, &series.id, from, 14).unwrap().is_empty());
    repo::delete::<Action>(&conn, &mut clock, &first[0].id).unwrap();
    assert!(repo::materialize(&conn, &series.id, from, 14).unwrap().is_empty());
    assert_eq!(repo::list::<Action>(&conn, &[("series_id", &series.id)]).unwrap().len(), 13);
    assert_eq!(repo::materialize(&conn, &series.id, from, 15).unwrap().len(), 1);
}

#[test]
fn split_tombstones_planned_future_and_keeps_locked_and_done() {
    let conn = db::open_in_memory().unwrap();
    let mut clock = clock();
    let mut series = samples::series();
    repo::put(&conn, &mut clock, &mut series).unwrap();
    let from = Utc.with_ymd_and_hms(2026, 11, 23, 0, 0, 0).unwrap();
    let occ = repo::materialize(&conn, &series.id, from, 14).unwrap();
    let mut locked = occ[6].clone();
    locked.locked = true;
    repo::put(&conn, &mut clock, &mut locked).unwrap();
    let mut done = occ[7].clone();
    done.status = ActionStatus::Done;
    repo::put(&conn, &mut clock, &mut done).unwrap();

    let at = occ[5].start_at;
    let (old, new) = repo::split_series(&conn, &mut clock, &series.id, at, from, 14).unwrap();
    assert!(!old.deleted && old.rrule.contains("UNTIL="));
    assert_eq!(repo::get::<ActionSeries>(&conn, &new.id).unwrap(), new);
    let old_live: Vec<Action> = repo::list(&conn, &[("series_id", &series.id)]).unwrap();
    assert_eq!(old_live.len(), 5);
    assert!(old_live.iter().all(|a| a.start_at < at));
    let mut new_live: Vec<Action> = repo::list(&conn, &[("series_id", &new.id)]).unwrap();
    new_live.sort_by_key(|a| a.start_at);
    assert_eq!(new_live.len(), 9);
    assert_eq!(new_live[0].start_at, at);
    let kept: Vec<&Action> = new_live.iter().filter(|a| a.id == locked.id || a.id == done.id).collect();
    assert_eq!(kept.len(), 2);
    assert!(kept.iter().any(|a| a.locked) && kept.iter().any(|a| a.status == ActionStatus::Done));
    assert!(new_live.iter().all(|a| a.validate().is_ok()));
    let all: Vec<Action> = repo::list(&conn, &[("goal_id", &samples::goal_id())]).unwrap();
    let starts: std::collections::BTreeSet<_> = all.iter().map(|a| a.start_at).collect();
    assert_eq!(starts.len(), all.len(), "no instant is booked twice");
    assert!(repo::materialize(&conn, &new.id, from, 14).unwrap().is_empty());
}

#[test]
fn day_kinds_round_trip_with_and_without_a_kind() {
    let conn = db::open_in_memory().unwrap();
    let mut clock = clock();
    let samples = samples::all();
    let mut block: BusyBlock = serde_json::from_value(samples[12].clone()).unwrap();
    let mut rule: CalendarRule = serde_json::from_value(samples[13].clone()).unwrap();
    for kind in [Some(DayKind::Away), None] {
        block.day_kind = kind;
        rule.day_kind = kind;
        repo::put(&conn, &mut clock, &mut block).unwrap();
        repo::put(&conn, &mut clock, &mut rule).unwrap();
        assert_eq!(repo::get::<BusyBlock>(&conn, &block.id).unwrap(), block);
        assert_eq!(repo::get::<CalendarRule>(&conn, &rule.id).unwrap(), rule);
    }
    let raw: Option<String> = conn.query_row("SELECT day_kind FROM busy_blocks WHERE id = ?1", [&block.id], |r| r.get(0)).unwrap();
    assert_eq!(raw, None);
    block.day_kind = Some(DayKind::Reminder);
    repo::put(&conn, &mut clock, &mut block).unwrap();
    let raw: String = conn.query_row("SELECT day_kind FROM busy_blocks WHERE id = ?1", [&block.id], |r| r.get(0)).unwrap();
    assert_eq!(raw, "reminder");
}

#[test]
fn migrating_a_fourth_version_file_adds_goal_file_text_and_keeps_rows() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!("{}\nPRAGMA user_version = 4;", db::MIGRATIONS[..4].join("\n"))).unwrap();
    let file_id = "0190f2a0-0000-7000-8000-0000000000f1";
    let goal_id = "0190f2a0-0000-7000-8000-000000000091";
    conn.execute(
        "INSERT INTO goal_files VALUES (?1, ?2, ?3, 'plan.pdf', 'document', ?4, NULL, 'application/pdf', 1024, 'pending', ?5, 0)",
        [file_id, samples::USER, goal_id, &format!("goals/{goal_id}/plan.pdf"), samples::STAMP],
    )
    .unwrap();
    db::migrate(&conn).unwrap();
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
    assert_eq!(version, db::SCHEMA_VERSION);
    let file: aroh_domain::rows::GoalFile = repo::get(&conn, file_id).unwrap();
    assert_eq!((file.name.as_str(), file.size_bytes, file.extracted_text), ("plan.pdf", 1024, None));
}

#[test]
fn migrating_a_third_version_file_adds_goal_controls_and_profile_columns() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!("{}\n{}\n{}\nPRAGMA user_version = 3;", db::MIGRATIONS[0], db::MIGRATIONS[1], db::MIGRATIONS[2])).unwrap();
    let goal_id = "0190f2a0-0000-7000-8000-000000000091";
    let pref_id = "0190f2a0-0000-7000-8000-00000000000d";
    conn.execute(
        "INSERT INTO goals VALUES (?1, ?2, NULL, 'Read Dune', 'Evenings', '2027-01-31', 'active', 'ask', '2026-11-23T08:00:00Z', ?3, 0)",
        [goal_id, samples::USER, samples::STAMP],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO preferences VALUES (?1, ?2, 'Mornings', 'chat', '2026-11-23T08:00:00Z', 1, ?3, 0)",
        [pref_id, samples::USER, samples::STAMP],
    )
    .unwrap();
    db::migrate(&conn).unwrap();
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
    assert_eq!(version, db::SCHEMA_VERSION);
    let goal: Goal = repo::get(&conn, goal_id).unwrap();
    assert_eq!((goal.title.as_str(), goal.reminders, goal.use_calendars, goal.run_limit_minutes), ("Read Dune", true, true, None));
    let pref: Preference = repo::get(&conn, pref_id).unwrap();
    assert_eq!(
        (pref.fact.as_str(), pref.topic, pref.goal_id, pref.origin, pref.blocked),
        ("Mornings", None, None, PreferenceOrigin::Edit, false)
    );
}

#[test]
fn migrating_a_first_version_file_adds_day_kinds_and_keeps_rows() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!("{}\nPRAGMA user_version = 1;", db::MIGRATIONS[0])).unwrap();
    conn.execute(
        "INSERT INTO busy_blocks VALUES ('0190f2a0-0000-7000-8000-00000000000e', 'u', 'work', 'evt-1', 'Standup', '2026-11-23T09:00:00Z', '2026-11-23T10:00:00Z', 0, 'Work', ?1, 0)",
        [samples::STAMP],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO calendar_rules VALUES ('0190f2a0-0000-7000-8000-00000000000f', 'u', 'work', NULL, NULL, 'Work', 'keep', ?1, 0)",
        [samples::STAMP],
    )
    .unwrap();
    db::migrate(&conn).unwrap();
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
    assert_eq!(version, db::SCHEMA_VERSION);
    let block: BusyBlock = repo::get(&conn, "0190f2a0-0000-7000-8000-00000000000e").unwrap();
    assert_eq!((block.title.as_str(), block.group_name.as_deref(), block.day_kind), ("Standup", Some("Work"), None));
    let rule: CalendarRule = repo::get(&conn, "0190f2a0-0000-7000-8000-00000000000f").unwrap();
    assert_eq!((rule.group_name.as_str(), rule.day_kind), ("Work", None));
    let indexed: i64 = conn.query_row("SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = 'busy_blocks_start'", [], |r| r.get(0)).unwrap();
    assert_eq!(indexed, 1);
}
