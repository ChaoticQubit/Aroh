use aroh_domain::rows::{Action, ActionSeries, ActionStatus, Autonomy, Goal, GoalStatus, Place, PlaceLink, PlaceSource};
use aroh_location::nudge::{fire_time, nudge_plan, NudgeIn, Reason};
use aroh_location::pattern::{detect_patterns, Declined};
use aroh_location::prompt::{guess, prompt_clusters};
use aroh_location::streak::{cadence, streak};
use aroh_location::watch::{choose_watched, MAX_WATCHED};
use aroh_location::Visit;
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use std::collections::{BTreeMap, BTreeSet};

const USER: &str = "0190f2a0-0000-7000-8000-00000000000a";
const ZONE: Tz = Tz::UTC;
const GYM: (i64, i64) = (37_776_000, -122_417_000);

fn id(n: u32) -> String {
    format!("0190f2a0-0000-7000-8000-{n:012x}")
}

fn at(day: u32, hour: u32, min: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, day, hour, min, 0).unwrap()
}

fn now() -> DateTime<Utc> {
    at(26, 12, 0)
}

fn visit(n: u32, (lat, lon): (i64, i64), arrived: DateTime<Utc>, minutes: i64, place: Option<&str>) -> Visit {
    Visit { id: id(1000 + n), lat_e6: lat, lon_e6: lon, arrived_at: arrived, left_at: Some(arrived + Duration::minutes(minutes)), place_id: place.map(str::to_string) }
}

fn place(n: u32, name: &str, (lat, lon): (i64, i64)) -> Place {
    Place { id: id(n), user_id: USER.into(), name: name.into(), address: None, lat_e6: lat, lon_e6: lon, radius_m: 150, source: PlaceSource::Prompt, updated_at: String::new(), deleted: false }
}

fn link(n: u32, place: &Place, goal: &str, series: Option<&str>) -> PlaceLink {
    PlaceLink { id: id(n), user_id: USER.into(), place_id: place.id.clone(), goal_id: goal.into(), series_id: series.map(str::to_string), updated_at: String::new(), deleted: false }
}

fn goal(n: u32, title: &str, category: &str) -> Goal {
    Goal {
        id: id(n),
        user_id: USER.into(),
        category_id: Some(aroh_domain::derived_id(USER, category, 0)),
        title: title.into(),
        context: None,
        deadline: None,
        status: GoalStatus::Active,
        autonomy: Autonomy::Balanced,
        created_at: at(1, 0, 0),
        reminders: true,
        use_calendars: true,
        run_limit_minutes: None,
        updated_at: String::new(),
        deleted: false,
    }
}

fn action(n: u32, goal: &str, title: &str, start: DateTime<Utc>, status: ActionStatus) -> Action {
    Action {
        id: id(n),
        user_id: USER.into(),
        goal_id: goal.into(),
        series_id: None,
        occurrence: None,
        title: title.into(),
        notes: None,
        start_at: start,
        end_at: start + Duration::minutes(60),
        status,
        locked: false,
        done_at: (status == ActionStatus::Done).then_some(start + Duration::minutes(30)),
        updated_at: String::new(),
        deleted: false,
    }
}

fn north(meters: i64) -> (i64, i64) {
    (GYM.0 + meters * 9, GYM.1)
}

#[test]
fn prompt_after_three_distinct_days_within_150_m_and_not_before() {
    let two = [visit(1, GYM, at(23, 18, 0), 60, None), visit(2, north(60), at(24, 18, 0), 60, None), visit(3, north(40), at(24, 20, 0), 30, None)];
    assert!(prompt_clusters(&two, &[], now(), ZONE).is_empty(), "two distinct days are not enough");
    let mut three = two.to_vec();
    three.push(visit(4, north(-50), at(25, 18, 5), 70, None));
    let clusters = prompt_clusters(&three, &[], now(), ZONE);
    assert_eq!(clusters.len(), 1);
    assert_eq!((clusters[0].visits, clusters[0].days.len()), (4, 3));
    let far = [visit(5, north(400), at(21, 9, 0), 60, None)];
    let mixed: Vec<Visit> = three.iter().cloned().chain(far).collect();
    assert_eq!(prompt_clusters(&mixed, &[], now(), ZONE).len(), 1, "a visit 400 m away is another cluster");
    let old = [visit(6, GYM, at(1, 18, 0), 60, None), visit(7, GYM, at(2, 18, 0), 60, None), visit(8, GYM, at(3, 18, 0), 60, None)];
    assert!(prompt_clusters(&old, &[], now(), ZONE).is_empty(), "visits older than 21 days do not prompt");
}

