use aroh_app::{calendar, local, snapshot, tick, write};
use aroh_domain::calendar::{day_rule, rules_for, DeviceEvent, RuleSource};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{Action, ActionSeries, ActionStatus, BusyBlock, CalendarRule, DayKind, RuleDecision};
use aroh_store::sync::pending;
use aroh_store::{db, repo};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::Connection;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 26, 8, 15, 0).unwrap()
}

fn setup() -> (Connection, Hlc, String) {
    let conn = db::open_in_memory().unwrap();
    let user = local::user_id(&conn).unwrap();
    (conn, Hlc::new("00000000000000d1"), user)
}

fn event(id: &str, title: &str, start: DateTime<Utc>) -> DeviceEvent {
    DeviceEvent {
        calendar_id: "work".into(),
        calendar: "Work".into(),
        event_id: id.into(),
        title: title.into(),
        start_at: start,
        end_at: start + Duration::minutes(30),
        all_day: false,
        rrule: None,
        holiday: false,
    }
}

fn source(id: &str, title: &str, recurring: bool) -> RuleSource {
    RuleSource { calendar_id: "work".into(), event_id: id.into(), title: title.into(), recurring }
}

fn store_rules(conn: &Connection, clock: &mut Hlc, user: &str, group: &str, decision: RuleDecision, sources: &[RuleSource]) {
    for mut rule in rules_for(user, group, decision, sources) {
        repo::put(conn, clock, &mut rule).unwrap();
    }
}

fn window() -> (DateTime<Utc>, DateTime<Utc>) {
    (now() - Duration::days(1), now() + Duration::days(14))
}

#[test]
fn import_keeps_matched_and_returns_unmatched_once_per_series() {
    let (conn, mut clock, user) = setup();
    store_rules(&conn, &mut clock, &user, "Work", RuleDecision::Keep, &[source("standup", "Standup", true)]);
    let day = |d: i64| now() + Duration::days(d);
    let events = vec![
        event("standup", "Standup", day(1)),
        event("standup", "Standup", day(2)),
        event("yoga", "Yoga", day(3)),
        event("yoga", "Yoga", day(1)),
        event("standup", "Standup", day(30)),
    ];
    let (from, to) = window();
    let report = calendar::import(&conn, &mut clock, &user, &events, from, to).unwrap();
    assert_eq!((report.kept, report.ignored, report.removed), (2, 0, 0));
    assert_eq!(report.unmatched.len(), 1);
    assert_eq!(report.unmatched[0].start_at, day(1));
    let blocks: Vec<BusyBlock> = repo::list(&conn, &[]).unwrap();
    assert_eq!(blocks.len(), 2);
    assert!(blocks.iter().all(|b| b.group_name.as_deref() == Some("Work")));
    let snap = snapshot::snapshot(&conn, &user, now(), day(2) - Duration::minutes(1), now(), now()).unwrap();
    assert_eq!(snap.busy.len(), 1);
    assert!(snap.actions.is_empty());
}

#[test]
fn import_tombstones_moved_events() {
    let (conn, mut clock, user) = setup();
    store_rules(&conn, &mut clock, &user, "Work", RuleDecision::Keep, &[source("review", "Design review", false)]);
    let (from, to) = window();
    let first = now() + Duration::days(1);
    calendar::import(&conn, &mut clock, &user, &[event("review", "Design review", first)], from, to).unwrap();
    let later = first + Duration::hours(2);
    let moved = calendar::import(&conn, &mut clock, &user, &[event("review", "Design review", later)], from, to).unwrap();
    assert_eq!((moved.kept, moved.removed), (1, 1));
    let live: Vec<BusyBlock> = repo::list(&conn, &[]).unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].start_at, later);
}

