use aroh_app::card::{self, CardUndo, LogInput, Scope};
use aroh_app::{local, sample};
use aroh_domain::card::{Card, Checked, Effort, SetLog};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{Action, ActionSeries, Proposal, ProposalSize, ProposalStatus};
use aroh_store::{db, repo};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::Connection;
use serde_json::json;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 26, 8, 15, 0).unwrap()
}

fn seeded() -> (Connection, Hlc, String) {
    let conn = db::open_in_memory().unwrap();
    let user = local::user_id(&conn).unwrap();
    let mut clock = Hlc::new("00000000000000d1");
    assert!(sample::seed(&conn, &mut clock, &user, now(), chrono_tz::Europe::Berlin).unwrap());
    aroh_app::tick::tick(&conn, &mut clock, &user, now(), chrono_tz::Europe::Berlin).unwrap();
    (conn, clock, user)
}

fn card(name: &str) -> Card {
    serde_json::from_value(json!({"v": 1, "depth": "full", "why": "Legs.", "outline": null,
        "blocks": [{"kind": "workout", "exercises": [{"name": name, "unit": "kg", "rest_s": 120, "cue": null, "sets": [{"reps": 8, "weight": 60.0}]}]}]}))
    .unwrap()
}

fn future_series_actions(conn: &Connection, user: &str) -> Vec<Action> {
    let mut all: Vec<Action> = repo::list::<Action>(conn, &[("user_id", user)]).unwrap().into_iter().filter(|a| a.series_id.is_some() && a.start_at > now()).collect();
    all.sort_by_key(|a| a.start_at);
    let series = all[0].series_id.clone();
    all.into_iter().filter(|a| a.series_id == series).collect()
}

fn name_of(a: &Action) -> Option<String> {
    a.card.as_ref().map(|c| serde_json::to_value(c).unwrap()["blocks"][0]["exercises"][0]["name"].as_str().unwrap().to_string())
}

#[test]
fn a_manual_edit_reaches_this_or_all_future_sessions_and_undoes() {
    let (conn, mut clock, user) = seeded();
    let actions = future_series_actions(&conn, &user);
    assert!(actions.len() >= 3);
    let (first, second) = (&actions[0], &actions[1]);
    let undo = card::set_card(&conn, &mut clock, &first.id, card("Squat"), None, Scope::This).unwrap();
    assert_eq!(undo.rows.len(), 1);
    let read = |id: &str| repo::get::<Action>(&conn, id).unwrap();
    assert_eq!(name_of(&read(&first.id)).as_deref(), Some("Squat"));
    assert_eq!(name_of(&read(&second.id)), second.card.as_ref().map(|_| name_of(second).unwrap()));

    card::save_log(&conn, &mut clock, &second.id, LogInput { sets: vec![], checked: vec![Checked { block: 0, row: 0 }], effort: None, note: None }, now()).unwrap();
    let all = card::set_card(&conn, &mut clock, &first.id, card("Split squat"), Some(45), Scope::Future).unwrap();
    assert_eq!(all.rows.len(), actions.len() + 1 - 1, "series plus every later unlogged session, the logged one kept");
    assert_eq!(name_of(&read(&first.id)).as_deref(), Some("Split squat"));
    assert_eq!((read(&first.id).end_at - read(&first.id).start_at).num_minutes(), 45);
    assert_ne!(name_of(&read(&second.id)).as_deref(), Some("Split squat"), "a logged session keeps its card");
    assert_eq!(name_of(&read(&actions[2].id)).as_deref(), Some("Split squat"));
    let series: ActionSeries = repo::get(&conn, first.series_id.as_deref().unwrap()).unwrap();
    assert_eq!(series.duration_minutes, 45);

    card::restore_cards(&conn, &mut clock, &all).unwrap();
    assert_eq!(name_of(&read(&first.id)).as_deref(), Some("Squat"));
    assert_eq!(read(&actions[2].id).end_at, actions[2].end_at);
    let series: ActionSeries = repo::get(&conn, first.series_id.as_deref().unwrap()).unwrap();
    assert_eq!(series.duration_minutes, (first.end_at - first.start_at).num_minutes());
    assert!(card::set_card(&conn, &mut clock, &first.id, card(""), None, Scope::This).is_err(), "an exercise needs a name");
    assert!(card::set_card(&conn, &mut clock, &first.id, card("Squat"), Some(0), Scope::This).is_err());
}

#[test]
fn a_chat_edit_undoes_once_and_logs_keep_one_row_per_session() {
    let (conn, mut clock, user) = seeded();
    let actions = future_series_actions(&conn, &user);
    let a = &actions[0];
    let undo = card::set_card(&conn, &mut clock, &a.id, card("Lunge"), None, Scope::This).unwrap();
    let mut p = Proposal {
        id: aroh_domain::new_id(),
        user_id: user.clone(),
        goal_id: a.goal_id.clone(),
        run_id: None,
        summary: "Swapped.".into(),
        diff: json!({"op": "card", "action_id": a.id, "scope": "this"}),
        size: ProposalSize::Small,
        status: ProposalStatus::Applied,
        undo: Some(serde_json::to_value(&undo).unwrap()),
        created_at: now(),
        sealed_summary: None,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(&conn, &mut clock, &mut p).unwrap();
    assert_eq!(card::latest_card_edit(&conn, &user, &a.id).unwrap().unwrap().id, p.id);
    assert_eq!(card::undo_card_edit(&conn, &mut clock, &p.id).unwrap().status, ProposalStatus::Undone);
    assert_eq!(repo::get::<Action>(&conn, &a.id).unwrap().card, a.card);
    assert!(card::undo_card_edit(&conn, &mut clock, &p.id).is_err(), "undo runs once");
    assert!(card::restore_cards(&conn, &mut clock, &CardUndo { rows: vec![json!({"table": "goals", "id": a.id, "card": null})] }).is_err());

    let set = SetLog { block: 0, row: 0, set: 0, reps: 7, weight: Some(60.0), done: true };
    let first = card::save_log(&conn, &mut clock, &a.id, LogInput { sets: vec![set.clone()], checked: vec![], effort: None, note: Some("  ".into()) }, now()).unwrap();
    let again = card::save_log(&conn, &mut clock, &a.id, LogInput { sets: vec![set], checked: vec![], effort: Some(Effort::Hard), note: Some(" heavy ".into()) }, now()).unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(first.note, None);
    let stored = card::log_of(&conn, &a.id).unwrap().unwrap();
    assert_eq!((stored.effort, stored.note.as_deref()), (Some(Effort::Hard), Some("heavy")));
    assert_eq!(card::log_of(&conn, &actions[1].id).unwrap(), None);
    assert_eq!(card::set_notes(&conn, &mut clock, &a.id, "Belt on for the top sets").unwrap().notes.as_deref(), Some("Belt on for the top sets"));
    assert_eq!(card::set_notes(&conn, &mut clock, &a.id, "  ").unwrap().notes, None);
    assert!(card::set_notes(&conn, &mut clock, &a.id, &"x".repeat(4001)).is_err());
    let bad = SetLog { block: 99, row: 0, set: 0, reps: 1, weight: None, done: true };
    assert!(card::save_log(&conn, &mut clock, &a.id, LogInput { sets: vec![bad], checked: vec![], effort: None, note: None }, now()).is_err());
}