#[test]
fn named_and_ignored_places_never_prompt_and_passing_by_is_ignored() {
    let visits = [visit(1, GYM, at(23, 18, 0), 60, None), visit(2, GYM, at(24, 18, 0), 60, None), visit(3, GYM, at(25, 18, 0), 60, None)];
    assert!(prompt_clusters(&visits, &[north(80)], now(), ZONE).is_empty(), "near a named or ignored place");
    let passing = [visit(1, GYM, at(23, 18, 0), 2, None), visit(2, GYM, at(24, 18, 0), 3, None), visit(3, GYM, at(25, 18, 0), 1, None)];
    assert!(prompt_clusters(&passing, &[], now(), ZONE).is_empty(), "stays under 5 minutes are passing by");
    assert_eq!(guess(Some("MKPOICategoryFitnessCenter")), Some("gym"));
    assert_eq!(guess(Some("MKPOICategoryFoodMarket")), Some("grocery store"));
    assert_eq!(guess(Some("MKPOICategoryBank")), None);
    assert_eq!(guess(None), None);
}

struct World {
    gym: Place,
    links: Vec<PlaceLink>,
    goals: Vec<Goal>,
}

fn world() -> World {
    let fit = goal(50, "Six pack", "health-fitness");
    let gym = place(60, "Iron Works Gym", GYM);
    let links = vec![link(61, &gym, &fit.id, None)];
    World { gym, links, goals: vec![fit] }
}

fn plan_for(w: &World, actions: &[Action], sent: &BTreeSet<(String, NaiveDate)>, paused: bool) -> Vec<aroh_location::nudge::Nudge> {
    plan_kinds(w, actions, sent, paused, &BTreeMap::new())
}

fn plan_kinds(w: &World, actions: &[Action], sent: &BTreeSet<(String, NaiveDate)>, paused: bool, kinds: &BTreeMap<String, String>) -> Vec<aroh_location::nudge::Nudge> {
    let places = [w.gym.clone()];
    nudge_plan(&NudgeIn { places: &places, links: &w.links, goals: &w.goals, actions, sent, kinds, paused }, now(), ZONE)
}