#[test]
fn ignored_timed_events_make_no_blocks() {
    let (conn, mut clock, user) = setup();
    store_rules(&conn, &mut clock, &user, "Noise", RuleDecision::Ignore, &[source("holiday", "Holiday", false)]);
    let (from, to) = window();
    let events = [event("h2", "Public holiday", now() + Duration::days(1))];
    let report = calendar::import(&conn, &mut clock, &user, &events, from, to).unwrap();
    assert_eq!((report.kept, report.ignored, report.unmatched.len()), (0, 1, 0));
    assert!(repo::list::<BusyBlock>(&conn, &[]).unwrap().is_empty());
    let mut bad = event("x", "x", now());
    bad.end_at = bad.start_at;
    assert!(calendar::import(&conn, &mut clock, &user, &[bad], from, to).is_err());
}

const BERLIN: Tz = chrono_tz::Europe::Berlin;

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
}

fn local(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    BERLIN.from_local_datetime(&date(day).and_hms_opt(hour, minute, 0).unwrap()).unwrap().with_timezone(&Utc)
}

fn all_day(id: &str, title: &str, first: u32, days: u32) -> DeviceEvent {
    DeviceEvent { start_at: local(first, 0, 0), end_at: local(first + days, 0, 0), all_day: true, ..event(id, title, now()) }
}

fn block_of(conn: &Connection, event_id: &str) -> BusyBlock {
    let mut blocks: Vec<BusyBlock> = repo::list(conn, &[("event_id", event_id)]).unwrap();
    assert_eq!(blocks.len(), 1, "{event_id}");
    blocks.remove(0)
}

fn action(conn: &Connection, clock: &mut Hlc, user: &str, title: &str, start: DateTime<Utc>, minutes: i64, status: ActionStatus) -> Action {
    let mut a = Action {
        id: aroh_domain::new_id(),
        user_id: user.into(),
        goal_id: "0190f2a0-0000-7000-8000-000000000091".into(),
        series_id: None,
        occurrence: None,
        title: title.into(),
        notes: None,
        start_at: start,
        end_at: start + Duration::minutes(minutes),
        status,
        locked: false,
        done_at: None,
        card: None,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut a).unwrap();
    a
}

#[test]
fn all_day_events_without_a_rule_wait_for_a_kind_and_go_to_triage() {
    let (conn, mut clock, user) = setup();
    store_rules(&conn, &mut clock, &user, "Noise", RuleDecision::Ignore, &[source("birthday", "Birthday", false)]);
    let (from, to) = window();
    let trip = all_day("trip", "Trip", 23, 5);
    let birthday = all_day("birthday", "Birthday", 28, 1);
    let report = calendar::import(&conn, &mut clock, &user, &[trip, birthday], from, to).unwrap();
    assert_eq!((report.kept, report.ignored), (2, 0), "a trip that began before the window still shows");
    assert_eq!(report.unmatched.iter().map(|e| e.event_id.as_str()).collect::<Vec<_>>(), ["trip"]);
    let trip = block_of(&conn, "trip");
    assert!(trip.all_day);
    assert_eq!((trip.day_kind, trip.group_name.as_deref()), (None, None));
    let birthday = block_of(&conn, "birthday");
    assert_eq!((birthday.day_kind, birthday.group_name.as_deref()), (None, Some("Noise")), "an all-day event keeps its chip");
}

#[test]
fn a_kind_rule_decides_all_day_events_but_never_timed_ones() {
    let (conn, mut clock, user) = setup();
    let mut rule = day_rule(&user, "work", "conf", "Conference", "All day", DayKind::Busy);
    repo::put(&conn, &mut clock, &mut rule).unwrap();
    let mut title = rules_for(&user, "Family", RuleDecision::Keep, &[source("x", "Holiday", false)]).remove(0);
    title.day_kind = Some(DayKind::Reminder);
    repo::put(&conn, &mut clock, &mut title).unwrap();
    let (from, to) = window();
    let events = [all_day("conf", "Conference", 27, 2), event("h", "Holiday party", now() + Duration::days(1))];
    let report = calendar::import(&conn, &mut clock, &user, &events, from, to).unwrap();
    assert_eq!((report.kept, report.unmatched.len()), (2, 0));
    assert_eq!(block_of(&conn, "conf").day_kind, Some(DayKind::Busy));
    assert_eq!(block_of(&conn, "h").day_kind, None, "a title rule's kind never reaches a timed event");
    rule.day_kind = Some(DayKind::Away);
    repo::put(&conn, &mut clock, &mut rule).unwrap();
    calendar::import(&conn, &mut clock, &user, &events, from, to).unwrap();
    assert_eq!(block_of(&conn, "conf").day_kind, Some(DayKind::Away), "a changed kind is written on the next import");
}

