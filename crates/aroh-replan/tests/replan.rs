use aroh_replan::*;
use chrono::{DateTime, Utc};

fn t(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text).unwrap().with_timezone(&Utc)
}

fn goal(id: &str, target: &str) -> Goal {
    Goal {
        id: id.into(),
        title: id.into(),
        kind: GoalKind::Course,
        source_url: None,
        target_date: target.into(),
        status: GoalStatus::Active,
        answers_json: None,
        created_at: "2026-11-01T00:00:00Z".into(),
        updated_at: "0".into(),
        deleted: false,
    }
}

fn action(id: &str, goal_id: &str, start: &str, end: &str, status: ActionStatus) -> Action {
    Action {
        id: id.into(),
        goal_id: goal_id.into(),
        title: id.into(),
        notes: None,
        start_at: start.into(),
        end_at: end.into(),
        est_minutes: (t(end) - t(start)).num_minutes(),
        status,
        seq: 0,
        done_at: None,
        updated_at: "0".into(),
        deleted: false,
    }
}

fn every_day(start: &str, end: &str) -> Availability {
    Availability {
        windows: (0..7)
            .map(|weekday| Window {
                weekday,
                start: HhMm::parse(start).unwrap(),
                end: HhMm::parse(end).unwrap(),
            })
            .collect(),
        utc_offset_minutes: 0,
    }
}

fn week_input() -> PlanInput {
    let mut actions = vec![action(
        "missed",
        "a",
        "2026-11-22T09:00:00Z",
        "2026-11-22T10:00:00Z",
        ActionStatus::Missed,
    )];
    for day in 23..=27 {
        actions.push(action(
            &format!("a{day}"),
            "a",
            &format!("2026-11-{day}T09:00:00Z"),
            &format!("2026-11-{day}T10:00:00Z"),
            ActionStatus::Planned,
        ));
    }
    PlanInput {
        goals: vec![goal("a", "2026-11-27")],
        actions,
        availability: every_day("09:00", "12:00"),
        now: t("2026-11-23T08:00:00Z"),
        missed_ids: vec!["missed".into()],
    }
}

fn overlaps(actions: &[Action]) -> Vec<(String, String)> {
    let live: Vec<&Action> = actions
        .iter()
        .filter(|a| a.status == ActionStatus::Planned && !a.deleted)
        .collect();
    let mut found = Vec::new();
    for (i, a) in live.iter().enumerate() {
        for b in &live[i + 1..] {
            if t(&a.start_at) < t(&b.end_at) && t(&b.start_at) < t(&a.end_at) {
                found.push((a.id.clone(), b.id.clone()));
            }
        }
    }
    found
}

#[test]
fn keeps_missed_45_minutes_whole_on_the_next_free_day() {
    let mut input = week_input();
    input.actions[0] = action("missed", "a", "2026-11-22T09:00:00Z", "2026-11-22T09:45:00Z", ActionStatus::Missed);
    let proposal = plan(&input);
    assert!(proposal.infeasible.is_empty());
    assert_eq!(proposal.diff.len(), 1);
    let op = &proposal.diff[0];
    assert_eq!(op.op, Op::Resize);
    assert_eq!(op.action_id, "a23");
    assert_eq!(op.after.as_ref().unwrap().est_minutes, 105);
    assert!(validate(&input, &proposal.diff).is_ok());
}

#[test]
fn spills_to_later_days_only_in_chunks_of_at_least_25_minutes() {
    let mut input = week_input();
    input.availability = every_day("09:00", "10:30");
    let proposal = plan(&input);
    assert!(proposal.infeasible.is_empty());
    let grown: Vec<(&str, i64)> = proposal
        .diff
        .iter()
        .map(|op| (op.action_id.as_str(), op.after.as_ref().unwrap().est_minutes - op.before.as_ref().unwrap().est_minutes))
        .collect();
    assert_eq!(grown, vec![("a23", 30), ("a24", 30)]);
    assert!(validate(&input, &proposal.diff).is_ok());
}