#[test]
fn nudge_due_today_missed_yesterday_done_quiet_sent_and_paused() {
    let w = world();
    let goal_id = w.goals[0].id.clone();
    let none = BTreeSet::new();
    let due = [action(70, &goal_id, "Strength session", at(26, 18, 0), ActionStatus::Planned)];
    let plan = plan_for(&w, &due, &none, false);
    let today = plan.iter().find(|n| n.date == now().date_naive()).expect("a nudge today");
    assert_eq!((today.reason, today.action_id.as_str()), (Reason::Due, due[0].id.as_str()));
    assert_eq!(today.title, "Strength session, due 18:00");
    assert_eq!(today.body, "Near Iron Works Gym. Quick workout to stay on track?", "no guessed kind: the place name");
    let kinds: BTreeMap<String, String> = [(w.gym.id.clone(), "gym".to_string())].into();
    let guessed = plan_kinds(&w, &due, &none, false, &kinds);
    let guessed = guessed.iter().find(|n| n.date == now().date_naive()).expect("a nudge today");
    assert_eq!(guessed.body, "Near your gym. Quick workout to stay on track?", "the guessed kind when known");

    let missed = [action(71, &goal_id, "Strength session", at(25, 18, 0), ActionStatus::Missed)];
    let plan = plan_for(&w, &missed, &none, false);
    let today = plan.iter().find(|n| n.date == now().date_naive()).expect("a missed nudge today");
    assert_eq!((today.reason, today.title.as_str()), (Reason::Missed, "Strength session, missed Wed"));

    let done = [action(72, &goal_id, "Strength session", at(26, 18, 0), ActionStatus::Done)];
    assert!(plan_for(&w, &done, &none, false).iter().all(|n| n.date != now().date_naive()), "done means no nudge");

    let entered_late = at(26, 21, 59);
    assert_eq!(fire_time(today, at(26, 19, 0)), Some(at(26, 19, 2)));
    assert_eq!(fire_time(today, entered_late), None, "the nudge would fire after 22:00");
    assert_eq!(fire_time(today, at(26, 5, 0)), None, "quiet hours before 06:00");
    assert_eq!(fire_time(today, at(27, 9, 0)), None, "another day");

    let sent: BTreeSet<(String, NaiveDate)> = [(w.gym.id.clone(), now().date_naive())].into();
    assert!(plan_for(&w, &due, &sent, false).iter().all(|n| n.date != now().date_naive()), "one nudge per place a day");
    assert!(plan_for(&w, &due, &none, true).is_empty(), "paused");

    let piano = goal(51, "Learn piano", "hobby-skill");
    let studio = place(62, "Piano studio", north(2000));
    let w2 = World { gym: studio.clone(), links: vec![link(63, &studio, &piano.id, None)], goals: vec![piano.clone()] };
    let practice = [action(73, &piano.id, "Piano practice", at(26, 20, 0), ActionStatus::Planned)];
    let n = plan_for(&w2, &practice, &none, false).remove(0);
    assert_eq!(n.body, "Near Piano studio. Good time for Piano practice?");
}

#[test]
fn watched_set_caps_at_20_and_orders_by_soonest_linked_action() {
    let (a, b, c) = (id(51), id(52), id(53));
    let places: Vec<Place> = (0..25).map(|i| place(100 + i, &format!("Place {i}"), north(i as i64 * 500))).collect();
    let links = vec![link(200, &places[7], &a, None), link(201, &places[3], &b, None), link(202, &places[9], &c, Some(&id(999)))];
    let actions = [
        action(300, &b, "Later", at(28, 9, 0), ActionStatus::Planned),
        action(301, &a, "Soon", at(26, 15, 0), ActionStatus::Planned),
        action(302, &c, "Not the linked series", at(26, 13, 0), ActionStatus::Planned),
        action(303, &a, "Too far out", at(26, 15, 0) + Duration::days(9), ActionStatus::Planned),
    ];
    let visits: BTreeMap<String, usize> = [(places[20].id.clone(), 9)].into();
    let watched = choose_watched(&places, &links, &actions, &visits, now());
    assert_eq!(watched.len(), MAX_WATCHED);
    assert_eq!(&watched[..4], &[places[7].id.clone(), places[3].id.clone(), places[9].id.clone(), places[20].id.clone()]);
    assert_eq!(choose_watched(&places[..5], &links, &actions, &visits, now()).len(), 5);
}

fn saturdays(count: u32, gap_days: i64, jitter: &[i64]) -> (Vec<Visit>, Vec<Action>) {
    let tj = id(80);
    let goal_id = id(81);
    let first = at(26, 10, 0) - Duration::days(gap_days * count as i64 + 1);
    let mut visits = Vec::new();
    let mut actions = Vec::new();
    for i in 0..count {
        let day = first + Duration::days(gap_days * i as i64 + jitter.get(i as usize).copied().unwrap_or(0));
        visits.push(visit(i, (1, 1), day, 45, Some(&tj)));
        let mut a = action(400 + i, &goal_id, if i % 2 == 0 { "Groceries" } else { " groceries " }, day, ActionStatus::Done);
        a.done_at = Some(day + Duration::minutes(40));
        actions.push(a);
    }
    (visits, actions)
}