#[test]
fn busy_day_writes_a_rule_and_moves_planned_actions_to_the_next_free_day() {
    let (conn, mut clock, user) = setup();
    let (from, to) = window();
    let mut recurring = all_day("retreat", "Retreat", 27, 1);
    recurring.rrule = Some("FREQ=WEEKLY".into());
    let mut later = recurring.clone();
    later.start_at = local(27, 0, 0) + Duration::days(7);
    later.end_at = later.start_at + Duration::days(1);
    let events = [recurring, later, all_day("fair", "Book fair", 28, 1), event("lunch", "Lunch", local(29, 12, 0))];
    store_rules(&conn, &mut clock, &user, "Work", RuleDecision::Keep, &[source("lunch", "Lunch", false)]);
    calendar::import(&conn, &mut clock, &user, &events, from, to).unwrap();
    let fair = repo::list::<BusyBlock>(&conn, &[("event_id", "fair")]).unwrap().remove(0);
    write::set_day_kind(&conn, &mut clock, &user, &fair.id, DayKind::Busy, date(26), BERLIN).unwrap();

    let run = action(&conn, &mut clock, &user, "Run", local(27, 9, 0), 60, ActionStatus::Planned);
    let done = action(&conn, &mut clock, &user, "Stretch", local(27, 7, 0), 30, ActionStatus::Done);
    let piano = action(&conn, &mut clock, &user, "Piano", local(27, 18, 0), 30, ActionStatus::Planned);
    action(&conn, &mut clock, &user, "Dinner prep", local(29, 18, 15), 30, ActionStatus::Planned);
    let core = action(&conn, &mut clock, &user, "Core", local(27, 12, 0), 30, ActionStatus::Planned);
    let other = action(&conn, &mut clock, &user, "Read", local(26, 9, 0), 30, ActionStatus::Planned);

    let retreat = repo::list::<BusyBlock>(&conn, &[("event_id", "retreat")]).unwrap();
    assert_eq!(retreat.len(), 2);
    let set = write::set_day_kind(&conn, &mut clock, &user, &retreat[0].id, DayKind::Busy, date(26), BERLIN).unwrap();
    assert_eq!(set.rule.series_key.as_deref(), Some("retreat"));
    assert_eq!((set.rule.day_kind, set.rule.decision), (Some(DayKind::Busy), RuleDecision::Keep));
    assert_eq!(set.blocks, 2);
    assert!(set.stuck.is_empty());
    assert!(repo::list::<BusyBlock>(&conn, &[("event_id", "retreat")]).unwrap().iter().all(|b| b.day_kind == Some(DayKind::Busy)));
    let at = |a: &Action| repo::get::<Action>(&conn, &a.id).unwrap();
    assert_eq!(at(&run).start_at, local(29, 9, 0), "the book fair on the 28th is a Busy day too");
    assert_eq!(at(&run).end_at, local(29, 10, 0));
    assert_eq!(at(&piano).start_at, local(30, 18, 0), "dinner prep holds the 29th at 18:00");
    assert_eq!(at(&core).start_at, local(30, 12, 0), "lunch is busy time on the 29th");
    assert_eq!(at(&done).start_at, local(27, 7, 0), "done actions stay");
    assert_eq!(at(&other).start_at, local(26, 9, 0));
    assert!(!at(&run).locked, "moves by the app stay unlocked");
    assert_eq!(set.moved.len(), 3);

    let again = write::set_day_kind(&conn, &mut clock, &user, &retreat[0].id, DayKind::Reminder, date(26), BERLIN).unwrap();
    assert!(again.moved.is_empty());
    assert_eq!(again.rule.id, set.rule.id);
    assert_eq!(at(&run).start_at, local(29, 9, 0), "changing to Reminder later moves nothing back");
}

