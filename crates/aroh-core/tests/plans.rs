use aroh_core::hlc::Hlc;
use aroh_core::models::{Action, ActionStatus, Goal, GoalKind, GoalStatus, Plan, PlanStatus};
use aroh_core::repo::{self, GoalDraft};
use aroh_core::{db, Error};
use rusqlite::Connection;

fn draft_goal(conn: &Connection, clock: &mut Hlc) -> Goal {
    let mut goal = Goal {
        id: aroh_core::new_id(),
        title: "Finish Rust course".into(),
        kind: GoalKind::Course,
        source_url: Some("https://example.com/course".into()),
        target_date: "2026-12-10".into(),
        status: GoalStatus::Draft,
        answers_json: None,
        created_at: "2026-11-20T08:00:00Z".into(),
        updated_at: String::new(),
        deleted: false,
    };
    repo::insert_goal(conn, clock, &mut goal).unwrap();
    goal
}

fn proposed_action(goal_id: &str, seq: i64) -> Action {
    Action {
        id: aroh_core::new_id(),
        goal_id: goal_id.into(),
        title: format!("Lesson {seq}"),
        notes: None,
        start_at: format!("2026-11-{:02}T09:00:00Z", 21 + seq),
        end_at: format!("2026-11-{:02}T09:30:00Z", 21 + seq),
        est_minutes: 30,
        status: ActionStatus::Planned,
        seq,
        done_at: None,
        updated_at: String::new(),
        deleted: false,
    }
}

fn proposed_plan(conn: &Connection, clock: &mut Hlc, goal: &Goal, actions: &[Action]) -> Plan {
    let mut plan = Plan {
        id: aroh_core::new_id(),
        goal_id: goal.id.clone(),
        version: 1,
        status: PlanStatus::Proposed,
        summary: Some("Three short lessons".into()),
        updated_at: String::new(),
        deleted: false,
    };
    repo::insert_plan(conn, clock, &mut plan).unwrap();
    let mut draft = GoalDraft {
        goal_id: goal.id.clone(),
        analysis_json: Some("{\"title\":\"Rust\"}".into()),
        questions_json: None,
        proposed_actions: actions.to_vec(),
        updated_at: String::new(),
    };
    repo::put_draft(conn, clock, &mut draft).unwrap();
    plan
}

#[test]
fn plan_and_draft_storage_round_trip() {
    let conn = db::open_in_memory().unwrap();
    let mut clock = Hlc::new("00000000000000aa");
    let goal = draft_goal(&conn, &mut clock);
    assert!(repo::latest_plan(&conn, &goal.id).unwrap().is_none());
    assert!(repo::get_draft(&conn, &goal.id).unwrap().is_none());

    let actions = vec![proposed_action(&goal.id, 0), proposed_action(&goal.id, 1)];
    let mut plan = proposed_plan(&conn, &mut clock, &goal, &actions);
    assert_eq!(repo::get_plan(&conn, &plan.id).unwrap(), plan);
    assert_eq!(repo::latest_plan(&conn, &goal.id).unwrap(), Some(plan.clone()));
    let draft = repo::get_draft(&conn, &goal.id).unwrap().unwrap();
    assert_eq!(draft.proposed_actions, actions);
    assert_eq!(draft.analysis_json.as_deref(), Some("{\"title\":\"Rust\"}"));

    let before = plan.updated_at.clone();
    plan.summary = Some("Two lessons".into());
    repo::update_plan(&conn, &mut clock, &mut plan).unwrap();
    assert!(plan.updated_at > before);
    assert_eq!(repo::get_plan(&conn, &plan.id).unwrap(), plan);
    assert!(matches!(repo::get_plan(&conn, "missing"), Err(Error::NotFound(_))));
    assert!(repo::list_actions(&conn, None).unwrap().is_empty(), "proposed actions must not reach the calendar");
}

#[test]
fn approve_inserts_actions_once_and_activates_goal() {
    let conn = db::open_in_memory().unwrap();
    let mut clock = Hlc::new("00000000000000aa");
    let goal = draft_goal(&conn, &mut clock);
    let actions: Vec<Action> = (0..3).map(|seq| proposed_action(&goal.id, seq)).collect();
    let plan = proposed_plan(&conn, &mut clock, &goal, &actions);

    let inserted = repo::approve_plan(&conn, &mut clock, &plan.id).unwrap();
    assert_eq!(inserted.len(), 3);
    let listed = repo::list_actions(&conn, Some(&goal.id)).unwrap();
    assert_eq!(listed.len(), 3);
    assert_eq!(listed.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), actions.iter().map(|a| a.id.as_str()).collect::<Vec<_>>());
    assert_eq!(repo::get_plan(&conn, &plan.id).unwrap().status, PlanStatus::Approved);
    assert_eq!(repo::get_goal(&conn, &goal.id).unwrap().status, GoalStatus::Active);
    assert!(repo::get_draft(&conn, &goal.id).unwrap().unwrap().proposed_actions.is_empty());

    assert!(matches!(repo::approve_plan(&conn, &mut clock, &plan.id), Err(Error::InvalidState(_))));
    assert_eq!(repo::list_actions(&conn, None).unwrap().len(), 3, "second approve must insert nothing");
}

#[test]
fn approve_rolls_back_when_an_insert_fails() {
    let conn = db::open_in_memory().unwrap();
    let mut clock = Hlc::new("00000000000000aa");
    let goal = draft_goal(&conn, &mut clock);
    let mut actions = vec![proposed_action(&goal.id, 0), proposed_action(&goal.id, 1)];
    actions[1].id = actions[0].id.clone();
    let plan = proposed_plan(&conn, &mut clock, &goal, &actions);

    assert!(repo::approve_plan(&conn, &mut clock, &plan.id).is_err());
    assert!(repo::list_actions(&conn, None).unwrap().is_empty());
    assert_eq!(repo::get_plan(&conn, &plan.id).unwrap().status, PlanStatus::Proposed);
    assert_eq!(repo::get_goal(&conn, &goal.id).unwrap().status, GoalStatus::Draft);
}
