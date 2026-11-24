use aroh_app::undo;
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{samples, Action, Proposal, ProposalSize, ProposalStatus};
use aroh_store::{db, repo, Error};
use chrono::{Duration, TimeZone, Utc};
use serde_json::json;

const RUN: &str = "0195a1b2-0000-7000-8000-000000000077";

fn proposal(id: &str, status: ProposalStatus, undo: serde_json::Value) -> Proposal {
    Proposal {
        id: id.into(),
        user_id: samples::USER.into(),
        goal_id: samples::goal_id(),
        run_id: Some(RUN.into()),
        summary: "Moved Buy a scale to the afternoon.".into(),
        diff: json!({"changes": []}),
        size: ProposalSize::Small,
        status,
        undo: Some(undo),
        created_at: Utc.with_ymd_and_hms(2026, 11, 23, 6, 0, 0).unwrap(),
        updated_at: String::new(),
        deleted: false,
    }
}

#[test]
fn undo_run_restores_moves_removes_creates_and_marks_undone() {
    let conn = db::open_in_memory().unwrap();
    let mut clock = Hlc::new("00000000000000d1");
    let mut moved = samples::action();
    moved.updated_at = String::new();
    let before = (moved.start_at, moved.end_at);
    moved.start_at += Duration::hours(5);
    moved.end_at += Duration::hours(5);
    repo::put(&conn, &mut clock, &mut moved).unwrap();
    let mut created = samples::action();
    created.id = "0195a1b2-0000-7000-8000-0000000000c1".into();
    created.updated_at = String::new();
    repo::put(&conn, &mut clock, &mut created).unwrap();
    let mut p = proposal(
        "0195a1b2-0000-7000-8000-0000000000a1",
        ProposalStatus::Applied,
        json!({"changes": [
            {"op": "restore", "action_id": moved.id, "before": {"start_at": before.0, "end_at": before.1}, "deleted": false},
            {"op": "delete", "action_id": created.id}
        ]}),
    );
    repo::put(&conn, &mut clock, &mut p).unwrap();

    assert_eq!(undo::run_proposals(&conn, RUN).unwrap().len(), 1);
    assert_eq!(undo::undo_run(&conn, &mut clock, RUN).unwrap(), 2);
    let back: Action = repo::get(&conn, &moved.id).unwrap();
    assert_eq!((back.start_at, back.end_at), before);
    assert!(matches!(repo::get::<Action>(&conn, &created.id), Err(Error::NotFound(_))));
    assert_eq!(repo::get::<Proposal>(&conn, &p.id).unwrap().status, ProposalStatus::Undone);
    assert!(undo::undo_run(&conn, &mut clock, RUN).is_err(), "a second undo has nothing left");
}

#[test]
fn undo_run_reads_the_device_shape_and_skips_open_proposals() {
    let conn = db::open_in_memory().unwrap();
    let mut clock = Hlc::new("00000000000000d1");
    let mut action = samples::action();
    action.updated_at = String::new();
    let original = action.clone();
    repo::put(&conn, &mut clock, &mut action).unwrap();
    repo::delete::<Action>(&conn, &mut clock, &action.id).unwrap();
    let mut open = proposal("0195a1b2-0000-7000-8000-0000000000a2", ProposalStatus::Proposed, json!({}));
    repo::put(&conn, &mut clock, &mut open).unwrap();
    assert!(undo::undo_run(&conn, &mut clock, RUN).is_err(), "an open proposal is not undone");
    let mut applied = proposal("0195a1b2-0000-7000-8000-0000000000a3", ProposalStatus::Applied, json!({"actions": [{"id": action.id, "before": original}]}));
    repo::put(&conn, &mut clock, &mut applied).unwrap();
    assert_eq!(undo::undo_run(&conn, &mut clock, RUN).unwrap(), 1);
    let back: Action = repo::get(&conn, &action.id).unwrap();
    assert_eq!((back.start_at, back.deleted), (original.start_at, false));
    assert_eq!(repo::get::<Proposal>(&conn, &open.id).unwrap().status, ProposalStatus::Proposed);
}
