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
        timezone: chrono_tz::UTC,
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
fn one_missed_45_minute_action_is_one_small_move_into_tomorrows_free_time() {
    let mut input = week_input();
    input.actions[0] = action("missed", "a", "2026-11-22T09:00:00Z", "2026-11-22T09:45:00Z", ActionStatus::Missed);
    let proposal = plan(&input);
    assert!(proposal.infeasible.is_empty());
    assert_eq!(proposal.diff.len(), 1);
    let op = &proposal.diff[0];
    assert_eq!((op.op, op.action_id.as_str(), op.revive), (Op::Move, "missed", true));
    let after = op.after.as_ref().unwrap();
    assert_eq!((after.start_at.as_str(), after.end_at.as_str(), after.est_minutes), ("2026-11-23T10:00:00Z", "2026-11-23T10:45:00Z", 45));
    assert_eq!(classify(&proposal.diff), Size::Small);
    assert!(validate(&input, &proposal.diff).is_ok());
    let applied = apply(&input.actions, &proposal.diff);
    assert_eq!(applied.iter().find(|a| a.id == "missed").unwrap().status, ActionStatus::Planned);
    assert!(overlaps(&applied).is_empty());
    assert_eq!(undo(&applied, &proposal.diff), input.actions);
}

fn six_missed_hours() -> PlanInput {
    let mut actions = Vec::new();
    for day in 17..=22 {
        actions.push(action(&format!("m{day}"), "a", &format!("2026-11-{day}T18:00:00Z"), &format!("2026-11-{day}T19:00:00Z"), ActionStatus::Missed));
    }
    for day in 23..=30 {
        actions.push(action(&format!("a{day}"), "a", &format!("2026-11-{day}T18:00:00Z"), &format!("2026-11-{day}T19:00:00Z"), ActionStatus::Planned));
    }
    PlanInput {
        goals: vec![goal("a", "2026-11-30")],
        missed_ids: actions.iter().filter(|a| a.status == ActionStatus::Missed).map(|a| a.id.clone()).collect(),
        actions,
        availability: every_day("18:00", "21:00"),
        now: t("2026-11-23T08:00:00Z"),
    }
}

fn goal_minutes_per_day(actions: &[Action]) -> std::collections::BTreeMap<String, i64> {
    let mut days = std::collections::BTreeMap::new();
    for a in actions.iter().filter(|a| a.status == ActionStatus::Planned && !a.deleted) {
        *days.entry(a.start_at[..10].to_string()).or_insert(0) += (t(&a.end_at) - t(&a.start_at)).num_minutes();
    }
    days
}

#[test]
fn six_missed_hours_spread_over_at_least_three_days_under_the_daily_cap() {
    let input = six_missed_hours();
    let proposal = plan(&input);
    assert!(proposal.infeasible.is_empty());
    assert!(validate(&input, &proposal.diff).is_ok());
    let before = goal_minutes_per_day(&input.actions);
    let after = goal_minutes_per_day(&apply(&input.actions, &proposal.diff));
    assert_eq!(after.values().sum::<i64>(), 6 * 60 + 8 * 60);
    assert!(after.values().all(|m| *m <= 120), "{after:?}");
    let grown = after.iter().filter(|(day, m)| **m > before.get(*day).copied().unwrap_or(0)).count();
    assert!(grown >= 3, "{after:?}");
}

#[test]
fn check_llm_output_rejects_a_day_over_the_cap() {
    let input = six_missed_hours();
    let json = r#"{"diff":[{"action_id":"a23","op":"resize",
        "before":{"start_at":"2026-11-23T18:00:00Z","end_at":"2026-11-23T19:00:00Z","est_minutes":60},
        "after":{"start_at":"2026-11-23T18:00:00Z","end_at":"2026-11-23T21:00:00Z","est_minutes":180}}]}"#;
    assert!(validate(&input, &serde_json::from_str::<Proposal>(json).unwrap().diff).is_ok());
    let violations = check_llm_output(&input, json).unwrap_err();
    assert!(violations.contains(&Violation::OverDailyCap { goal_id: "a".into(), date: "2026-11-23".into() }), "{violations:?}");
    let good = serde_json::to_string(&plan(&input)).unwrap();
    assert!(check_llm_output(&input, &good).is_ok());
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
    assert!(overlaps(&applied).is_empty());
    let planned = |actions: &[Action]| goal_minutes_per_day(actions).values().sum::<i64>();
    assert_eq!(planned(&applied) - planned(&input.actions), 100);
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
        revive: false,
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
    diff.push(DiffOp { action_id: "new".into(), op: Op::Create, before: None, after: Some(slot), goal_id: Some("a".into()), title: Some("Catch up".into()), revive: false });
    let a27 = input.actions.iter().find(|a| a.id == "a27").unwrap();
    diff.push(DiffOp {
        action_id: "a27".into(),
        op: Op::Delete,
        before: Some(Slot { start_at: a27.start_at.clone(), end_at: a27.end_at.clone(), est_minutes: a27.est_minutes }),
        after: None,
        goal_id: None,
        title: None,
        revive: false,
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
        revive: false,
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
        actions: vec![action("missed", "a", "2026-11-22T09:00:00Z", "2026-11-22T10:00:00Z", ActionStatus::Missed)],
        availability: every_day("09:00", "09:40"),
        now: t("2026-11-23T08:00:00Z"),
        missed_ids: vec!["missed".into()],
    };
    let proposal = plan(&input);
    assert_eq!(proposal.diff.len(), 1);
    assert_eq!(proposal.infeasible, vec![Infeasible { goal_id: "a".into(), unplaced_minutes: 25 }]);
    let op = &proposal.diff[0];
    assert_eq!(op.op, Op::Create);
    assert_eq!(op.after.as_ref().unwrap().start_at, "2026-11-23T09:00:00Z");
    assert_eq!((op.action_id.len(), op.action_id.as_bytes()[14]), (36, b'7'));
    assert!(validate(&input, &proposal.diff).is_ok());
}