#[test]
fn pattern_for_five_saturday_grocery_runs_and_not_for_bad_evidence() {
    let (visits, actions) = saturdays(5, 7, &[]);
    let found = detect_patterns(&visits, &actions, &[], &[], now(), ZONE);
    assert_eq!(found.len(), 1);
    let p = &found[0];
    assert_eq!((p.place_id.as_str(), p.key.as_str(), p.gap_days, p.evidence), (id(80).as_str(), "groceries", 7, 5));
    assert_eq!(p.weekday, Some(visits[0].arrived_at.with_timezone(&ZONE).weekday()));
    assert_eq!(p.local_time.to_string(), "10:00:00");
    assert_eq!(p.duration_minutes, 60);

    let (visits, actions) = saturdays(5, 7, &[0, 3, -2, 6, 1]);
    assert!(detect_patterns(&visits, &actions, &[], &[], now(), ZONE).is_empty(), "irregular gaps");
    let (visits, actions) = saturdays(3, 7, &[]);
    assert!(detect_patterns(&visits, &actions, &[], &[], now(), ZONE).is_empty(), "three occurrences are not enough");

    let (visits, actions) = saturdays(5, 7, &[]);
    let declined = [Declined { place_id: id(80), key: "groceries".into(), until: now() + Duration::days(30) }];
    assert!(detect_patterns(&visits, &actions, &[], &declined, now(), ZONE).is_empty(), "declined for a while");
    let expired = [Declined { place_id: id(80), key: "groceries".into(), until: now() - Duration::days(1) }];
    assert_eq!(detect_patterns(&visits, &actions, &[], &expired, now(), ZONE).len(), 1);
    let series = [series(90, "Groceries", "FREQ=WEEKLY;BYDAY=SA")];
    assert!(detect_patterns(&visits, &actions, &series, &[], now(), ZONE).is_empty(), "a series already exists");
    let mut away = visits.clone();
    for v in &mut away {
        v.arrived_at -= Duration::hours(5);
        v.left_at = v.left_at.map(|l| l - Duration::hours(5));
    }
    assert!(detect_patterns(&away, &actions, &[], &[], now(), ZONE).is_empty(), "done away from the place");
}

fn series(n: u32, title: &str, rrule: &str) -> ActionSeries {
    ActionSeries {
        id: id(n),
        user_id: USER.into(),
        goal_id: id(81),
        title: title.into(),
        notes: None,
        rrule: rrule.into(),
        dtstart_local: "2026-10-01T10:00:00".into(),
        timezone: "UTC".into(),
        duration_minutes: 45,
        updated_at: String::new(),
        deleted: false,
    }
}

fn occurrence(n: u32, s: &ActionSeries, start: DateTime<Utc>, status: ActionStatus) -> Action {
    let mut a = action(n, &s.goal_id, &s.title, start, status);
    a.series_id = Some(s.id.clone());
    a.occurrence = Some(start);
    a
}

#[test]
fn streak_units_follow_the_cadence() {
    let weekly = series(90, "Groceries", "FREQ=WEEKLY;BYDAY=SA");
    let mut actions: Vec<Action> = (0..6).map(|i| occurrence(500 + i, &weekly, at(26, 10, 0) - Duration::weeks(6 - i as i64), ActionStatus::Done)).collect();
    actions[1].status = ActionStatus::Missed;
    actions[1].done_at = None;
    actions.push(occurrence(510, &weekly, at(28, 10, 0), ActionStatus::Planned));
    assert_eq!(streak(&weekly, &actions, now()).label, "4 weeks");
    assert_eq!(cadence(&weekly), "Weekly, Sat");

    let daily = series(91, "Morning walk", "FREQ=DAILY");
    let walks: Vec<Action> = (1..=12).map(|i| occurrence(600 + i, &daily, at(26, 7, 0) - Duration::days(12 - i as i64), ActionStatus::Done)).collect();
    let s = streak(&daily, &walks, now());
    assert_eq!((s.count, s.label.as_str()), (12, "12 days"));
    assert_eq!(cadence(&daily), "Daily");

    let every = series(92, "Return books", "FREQ=WEEKLY;INTERVAL=3");
    let books = [occurrence(700, &every, at(5, 17, 0), ActionStatus::Done)];
    assert_eq!(streak(&every, &books, now()).label, "1 time");
    assert_eq!(cadence(&every), "Every 3 weeks");
    assert_eq!(streak(&every, &[], now()).label, "0 times");
    assert_eq!(Weekday::Sat.to_string(), "Sat");
}