#[test]
fn reminder_and_away_move_nothing_and_timed_blocks_have_no_kind() {
    let (conn, mut clock, user) = setup();
    let (from, to) = window();
    store_rules(&conn, &mut clock, &user, "Work", RuleDecision::Keep, &[source("standup", "Standup", true)]);
    calendar::import(&conn, &mut clock, &user, &[all_day("bday", "Mom's birthday", 28, 1), event("standup", "Standup", local(27, 9, 0))], from, to).unwrap();
    let run = action(&conn, &mut clock, &user, "Run", local(28, 9, 0), 60, ActionStatus::Planned);
    let bday = block_of(&conn, "bday");
    for kind in [DayKind::Reminder, DayKind::Away] {
        let set = write::set_day_kind(&conn, &mut clock, &user, &bday.id, kind, date(26), BERLIN).unwrap();
        assert!(set.moved.is_empty());
        assert_eq!(block_of(&conn, "bday").day_kind, Some(kind));
        assert_eq!(repo::get::<Action>(&conn, &run.id).unwrap().start_at, local(28, 9, 0));
    }
    let rules: Vec<CalendarRule> = repo::list(&conn, &[("series_key", "bday")]).unwrap();
    assert_eq!(rules.len(), 1, "one rule per event, overwritten");
    let standup = block_of(&conn, "standup");
    assert!(write::set_day_kind(&conn, &mut clock, &user, &standup.id, DayKind::Busy, date(26), BERLIN).is_err());
    assert!(write::set_day_kind(&conn, &mut clock, &user, "0190f2a0-0000-7000-8000-0000000000ff", DayKind::Busy, date(26), BERLIN).is_err());
}

#[test]
fn import_twice_writes_nothing_new() {
    let (conn, mut clock, user) = setup();
    store_rules(&conn, &mut clock, &user, "Work", RuleDecision::Keep, &[source("standup", "Standup", true)]);
    let (from, to) = window();
    let events = [event("standup", "Standup", now() + Duration::days(1))];
    calendar::import(&conn, &mut clock, &user, &events, from, to).unwrap();
    let table = aroh_domain::schema::table("busy_blocks").unwrap();
    let pushed = pending(&conn, table, &user, clock.node(), "", 500).unwrap();
    assert_eq!(pushed.len(), 1);
    let last = pushed[0]["updated_at"].as_str().unwrap().to_string();
    let again = calendar::import(&conn, &mut clock, &user, &events, from, to).unwrap();
    assert_eq!(again.kept, 1);
    assert!(pending(&conn, table, &user, clock.node(), &last, 500).unwrap().is_empty());
}

fn busy_event(conn: &Connection, clock: &mut Hlc, user: &str, id: &str, first: u32, days: i64) {
    let mut rule = day_rule(user, "work", id, "Conference", "All day", DayKind::Busy);
    repo::put(conn, clock, &mut rule).unwrap();
    let start = local(first, 0, 0);
    let e = DeviceEvent { start_at: start, end_at: start + Duration::days(days), all_day: true, ..event(id, "Conference", now()) };
    let (from, to) = window();
    let report = calendar::import(conn, clock, user, &[e], from, to).unwrap();
    assert_eq!((report.kept, report.unmatched.len()), (1, 0));
}

fn repeating(conn: &Connection, clock: &mut Hlc, user: &str, title: &str, rrule: &str, first: &str) -> ActionSeries {
    let mut row = ActionSeries {
        id: aroh_domain::new_id(),
        user_id: user.into(),
        goal_id: "0190f2a0-0000-7000-8000-000000000091".into(),
        title: title.into(),
        notes: None,
        rrule: rrule.into(),
        dtstart_local: first.into(),
        timezone: BERLIN.name().into(),
        duration_minutes: 30,
        card: None,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut row).unwrap();
    row
}

fn on_day(conn: &Connection, series: &ActionSeries, day: u32) -> Vec<Action> {
    let actions: Vec<Action> = repo::list(conn, &[("series_id", series.id.as_str())]).unwrap();
    actions.into_iter().filter(|a| a.start_at.with_timezone(&BERLIN).date_naive() == date(day)).collect()
}

