use aroh_core::hlc::Hlc;
use aroh_core::models::{Action, ActionStatus};
use aroh_core::{db, repo};

fn sample_action(clock: &mut Hlc) -> Action {
    Action {
        id: aroh_core::new_id(),
        goal_id: aroh_core::new_id(),
        title: "Watch lesson 1".into(),
        notes: None,
        start_at: "2026-11-21T09:00:00Z".into(),
        end_at: "2026-11-21T09:30:00Z".into(),
        est_minutes: 30,
        status: ActionStatus::Planned,
        seq: 1,
        done_at: None,
        updated_at: clock.tick(),
        deleted: false,
    }
}

#[test]
fn migrations_apply_on_fresh_db() {
    let conn = db::open_in_memory().unwrap();
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
    assert_eq!(version, db::SCHEMA_VERSION);
    for table in ["goals", "actions", "plans", "replans", "tracker_entries", "plugins", "settings"] {
        let n: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1", [table], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "missing table {table}");
    }
    db::migrate(&conn).unwrap();
}

#[test]
fn action_crud_roundtrip() {
    let conn = db::open_in_memory().unwrap();
    let mut clock = Hlc::new("00000000000000aa");
    let mut action = sample_action(&mut clock);
    repo::insert_action(&conn, &mut clock, &mut action).unwrap();
    assert_eq!(repo::list_actions(&conn, None).unwrap(), vec![action.clone()]);

    let before = action.updated_at.clone();
    action.title = "Watch lesson 1 and 2".into();
    action.status = ActionStatus::Done;
    repo::update_action(&conn, &mut clock, &mut action).unwrap();
    assert!(action.updated_at > before);
    let listed = repo::list_actions(&conn, Some(action.goal_id.as_str())).unwrap();
    assert_eq!(listed, vec![action.clone()]);

    repo::soft_delete_action(&conn, &mut clock, &action.id).unwrap();
    assert!(repo::list_actions(&conn, None).unwrap().is_empty());
    let (deleted, stamp): (bool, String) = conn
        .query_row("SELECT deleted, updated_at FROM actions WHERE id=?1", [&action.id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert!(deleted);
    assert!(stamp > action.updated_at);
}