#[test]
fn full_tomorrow_takes_the_missed_work_and_shifts_each_later_action_a_day() {
    let mut input = week_input();
    input.goals = vec![goal("a", "2026-11-28")];
    input.availability = every_day("09:00", "10:00");
    let proposal = plan(&input);
    assert!(proposal.infeasible.is_empty());
    let slots: Vec<(Op, &str, &str, i64)> = proposal
        .diff
        .iter()
        .map(|op| {
            let after = op.after.as_ref().unwrap();
            (op.op, if op.op == Op::Create { "new" } else { op.action_id.as_str() }, after.start_at.as_str(), after.est_minutes)
        })
        .collect();
    assert_eq!(
        slots,
        vec![
            (Op::Create, "new", "2026-11-23T09:00:00Z", 60),
            (Op::Move, "a23", "2026-11-24T09:00:00Z", 60),
            (Op::Move, "a24", "2026-11-25T09:00:00Z", 60),
            (Op::Move, "a25", "2026-11-26T09:00:00Z", 60),
            (Op::Move, "a26", "2026-11-27T09:00:00Z", 60),
            (Op::Move, "a27", "2026-11-28T09:00:00Z", 60),
        ]
    );
    assert_eq!(proposal.diff[0].title.as_deref(), Some("Catch up: missed"));
    assert_eq!(classify(&proposal.diff), Size::Big);
    assert!(validate(&input, &proposal.diff).is_ok());

    input.goals = vec![goal("a", "2026-11-27")];
    let tight = plan(&input);
    assert!(tight.diff.is_empty());
    assert_eq!(tight.infeasible, vec![Infeasible { goal_id: "a".into(), unplaced_minutes: 60 }]);
}

#[test]
fn tomorrow_holds_missed_plus_part_of_next_and_no_piece_is_under_25_minutes() {
    let mut input = week_input();
    input.goals = vec![goal("a", "2026-11-28")];
    input.availability = every_day("09:00", "10:40");
    input.actions.push(action("b23", "b", "2026-11-23T10:00:00Z", "2026-11-23T10:40:00Z", ActionStatus::Planned));
    input.actions.push(action("b24", "b", "2026-11-24T10:00:00Z", "2026-11-24T10:40:00Z", ActionStatus::Planned));
    input.goals.push(goal("b", "2026-11-28"));
    let proposal = plan(&input);
    assert!(proposal.infeasible.is_empty());
    let applied = apply(&input.actions, &proposal.diff);
    let live: Vec<Action> = applied.into_iter().filter(|a| a.id != "missed").collect();
    assert!(overlaps(&live).is_empty());
    assert!(live.iter().all(|a| a.est_minutes >= 25));
    let a23 = live.iter().find(|a| a.id == "a23").unwrap();
    assert_eq!((a23.start_at.as_str(), a23.est_minutes), ("2026-11-24T09:00:00Z", 60));
    let total: i64 = live.iter().filter(|a| a.goal_id == "a").map(|a| a.est_minutes).sum();
    assert_eq!(total, 6 * 60);
    assert!(validate(&input, &proposal.diff).is_ok());
}

#[test]
fn day_windows_follow_the_iana_zone_across_a_dst_change() {
    let input = PlanInput {
        goals: vec![goal("a", "2026-11-02")],
        actions: vec![action("missed", "a", "2026-10-31T13:00:00Z", "2026-10-31T13:30:00Z", ActionStatus::Missed)],
        availability: Availability { timezone: chrono_tz::America::New_York, ..every_day("09:00", "10:00") },
        now: t("2026-11-01T05:00:00Z"),
        missed_ids: vec!["missed".into()],
    };
    let proposal = plan(&input);
    assert!(proposal.infeasible.is_empty());
    assert_eq!(proposal.diff.len(), 1);
    assert_eq!(proposal.diff[0].after.as_ref().unwrap().start_at, "2026-11-01T14:00:00Z");
    assert!(validate(&input, &proposal.diff).is_ok());
}