#[test]
fn tick_keeps_repeating_and_synced_actions_off_busy_days() {
    let (conn, mut clock, user) = setup();
    busy_event(&conn, &mut clock, &user, "conf", 30, 1);
    let weekly = repeating(&conn, &mut clock, &user, "Long run", "FREQ=WEEKLY", "2026-11-23T07:00:00");
    let daily = repeating(&conn, &mut clock, &user, "Piano", "FREQ=DAILY", "2026-11-20T20:00:00");
    let synced = action(&conn, &mut clock, &user, "Swim", local(30, 12, 0), 30, ActionStatus::Planned);
    let mut locked = action(&conn, &mut clock, &user, "Dentist", local(30, 15, 0), 30, ActionStatus::Planned);
    locked.locked = true;
    repo::put(&conn, &mut clock, &mut locked).unwrap();
    let done = action(&conn, &mut clock, &user, "Stretch", local(30, 6, 0), 30, ActionStatus::Done);

    let first = tick::tick(&conn, &mut clock, &user, now(), BERLIN).unwrap();
    assert_eq!(first.moved, 2, "the weekly run and the synced swim");
    assert!(on_day(&conn, &weekly, 30).is_empty());
    let run = repo::list::<Action>(&conn, &[("series_id", weekly.id.as_str())]).unwrap().into_iter().find(|a| a.occurrence == Some(local(30, 7, 0))).unwrap();
    assert_eq!(run.start_at, local(30, 7, 0) + Duration::days(1), "the weekly run moves to the next free day");
    assert_eq!(repo::get::<Action>(&conn, &synced.id).unwrap().start_at, local(30, 12, 0) + Duration::days(1));
    assert_eq!(on_day(&conn, &daily, 30).len(), 1, "a daily occurrence has no free day: its next days hold its own occurrences");
    assert_eq!(repo::get::<Action>(&conn, &locked.id).unwrap().start_at, local(30, 15, 0), "locked actions are never moved");
    assert_eq!(repo::get::<Action>(&conn, &done.id).unwrap().start_at, local(30, 6, 0));

    let again = tick::tick(&conn, &mut clock, &user, now(), BERLIN).unwrap();
    assert_eq!((again.materialized, again.moved), (0, 0));
    tick::tick(&conn, &mut clock, &user, now() + Duration::days(1), BERLIN).unwrap();
    assert!(on_day(&conn, &weekly, 30).is_empty(), "no copy is materialized on the Busy day");
    let live: Vec<Action> = repo::list(&conn, &[("user_id", user.as_str())]).unwrap();
    let mut starts: Vec<DateTime<Utc>> = live.iter().filter(|a| a.status == ActionStatus::Planned).map(|a| a.start_at).collect();
    let count = starts.len();
    starts.sort();
    starts.dedup();
    assert_eq!(starts.len(), count, "no two planned actions start at once");
    let (_, stuck) = write::keep_off_busy_days(&conn, &mut clock, &user, date(27), BERLIN).unwrap();
    assert_eq!(stuck, ["Piano"]);
}

#[test]
fn past_busy_days_stay_and_a_day_with_no_free_slot_is_reported() {
    let (conn, mut clock, user) = setup();
    busy_event(&conn, &mut clock, &user, "old", 25, 1);
    let past = action(&conn, &mut clock, &user, "Walk", local(25, 9, 0), 30, ActionStatus::Planned);
    let (from, to) = window();
    let start = local(27, 0, 0);
    let long = DeviceEvent { start_at: start, end_at: start + Duration::days(15), all_day: true, ..event("tour", "Tour", now()) };
    calendar::import(&conn, &mut clock, &user, &[long], from, to).unwrap();
    let run = action(&conn, &mut clock, &user, "Run", local(27, 9, 0), 60, ActionStatus::Planned);
    let tour = block_of(&conn, "tour");
    let set = write::set_day_kind(&conn, &mut clock, &user, &tour.id, DayKind::Busy, date(26), BERLIN).unwrap();
    assert_eq!(set.stuck, ["Run"]);
    assert!(set.moved.is_empty());
    assert_eq!(repo::get::<Action>(&conn, &run.id).unwrap().start_at, local(27, 9, 0));
    assert_eq!(repo::get::<Action>(&conn, &past.id).unwrap().start_at, local(25, 9, 0), "past days stay");
}