#[test]
fn flags_infeasible_minutes_when_no_room_before_deadline() {
    let input = PlanInput {
        goals: vec![goal("a", "2026-11-23")],
        actions: vec![
            action("missed", "a", "2026-11-22T09:00:00Z", "2026-11-22T09:30:00Z", ActionStatus::Missed),
            action("a1", "a", "2026-11-23T09:00:00Z", "2026-11-23T10:00:00Z", ActionStatus::Planned),
        ],
        availability: every_day("09:00", "10:00"),
        now: t("2026-11-23T08:00:00Z"),
        missed_ids: vec!["missed".into()],
    };
    let proposal = plan(&input);
    assert!(proposal.diff.is_empty());
    assert_eq!(
        proposal.infeasible,
        vec![Infeasible { goal_id: "a".into(), unplaced_minutes: 30 }]
    );
}

#[test]
fn never_overlaps_across_two_goals() {
    let mut actions = vec![
        action("ma", "a", "2026-11-22T09:00:00Z", "2026-11-22T10:00:00Z", ActionStatus::Missed),
        action("mb", "b", "2026-11-22T10:00:00Z", "2026-11-22T10:40:00Z", ActionStatus::Missed),
    ];
    for day in 23..=24 {
        actions.push(action(&format!("a{day}"), "a", &format!("2026-11-{day}T09:00:00Z"), &format!("2026-11-{day}T10:00:00Z"), ActionStatus::Planned));
        actions.push(action(&format!("b{day}"), "b", &format!("2026-11-{day}T10:00:00Z"), &format!("2026-11-{day}T11:00:00Z"), ActionStatus::Planned));
    }
    let input = PlanInput {
        goals: vec![goal("a", "2026-11-24"), goal("b", "2026-11-24")],
        actions,
        availability: every_day("09:00", "12:00"),
        now: t("2026-11-23T08:00:00Z"),
        missed_ids: vec!["ma".into(), "mb".into()],
    };
    let proposal = plan(&input);
    assert!(proposal.infeasible.is_empty());
    let applied = apply(&input.actions, &proposal.diff);
    let planned: Vec<Action> = applied.into_iter().filter(|a| !input.missed_ids.contains(&a.id)).collect();
    assert!(overlaps(&planned).is_empty());
    let added: i64 = proposal
        .diff
        .iter()
        .map(|op| op.after.as_ref().unwrap().est_minutes - op.before.as_ref().map_or(0, |b| b.est_minutes))
        .sum();
    assert_eq!(added, 100);
    assert!(validate(&input, &proposal.diff).is_ok());
}

fn op(kind: Op) -> DiffOp {
    let slot = Slot { start_at: "2026-11-23T09:00:00Z".into(), end_at: "2026-11-23T10:00:00Z".into(), est_minutes: 60 };
    DiffOp {
        action_id: "x".into(),
        op: kind,
        before: if kind == Op::Create { None } else { Some(slot.clone()) },
        after: if kind == Op::Delete { None } else { Some(slot) },
        goal_id: None,
        title: None,
    }
}

#[test]
fn classifies_small_and_big() {
    assert_eq!(classify(&[]), Size::Small);
    assert_eq!(classify(&[op(Op::Move), op(Op::Resize), op(Op::Move)]), Size::Small);
    assert_eq!(classify(&[op(Op::Move), op(Op::Move), op(Op::Move), op(Op::Move)]), Size::Big);
    assert_eq!(classify(&[op(Op::Create)]), Size::Big);
    assert_eq!(classify(&[op(Op::Delete)]), Size::Big);
}

#[test]
fn apply_then_undo_is_identity() {
    let input = week_input();
    let mut diff = plan(&input).diff;
    let slot = Slot { start_at: "2026-11-28T09:00:00Z".into(), end_at: "2026-11-28T09:30:00Z".into(), est_minutes: 30 };
    diff.push(DiffOp { action_id: "new".into(), op: Op::Create, before: None, after: Some(slot), goal_id: Some("a".into()), title: Some("Catch up".into()) });
    let a27 = input.actions.iter().find(|a| a.id == "a27").unwrap();
    diff.push(DiffOp {
        action_id: "a27".into(),
        op: Op::Delete,
        before: Some(Slot { start_at: a27.start_at.clone(), end_at: a27.end_at.clone(), est_minutes: a27.est_minutes }),
        after: None,
        goal_id: None,
        title: None,
    });
    let applied = apply(&input.actions, &diff);
    assert_ne!(applied, input.actions);
    assert_eq!(undo(&applied, &diff), input.actions);
}

