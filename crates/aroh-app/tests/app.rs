use aroh_app::{local, remind, sample, snapshot, tick, write};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{Action, ActionSeries, ActionStatus, Category, EntrySource, FileKind, Goal, GoalFile, IngestStatus, Preference, PreferenceOrigin, Proposal, ProposalStatus, ProviderSetting, Record, Tracker, TrackerEntry};
use aroh_domain::remind::PlannedKind;
use aroh_domain::schema::SYNCED;
use aroh_store::sync::pending;
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
    assert_eq!(local::setting(&conn, "server_url").unwrap(), local::default_value("server_url"));
    assert_eq!(local::setting(&conn, "goal_budget_usd").unwrap(), None);
    assert!(local::set_setting(&conn, "server_url", "http://engine.example.com").is_err());
    assert!(local::set_setting(&conn, "user_id", "x").is_err());
    assert!(local::set_setting(&conn, "calendars", "maybe").is_err());
    assert!(local::set_setting(&conn, "setup_done", "maybe").is_err());
    local::set_setting(&conn, "setup_done", "true").unwrap();
    assert_eq!(local::setting(&conn, "setup_done").unwrap().as_deref(), Some("true"));
    assert!(local::set_setting(&conn, "goal_budget_usd", "-1").is_err());
    local::set_setting(&conn, "goal_budget_usd", "8").unwrap();
}

#[test]
fn dismissed_insights_hold_at_most_30_uuids() {
    let (conn, _, _) = setup();
    let ids: Vec<String> = (0..30).map(|i| format!("0190f2a0-0000-7000-8000-{i:012}")).collect();
    local::set_setting(&conn, "dismissed_insights", &ids.join(",")).unwrap();
    assert_eq!(local::setting(&conn, "dismissed_insights").unwrap(), Some(ids.join(",")));
    let too_many = format!("{},0190f2a0-0000-7000-8000-999999999999", ids.join(","));
    assert!(local::set_setting(&conn, "dismissed_insights", &too_many).is_err());
    assert!(local::set_setting(&conn, "dismissed_insights", "not-a-uuid").is_err());
    assert!(local::set_setting(&conn, "dismissed_insights", "true").is_err());
}