#[test]
fn import_of_a_busy_event_then_the_sweep_moves_that_days_actions() {
    let (conn, mut clock, user) = setup();
    let run = action(&conn, &mut clock, &user, "Run", local(28, 9, 0), 60, ActionStatus::Planned);
    busy_event(&conn, &mut clock, &user, "conf", 28, 1);
    assert_eq!(block_of(&conn, "conf").day_kind, Some(DayKind::Busy));
    let (moved, stuck) = write::keep_off_busy_days(&conn, &mut clock, &user, date(26), BERLIN).unwrap();
    assert_eq!((moved.len(), stuck.len()), (1, 0));
    assert_eq!(repo::get::<Action>(&conn, &run.id).unwrap().start_at, local(29, 9, 0));
}

#[test]
fn holiday_all_day_events_are_reminders_and_skip_triage() {
    let (conn, mut clock, user) = setup();
    let (from, to) = window();
    let mut feast = all_day("xmas", "Public holiday", 28, 1);
    feast.holiday = true;
    let mut party = event("party", "Holiday party", local(28, 19, 0));
    party.holiday = true;
    let report = calendar::import(&conn, &mut clock, &user, &[feast, party], from, to).unwrap();
    assert_eq!(report.unmatched.iter().map(|e| e.event_id.as_str()).collect::<Vec<_>>(), ["party"], "a timed holiday event still goes to triage");
    assert_eq!(block_of(&conn, "xmas").day_kind, Some(DayKind::Reminder));
    let set = write::set_day_kind(&conn, &mut clock, &user, &block_of(&conn, "xmas").id, DayKind::Away, date(26), BERLIN).unwrap();
    assert_eq!(set.rule.day_kind, Some(DayKind::Away));
    calendar::import(&conn, &mut clock, &user, &[DeviceEvent { holiday: true, ..all_day("xmas", "Public holiday", 28, 1) }], from, to).unwrap();
    assert_eq!(block_of(&conn, "xmas").day_kind, Some(DayKind::Away), "a chip tap wins over the default");
}

#[test]
fn the_fixed_time_switch_locks_and_unlocks_and_the_sweep_obeys_it() {
    let (conn, mut clock, user) = setup();
    let run = action(&conn, &mut clock, &user, "Run", local(28, 9, 0), 60, ActionStatus::Planned);
    let fixed = write::set_locked(&conn, &mut clock, &run.id, true).unwrap();
    assert!(fixed.locked);
    assert_eq!((fixed.start_at, fixed.end_at), (run.start_at, run.end_at), "the switch never moves the action");
    busy_event(&conn, &mut clock, &user, "conf", 28, 1);
    let (moved, stuck) = write::keep_off_busy_days(&conn, &mut clock, &user, date(26), BERLIN).unwrap();
    assert!(moved.is_empty());
    assert_eq!(stuck, Vec::<String>::new(), "a fixed action is not tried, so it is not stuck either");
    assert_eq!(repo::get::<Action>(&conn, &run.id).unwrap().start_at, local(28, 9, 0));
    assert!(!write::set_locked(&conn, &mut clock, &run.id, false).unwrap().locked);
    let (moved, _) = write::keep_off_busy_days(&conn, &mut clock, &user, date(26), BERLIN).unwrap();
    assert_eq!(moved.len(), 1, "a flexible action moves again");
    repo::delete::<Action>(&conn, &mut clock, &run.id).unwrap();
    assert!(write::set_locked(&conn, &mut clock, &run.id, true).is_err(), "a deleted action has no switch");
    assert!(write::set_locked(&conn, &mut clock, "0190f2a0-0000-7000-8000-0000000000ff", true).is_err());
}