#[test]
fn validate_rejects_overlap() {
    let input = week_input();
    let diff = vec![DiffOp {
        action_id: "a24".into(),
        op: Op::Move,
        before: Some(Slot { start_at: "2026-11-24T09:00:00Z".into(), end_at: "2026-11-24T10:00:00Z".into(), est_minutes: 60 }),
        after: Some(Slot { start_at: "2026-11-23T09:30:00Z".into(), end_at: "2026-11-23T10:30:00Z".into(), est_minutes: 60 }),
        goal_id: None,
        title: None,
    }];
    let violations = validate(&input, &diff).unwrap_err();
    assert!(violations.contains(&Violation::Overlap { action_id: "a24".into(), other_id: "a23".into() }));
}

#[test]
fn detects_missed_after_grace() {
    let actions = vec![
        action("late", "a", "2026-11-23T08:00:00Z", "2026-11-23T09:00:00Z", ActionStatus::Planned),
        action("early", "a", "2026-11-23T07:00:00Z", "2026-11-23T08:00:00Z", ActionStatus::Planned),
        action("grace", "a", "2026-11-23T09:00:00Z", "2026-11-23T09:50:00Z", ActionStatus::Planned),
        action("done", "a", "2026-11-23T06:00:00Z", "2026-11-23T07:00:00Z", ActionStatus::Done),
    ];
    assert_eq!(detect_missed(&actions, t("2026-11-23T10:00:00Z"), 15), vec!["early".to_string(), "late".to_string()]);
}

#[test]
fn check_llm_output_rejects_overlap_and_bad_json() {
    let input = week_input();
    let json = r#"{"diff":[{"action_id":"a24","op":"move",
        "before":{"start_at":"2026-11-24T09:00:00Z","end_at":"2026-11-24T10:00:00Z","est_minutes":60},
        "after":{"start_at":"2026-11-23T09:30:00Z","end_at":"2026-11-23T10:30:00Z","est_minutes":60}}]}"#;
    let violations = check_llm_output(&input, json).unwrap_err();
    assert!(violations.contains(&Violation::Overlap { action_id: "a24".into(), other_id: "a23".into() }));
    assert!(matches!(check_llm_output(&input, "not json").unwrap_err()[0], Violation::BadJson { .. }));
    let good = serde_json::to_string(&plan(&input)).unwrap();
    assert_eq!(check_llm_output(&input, &good).unwrap(), plan(&input));
}

#[test]
fn deterministic_agent_returns_deterministic_proposal() {
    let input = week_input();
    let deterministic = plan(&input);
    let got = futures_lite_block_on(DeterministicAgent.propose(&input, deterministic.clone()));
    assert_eq!(got, deterministic);
}

fn futures_lite_block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(future)
}

#[test]
fn creates_catch_up_action_with_uuid_v7_id() {
    let input = PlanInput {
        goals: vec![goal("a", "2026-11-23")],
        actions: vec![action("missed", "a", "2026-11-22T09:00:00Z", "2026-11-22T09:30:00Z", ActionStatus::Missed)],
        availability: every_day("09:00", "10:00"),
        now: t("2026-11-23T08:00:00Z"),
        missed_ids: vec!["missed".into()],
    };
    let proposal = plan(&input);
    assert_eq!(proposal.diff.len(), 1);
    let op = &proposal.diff[0];
    assert_eq!(op.op, Op::Create);
    assert_eq!(op.after.as_ref().unwrap().start_at, "2026-11-23T09:00:00Z");
    assert_eq!((op.action_id.len(), op.action_id.as_bytes()[14]), (36, b'7'));
    assert!(validate(&input, &proposal.diff).is_ok());
}