#[test]
fn day_suggestions_hold_at_most_300_known_kinds() {
    let (conn, _, _) = setup();
    local::set_setting(&conn, "day_suggestions", r#"{"trip":"away","fair":"busy","bday":"reminder"}"#).unwrap();
    assert!(local::setting(&conn, "day_suggestions").unwrap().unwrap().contains("trip"));
    local::set_setting(&conn, "day_suggestions", "{}").unwrap();
    let many: serde_json::Map<String, serde_json::Value> = (0..=300).map(|i| (format!("e{i}"), json!("busy"))).collect();
    assert!(local::set_setting(&conn, "day_suggestions", &serde_json::Value::Object(many).to_string()).is_err());
    assert!(local::set_setting(&conn, "day_suggestions", r#"{"trip":"holiday"}"#).is_err());
    assert!(local::set_setting(&conn, "day_suggestions", r#"{"":"busy"}"#).is_err());
    assert!(local::set_setting(&conn, "day_suggestions", &format!(r#"{{"{}":"busy"}}"#, "x".repeat(256))).is_err());
    assert!(local::set_setting(&conn, "day_suggestions", r#"["busy"]"#).is_err());
    assert!(local::set_setting(&conn, "day_suggestions", "true").is_err());
}

#[test]
fn tick_seeds_materializes_and_is_idempotent() {
    let (conn, mut clock, user) = seeded();
    let first = tick::tick(&conn, &mut clock, &user, now(), ZONE).unwrap();
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
    let second = tick::tick(&conn, &mut clock, &user, now(), ZONE).unwrap();
    assert_eq!((second.seeded, second.materialized, second.missed), (0, 0, 0));
}

#[test]
fn tick_marks_overdue_planned_actions_missed() {
    let (conn, mut clock, user) = seeded();
    tick::tick(&conn, &mut clock, &user, now(), ZONE).unwrap();
    let later = now() + Duration::days(2);
    let report = tick::tick(&conn, &mut clock, &user, later, ZONE).unwrap();
    assert!(report.missed > 0);
    let actions: Vec<Action> = repo::list(&conn, &[]).unwrap();
    let grace = Duration::minutes(tick::MISSED_GRACE_MIN);
    assert!(actions.iter().all(|a| !(a.status == ActionStatus::Planned && a.end_at + grace < later)));
    assert!(actions.iter().any(|a| a.status == ActionStatus::Done));
}

#[test]
fn snapshot_returns_live_rows_of_the_range() {
    let (conn, mut clock, user) = seeded();
    tick::tick(&conn, &mut clock, &user, now(), ZONE).unwrap();
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
    tick::tick(&conn, &mut clock, &user, now(), ZONE).unwrap();
    let planned = repo::list::<Action>(&conn, &[("status", "planned")]).unwrap().remove(0);
    let done = write::set_action_status(&conn, &mut clock, &planned.id, ActionStatus::Done, now()).unwrap();
    assert_eq!(done.done_at, Some(now()));
    let undone = write::set_action_status(&conn, &mut clock, &planned.id, ActionStatus::Planned, now()).unwrap();
    assert_eq!(undone.done_at, None);
    assert!(write::set_action_status(&conn, &mut clock, &planned.id, ActionStatus::Missed, now()).is_err());
}

#[test]
fn edit_action_moves_resizes_deletes_and_locks() {
    let (conn, mut clock, user) = seeded();
    tick::tick(&conn, &mut clock, &user, now(), ZONE).unwrap();
    let planned = repo::list::<Action>(&conn, &[("status", "planned")]).unwrap().remove(0);
    assert!(!planned.locked);
    let start = planned.start_at + Duration::hours(10);
    let moved = write::edit_action(&conn, &mut clock, &planned.id, Some((start, start + Duration::minutes(45)))).unwrap();
    assert_eq!((moved.start_at, moved.end_at, moved.locked), (start, start + Duration::minutes(45), true));
    let resized = write::edit_action(&conn, &mut clock, &planned.id, Some((start, start + Duration::minutes(20)))).unwrap();
    assert_eq!(resized.end_at - resized.start_at, Duration::minutes(20));
    assert!(write::edit_action(&conn, &mut clock, &planned.id, Some((start, start))).is_err());
    assert!(write::edit_action(&conn, &mut clock, &planned.id, Some((start, start + Duration::hours(25)))).is_err());
    let gone = write::edit_action(&conn, &mut clock, &planned.id, None).unwrap();
    assert!(gone.deleted && gone.locked);
    assert!(write::edit_action(&conn, &mut clock, &planned.id, None).is_err());
    let stored: (bool, bool) =
        conn.query_row("SELECT deleted, locked FROM actions WHERE id = ?1", [&planned.id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(stored, (true, true));
}

#[test]
fn reminders_cover_actions_and_check_ins() {
    let (conn, mut clock, user) = seeded();
    tick::tick(&conn, &mut clock, &user, now(), ZONE).unwrap();
    let planned = remind::plan(&conn, &user, now(), ZONE).unwrap();
    assert!(!planned.is_empty() && planned.len() <= remind::CAP);
    assert!(planned.windows(2).all(|w| w[0].fire_at <= w[1].fire_at));
    assert!(planned.iter().all(|p| p.fire_at > now() && p.fire_at <= now() + Duration::hours(24)));
    let next = repo::list::<Action>(&conn, &[("status", "planned")])
        .unwrap()
        .into_iter()
        .filter(|a| a.start_at > now() + Duration::minutes(10))
        .min_by_key(|a| a.start_at)
        .unwrap();
    assert_eq!(planned.iter().filter(|p| p.kind == PlannedKind::Action && p.row_id == next.id).count(), 3);
    let check_in = planned.iter().find(|p| p.kind == PlannedKind::CheckIn).unwrap();
    assert_eq!(check_in.fire_at.with_timezone(&ZONE).format("%H:%M").to_string(), "07:00");
    assert!(check_in.fire_at <= now() + Duration::hours(24));
}

#[test]
fn reminders_wait_for_sign_in_on_an_adopted_store() {
    let (mut conn, _, _) = setup();
    assert!(remind::reminders_allowed(&conn, false).unwrap(), "a local-only store keeps reminders");
    local::adopt(&mut conn, ACCOUNT).unwrap();
    assert!(remind::reminders_allowed(&conn, true).unwrap());
    assert!(!remind::reminders_allowed(&conn, false).unwrap(), "signed out with an account's rows");
}

#[test]
fn synced_seq_is_the_lowest_pull_cursor_of_actions_and_trackers() {
    let (conn, _, user) = setup();
    assert_eq!(remind::synced_seq(&conn, &user).unwrap(), 0);
    aroh_store::sync::set_state(&conn, &format!("pull:{user}:actions"), "40").unwrap();
    assert_eq!(remind::synced_seq(&conn, &user).unwrap(), 0);
    aroh_store::sync::set_state(&conn, &format!("pull:{user}:trackers"), "25").unwrap();
    assert_eq!(remind::synced_seq(&conn, &user).unwrap(), 25);
}

const ACCOUNT: &str = "0190f2a0-0000-7000-8000-0000000000ac";

#[test]
fn adopt_reowns_rows_and_reseeds() {
    let (mut conn, mut clock, local_user) = seeded();
    tick::tick(&conn, &mut clock, &local_user, now(), ZONE).unwrap();
    let old_seeds: Vec<String> = aroh_domain::seed::categories(&local_user).into_iter().map(|c| c.id).collect();
    let goals_before = repo::list::<Goal>(&conn, &[]).unwrap();
    assert!(goals_before.iter().any(|g| g.category_id.as_ref().is_some_and(|c| old_seeds.contains(c))));
    let rows_before: i64 = SYNCED.iter().map(|t| conn.query_row(&format!("SELECT count(*) FROM {}", t.name), [], |r| r.get::<_, i64>(0)).unwrap()).sum();

    local::adopt(&mut conn, ACCOUNT).unwrap();
    assert_eq!(local::user_id(&conn).unwrap(), ACCOUNT);
    for t in SYNCED.iter() {
        let left: i64 = conn.query_row(&format!("SELECT count(*) FROM {} WHERE user_id = ?1", t.name), [&local_user], |r| r.get(0)).unwrap();
        assert_eq!(left, 0, "{} still has local rows", t.name);
    }
    let rows_after: i64 = SYNCED.iter().map(|t| conn.query_row(&format!("SELECT count(*) FROM {}", t.name), [], |r| r.get::<_, i64>(0)).unwrap()).sum();
    assert_eq!(rows_after, rows_before, "nothing is lost; seeded categories are replaced one for one");
    let new_seeds: Vec<String> = aroh_domain::seed::categories(ACCOUNT).into_iter().map(|c| c.id).collect();
    let cats: Vec<String> = repo::list::<Category>(&conn, &[]).unwrap().into_iter().map(|c| c.id).collect();
    assert!(new_seeds.iter().all(|id| cats.contains(id)) && old_seeds.iter().all(|id| !cats.contains(id)));
    for (before, after) in goals_before.iter().zip(repo::list::<Goal>(&conn, &[]).unwrap()) {
        let expected = before.category_id.as_ref().map(|c| old_seeds.iter().position(|o| o == c).map_or(c.clone(), |i| new_seeds[i].clone()));
        assert_eq!(after.category_id, expected);
    }
    let goals = aroh_domain::schema::table("goals").unwrap();
    assert!(!pending(&conn, goals, ACCOUNT, clock.node(), "", 500).unwrap().is_empty(), "adopted rows push under the account");

    local::adopt(&mut conn, ACCOUNT).unwrap();
    assert!(local::adopt(&mut conn, "0190f2a0-0000-7000-8000-0000000000bd").is_err(), "another account is refused");
    assert!(local::adopt(&mut conn, "not-a-uuid").is_err());
}

#[test]
fn remove_sent_keeps_new_logs() {
    let (conn, _, _) = setup();
    for text in ["had pasta", "salad", "had pasta"] {
        local::queue_log(&conn, text, now()).unwrap();
    }
    let sent = local::pending_logs(&conn).unwrap()[..2].to_vec();
    local::queue_log(&conn, "late snack", now()).unwrap();
    assert_eq!(local::remove_sent(&conn, &sent).unwrap(), 2);
    let left: Vec<String> = local::pending_logs(&conn).unwrap().into_iter().map(|p| p.text).collect();
    assert_eq!(left, ["had pasta", "late snack"]);
}

#[test]
fn one_provider_row_per_vendor() {
    let (conn, mut clock, user) = setup();
    let a = write::save_provider(&conn, &mut clock, &user, "anthropic", "claude-sonnet-5-5").unwrap();
    let o = write::save_provider(&conn, &mut clock, &user, "openai", "gpt-6.2-sol").unwrap();
    let again = write::save_provider(&conn, &mut clock, &user, "anthropic", "claude-haiku-5-5").unwrap();
    assert_eq!(again.id, a.id);
    assert_ne!(o.id, a.id);
    let rows = repo::list::<ProviderSetting>(&conn, &[]).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().filter(|p| p.is_default).map(|p| p.vendor.as_str()).collect::<Vec<_>>(), ["anthropic"]);
    assert_eq!(rows.iter().find(|p| p.vendor == "anthropic").unwrap().model, "claude-haiku-5-5");
}

#[test]
fn supabase_settings_validate() {
    let (conn, _, _) = setup();
    local::set_setting(&conn, "supabase_url", "http://127.0.0.1:54321").unwrap();
    local::set_setting(&conn, "supabase_anon_key", "eyJhbGciOiJIUzI1NiJ9.e30.signature").unwrap();
    assert!(local::set_setting(&conn, "supabase_url", "http://evil.example.com").is_err());
    assert!(local::set_setting(&conn, "supabase_anon_key", "short").is_err());
    assert_eq!(local::settings(&conn).unwrap().len(), 2 + usize::from(local::default_value("server_url").is_some()));
}

#[test]
fn build_defaults_fill_unset_settings_and_an_empty_default_counts_as_unset() {
    let defaults = [
        ("supabase_url", Some("https://project.supabase.example")),
        ("supabase_anon_key", Some("")),
        ("server_url", None),
    ];
    assert_eq!(local::default_in(&defaults, "supabase_url").as_deref(), Some("https://project.supabase.example"));
    assert_eq!(local::default_in(&defaults, "supabase_anon_key"), None);
    assert_eq!(local::default_in(&defaults, "server_url"), None);
    assert_eq!(local::default_in(&defaults, "goal_budget_usd"), None);
}

#[test]
fn this_builds_defaults_are_valid_and_apply_until_the_owner_sets_others() {
    let (conn, _, _) = setup();
    for (name, _) in local::DEFAULTS {
        let default = local::default_value(name);
        assert_eq!(local::setting(&conn, name).unwrap(), default, "{name}");
        assert_eq!(local::settings(&conn).unwrap().get(name).cloned(), default, "{name}");
        if let Some(value) = default {
            local::set_setting(&conn, name, &value).unwrap();
        }
    }
    local::set_setting(&conn, "server_url", "http://127.0.0.1:8095").unwrap();
    assert_eq!(local::setting(&conn, "server_url").unwrap().as_deref(), Some("http://127.0.0.1:8095"));
    local::set_setting(&conn, "server_url", " ").unwrap();
    assert_eq!(local::setting(&conn, "server_url").unwrap(), local::default_value("server_url"));
}

fn fact(conn: &Connection, clock: &mut Hlc, user: &str, topic: Option<&str>) -> Preference {
    let mut p = Preference {
        id: aroh_domain::new_id(),
        user_id: user.into(),
        fact: "Free minutes a day: 30 min".into(),
        source: "answer".into(),
        noted_at: now() - Duration::days(9),
        confirmed: true,
        topic: topic.map(str::to_string),
        goal_id: None,
        origin: PreferenceOrigin::Answer,
        blocked: false,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut p).unwrap();
    p
}

#[test]
fn facts_are_edited_blocked_and_allowed_again() {
    let (conn, mut clock, user) = seeded();
    let p = fact(&conn, &mut clock, &user, Some("daily_minutes"));
    let edited = write::edit_fact(&conn, &mut clock, &p.id, "  Free minutes a day: 45 min ", now()).unwrap();
    assert_eq!((edited.fact.as_str(), edited.noted_at, edited.origin), ("Free minutes a day: 45 min", now(), PreferenceOrigin::Answer));
    assert!(write::edit_fact(&conn, &mut clock, &p.id, "   ", now()).is_err(), "a fact is never blank");

    let blocked = write::block_topic(&conn, &mut clock, &p.id).unwrap();
    assert!(blocked.blocked && blocked.confirmed);
    assert_eq!(blocked.fact, "Free minutes a day", "the answer is dropped, the label kept");
    assert!(write::edit_fact(&conn, &mut clock, &p.id, "x", now()).is_err(), "a blocked topic holds no fact");

    let snap = snapshot::snapshot(&conn, &user, now(), now() + Duration::days(1), now()).unwrap();
    assert_eq!(snap.facts.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), vec![p.id.as_str()], "kept facts, blocked included");
    assert!(snap.preferences.iter().all(|s| s.id != p.id), "kept facts never show as Inbox suggestions");

    let untopiced = fact(&conn, &mut clock, &user, None);
    assert!(write::block_topic(&conn, &mut clock, &untopiced.id).is_err(), "only a fact with a topic can be blocked");
    assert!(write::allow_topic(&conn, &mut clock, &untopiced.id).is_err(), "only a blocked topic is allowed again");
    write::allow_topic(&conn, &mut clock, &p.id).unwrap();
    assert!(repo::get::<Preference>(&conn, &p.id).is_err(), "allowing again removes the blocked row");
}

#[test]
fn agent_defaults_hold_an_autonomy_level_and_two_switches() {
    let (conn, _, _) = seeded();
    let good = r#"{"autonomy":"ask","reminders":false,"use_calendars":true}"#;
    local::set_setting(&conn, "agent_defaults", good).unwrap();
    assert_eq!(local::setting(&conn, "agent_defaults").unwrap().as_deref(), Some(good));
    for bad in [r#"{"autonomy":"wild","reminders":true,"use_calendars":true}"#, r#"{"autonomy":"ask"}"#, r#"{"autonomy":"ask","reminders":true,"use_calendars":true,"x":1}"#, "nope"] {
        assert!(local::set_setting(&conn, "agent_defaults", bad).is_err(), "{bad}");
    }
}

#[test]
fn goal_files_are_added_listed_without_text_and_removed() {
    let (conn, mut clock, user) = seeded();
    let goal = repo::list::<Goal>(&conn, &[]).unwrap().remove(0);
    let read = (IngestStatus::Done, Some("Chapter one".to_string()));
    let pdf = write::stored_file(&user, &goal.id, "Joy of X (notes).pdf", "application/pdf", 2048, read);
    assert_eq!(pdf.kind, FileKind::Document);
    let path = pdf.storage_path.clone().unwrap();
    assert!(path.starts_with(&format!("goals/{}/{}-", goal.id, pdf.id)) && path.ends_with("-Joy-of-X--notes-.pdf"), "{path}");
    let pdf = write::add_goal_file(&conn, &mut clock, pdf).unwrap();
    let photo = write::stored_file(&user, &goal.id, "shelf.jpg", "image/jpeg", 10, (IngestStatus::Done, None));
    assert_eq!(photo.kind, FileKind::Image);
    write::add_goal_file(&conn, &mut clock, photo).unwrap();
    let link = write::add_goal_link(&conn, &mut clock, &user, &goal.id, " https://example.org/joy-of-x ").unwrap();
    assert_eq!((link.kind, link.url.as_deref(), link.ingest_status), (FileKind::Link, Some("https://example.org/joy-of-x"), IngestStatus::Pending));
    assert!(write::add_goal_link(&conn, &mut clock, &user, &goal.id, "http://example.org").is_err());
    let early = write::stored_file(&user, "0190f2a0-0000-7000-8000-0000000000ff", "x.txt", "text/plain", 1, (IngestStatus::Done, None));
    let early = write::add_goal_file(&conn, &mut clock, early).unwrap();
    write::remove_goal_file(&conn, &mut clock, &early.id).unwrap();
    let mut gone = goal.clone();
    gone.id = aroh_domain::new_id();
    repo::put(&conn, &mut clock, &mut gone).unwrap();
    repo::delete::<Goal>(&conn, &mut clock, &gone.id).unwrap();
    let late = write::stored_file(&user, &gone.id, "x.txt", "text/plain", 1, (IngestStatus::Done, None));
    assert!(write::add_goal_file(&conn, &mut clock, late).is_err(), "a deleted goal takes no files");

    let snap = snapshot::snapshot(&conn, &user, now(), now() + Duration::days(1), now() - Duration::days(30)).unwrap();
    assert_eq!(snap.files.len(), 3);
    assert!(snap.files.iter().all(|f| f.extracted_text.is_none()), "the snapshot never carries file text");
    assert_eq!(repo::get::<GoalFile>(&conn, &pdf.id).unwrap().extracted_text.as_deref(), Some("Chapter one"));
    write::remove_goal_file(&conn, &mut clock, &pdf.id).unwrap();
    let snap = snapshot::snapshot(&conn, &user, now(), now() + Duration::days(1), now() - Duration::days(30)).unwrap();
    assert_eq!(snap.files.len(), 2);
}

#[test]
fn goal_controls_are_stored_and_bad_limits_refused() {
    let (conn, mut clock, _) = seeded();
    let goal = repo::list::<Goal>(&conn, &[]).unwrap().remove(0);
    let saved = write::set_goal_controls(&conn, &mut clock, &goal.id, false, false, Some(15)).unwrap();
    assert_eq!((saved.reminders, saved.use_calendars, saved.run_limit_minutes), (false, false, Some(15)));
    let stored: Goal = repo::get(&conn, &goal.id).unwrap();
    assert_eq!(stored, saved);
    assert!(write::set_goal_controls(&conn, &mut clock, &goal.id, true, true, Some(121)).is_err());
    assert_eq!(repo::get::<Goal>(&conn, &goal.id).unwrap().run_limit_minutes, Some(15), "a refused write changes nothing");
    assert_eq!(write::set_goal_controls(&conn, &mut clock, &goal.id, true, true, None).unwrap().run_limit_minutes, None);
}

#[test]
fn category_guess_names_a_seeded_category_of_the_owner() {
    let (conn, mut clock, user) = setup();
    tick::tick(&conn, &mut clock, &user, now(), ZONE).unwrap();
    let learning: Category = repo::list::<Category>(&conn, &[("user_id", &user)]).unwrap().into_iter().find(|c| c.name == "Learning").unwrap();
    assert_eq!(write::guess_category(&user, "Finish reading Dune", None), Some(learning.id));
    assert_eq!(write::guess_category(&user, "Ship my side project", Some("before spring")), None);
}

#[test]
fn new_categories_take_the_next_free_hue_or_the_owner_pick() {
    let (conn, mut clock, user) = setup();
    let hues = aroh_domain::category::hues();
    let first = write::add_category(&conn, &mut clock, &user, " Reading club ", None).unwrap();
    assert_eq!((first.name.as_str(), first.color.as_str(), first.seeded), ("Reading club", hues[0], false));
    let second = write::add_category(&conn, &mut clock, &user, "Garden", None).unwrap();
    assert_eq!(second.color, hues[1]);
    assert!(write::add_category(&conn, &mut clock, &user, "reading CLUB", None).is_err(), "names are unique");
    assert!(write::add_category(&conn, &mut clock, &user, "", None).is_err());
    assert!(write::add_category(&conn, &mut clock, &user, "Odd", Some("#123456")).is_err(), "only the 6 hues");
    tick::tick(&conn, &mut clock, &user, now(), ZONE).unwrap();
    assert!(write::add_category(&conn, &mut clock, &user, "Choir", None).is_err(), "past 6 the owner picks a hue");
    let picked = write::add_category(&conn, &mut clock, &user, "Choir", Some(&hues[4].to_uppercase())).unwrap();
    assert_eq!(picked.color, hues[4]);
    assert_eq!(pending(&conn, Category::TABLE, &user, clock.node(), "", 50).unwrap().len(), 3);
}
