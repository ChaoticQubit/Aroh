use aroh_app::local;
use aroh_app::location::{self, Choice, Drain, NudgedIn, PlaceDecision, ResponseIn, VisitIn};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{Action, ActionSeries, ActionStatus, Autonomy, Goal, GoalStatus, Place, PlaceLink};
use aroh_domain::schema::SYNCED;
use aroh_store::{db, repo};
use chrono::{DateTime, Duration, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::Connection;

const ZONE: Tz = Tz::UTC;
const GYM: (f64, f64) = (37.776, -122.417);
const SHOP: (f64, f64) = (37.790, -122.400);

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 26, 12, 0, 0).unwrap()
}

fn setup() -> (Connection, Hlc, String, Goal) {
    let conn = db::open_in_memory().unwrap();
    let user = local::user_id(&conn).unwrap();
    let mut clock = Hlc::new("00000000000000d3");
    aroh_app::tick::tick(&conn, &mut clock, &user, now(), ZONE).unwrap();
    let fitness = aroh_domain::derived_id(&user, "health-fitness", 0);
    let mut goal = Goal {
        id: aroh_domain::new_id(),
        user_id: user.clone(),
        category_id: Some(fitness),
        title: "Six pack".into(),
        context: None,
        deadline: None,
        status: GoalStatus::Active,
        autonomy: Autonomy::Balanced,
        created_at: now(),
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(&conn, &mut clock, &mut goal).unwrap();
    (conn, clock, user, goal)
}

fn visit((lat, lon): (f64, f64), arrived: DateTime<Utc>, minutes: i64) -> VisitIn {
    VisitIn { lat, lon, accuracy_m: 25.0, arrived_at: arrived, left_at: Some(arrived + Duration::minutes(minutes)) }
}

fn days_ago(days: i64, hour: u32) -> DateTime<Utc> {
    let day = now() - Duration::days(days);
    Utc.with_ymd_and_hms(2026, 11, 1, hour, 0, 0).unwrap() + (day.date_naive() - chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap())
}

fn synced_rows(conn: &Connection) -> i64 {
    SYNCED.iter().map(|t| conn.query_row(&format!("SELECT count(*) FROM {}", t.name), [], |r| r.get::<_, i64>(0)).unwrap()).sum()
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn spans(conn: &Connection) -> Vec<(String, DateTime<Utc>, DateTime<Utc>)> {
    repo::list::<Action>(conn, &[]).unwrap().into_iter().map(|a| (a.id, a.start_at, a.end_at)).collect()
}

fn action(conn: &Connection, clock: &mut Hlc, goal: &Goal, title: &str, start: DateTime<Utc>, status: ActionStatus) -> Action {
    let mut a = Action {
        id: aroh_domain::new_id(),
        user_id: goal.user_id.clone(),
        goal_id: goal.id.clone(),
        series_id: None,
        occurrence: None,
        title: title.into(),
        notes: None,
        start_at: start,
        end_at: start + Duration::minutes(45),
        status,
        locked: false,
        done_at: (status == ActionStatus::Done).then_some(start + Duration::minutes(20)),
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut a).unwrap();
    a
}

fn gym_visits() -> Vec<VisitIn> {
    vec![visit(GYM, days_ago(5, 18), 60), visit(GYM, days_ago(3, 18), 55), visit((GYM.0 + 0.0004, GYM.1), days_ago(3, 20), 20), visit(GYM, days_ago(1, 18), 70)]
}

#[test]
fn drain_stores_visits_prunes_and_creates_one_candidate_a_day() {
    let (mut conn, mut clock, user, _) = setup();
    let mut batch = Drain { visits: gym_visits(), ..Drain::default() };
    batch.visits.extend([visit(SHOP, days_ago(6, 10), 30), visit(SHOP, days_ago(4, 10), 30), visit(SHOP, days_ago(2, 10), 30)]);
    batch.visits.push(visit(GYM, days_ago(100, 9), 60));
    let report = location::drain(&mut conn, &mut clock, &user, &batch, now(), ZONE).unwrap();
    assert_eq!((report.stored, report.pruned), (8, 1));
    assert_eq!(count(&conn, "SELECT count(*) FROM visits"), 7);
    assert_eq!(report.lookups.len(), 1, "one new place prompt a day");
    assert_eq!(count(&conn, "SELECT count(*) FROM place_candidates WHERE state = 'pending'"), 1);
    let again = location::drain(&mut conn, &mut clock, &user, &Drain::default(), now(), ZONE).unwrap();
    assert!(again.lookups.is_empty());
    let tomorrow = location::drain(&mut conn, &mut clock, &user, &Drain::default(), now() + Duration::days(1), ZONE).unwrap();
    assert_eq!(tomorrow.lookups.len(), 1, "the second spot asks the next day");
    assert_eq!(count(&conn, "SELECT count(*) FROM place_candidates WHERE state = 'pending'"), 2);

    local::set_setting(&conn, "location_paused", "true").unwrap();
    let paused = location::drain(&mut conn, &mut clock, &user, &Drain { visits: vec![visit(SHOP, now(), 30)], ..Drain::default() }, now(), ZONE).unwrap();
    assert_eq!(paused.stored, 0, "paused tracking stores no visits");
    assert_eq!(count(&conn, "SELECT count(*) FROM visits"), 7);
}

#[test]
fn deciding_prompts_links_renames_or_ignores_without_moving_anything() {
    let (mut conn, mut clock, user, goal) = setup();
    let due = action(&conn, &mut clock, &goal, "Strength session", now() + Duration::hours(6), ActionStatus::Planned);
    let before = spans(&conn);
    let report = location::drain(&mut conn, &mut clock, &user, &Drain { visits: gym_visits(), ..Drain::default() }, now(), ZONE).unwrap();
    let id = report.lookups[0].id.clone();
    location::set_lookup(&conn, &id, Some("Iron Works Gym"), Some("MKPOICategoryFitnessCenter"), Some("1200 Market St")).unwrap();
    let view = location::view(&conn, &user, now(), ZONE).unwrap();
    assert_eq!(view.prompts.len(), 1);
    let prompt = &view.prompts[0];
    assert_eq!((prompt.guess.as_deref(), prompt.name.as_str(), prompt.visits, prompt.weeks), (Some("gym"), "Iron Works Gym", 4, 1));
    assert!(prompt.usual.starts_with("usually"), "{}", prompt.usual);

    let rows = synced_rows(&conn);
    let place = location::decide_prompt(&conn, &mut clock, &user, &id, PlaceDecision::Link { goal_id: goal.id.clone(), series_id: None }, now()).unwrap().unwrap();
    assert_eq!(synced_rows(&conn), rows + 2, "one place and one link");
    assert_eq!((place.name.as_str(), place.radius_m), ("Iron Works Gym", 150));
    let links = repo::list::<PlaceLink>(&conn, &[]).unwrap();
    assert_eq!((links.len(), links[0].goal_id.as_str()), (1, goal.id.as_str()));
    assert_eq!(count(&conn, "SELECT count(*) FROM place_candidates"), 0);
    assert_eq!(count(&conn, &format!("SELECT count(*) FROM visits WHERE place_id = '{}'", place.id)), 4, "visits join the new place");
    assert!(location::decide_prompt(&conn, &mut clock, &user, &id, PlaceDecision::Ignore, now()).is_err(), "decided once");
    let nudge = location::watch(&conn, &user, now(), ZONE).unwrap().nudges.into_iter().find(|n| n.date == now().date_naive()).expect("a nudge today");
    assert_eq!(nudge.body, "Near your gym. Quick workout to stay on track?", "a linked prompt keeps its guessed kind");

    let shop = location::drain(&mut conn, &mut clock, &user, &Drain { visits: vec![visit(SHOP, days_ago(6, 10), 30), visit(SHOP, days_ago(4, 10), 30), visit(SHOP, days_ago(2, 10), 30)], ..Drain::default() }, now() + Duration::days(1), ZONE).unwrap();
    let rows = synced_rows(&conn);
    location::decide_prompt(&conn, &mut clock, &user, &shop.lookups[0].id, PlaceDecision::Ignore, now()).unwrap();
    assert_eq!(synced_rows(&conn), rows, "ignoring writes no synced row");
    let view = location::view(&conn, &user, now(), ZONE).unwrap();
    assert_eq!((view.prompts.len(), view.settings.ignored.len()), (0, 1));
    let again = location::drain(&mut conn, &mut clock, &user, &Drain { visits: vec![visit(SHOP, days_ago(0, 9), 30)], ..Drain::default() }, now() + Duration::days(2), ZONE).unwrap();
    assert!(again.lookups.is_empty(), "an ignored place never asks again");
    location::unignore(&conn, &view.settings.ignored[0].id).unwrap();
    assert_eq!(location::view(&conn, &user, now(), ZONE).unwrap().settings.ignored.len(), 0);

    let renamed = location::drain(&mut conn, &mut clock, &user, &Drain::default(), now() + Duration::days(3), ZONE).unwrap();
    let pool = location::decide_prompt(&conn, &mut clock, &user, &renamed.lookups[0].id, PlaceDecision::Rename { name: "Corner shop".into() }, now()).unwrap().unwrap();
    assert_eq!(pool.name, "Corner shop");
    assert_eq!(repo::list::<PlaceLink>(&conn, &[]).unwrap().len(), 1, "a renamed place has no link");
    assert_eq!(repo::get::<Action>(&conn, &due.id).unwrap().status, ActionStatus::Planned);
    assert_eq!(spans(&conn), before, "Q53: no action moved");
}

fn grocery_history(conn: &mut Connection, clock: &mut Hlc, user: &str) -> Place {
    let shop = location::add_place(conn, clock, user, "Trader Joe's", Some("555 9th St"), SHOP.0, SHOP.1).unwrap();
    let errands = Goal { id: aroh_domain::new_id(), title: "Errands".into(), ..repo::list::<Goal>(conn, &[]).unwrap().remove(0) };
    let mut errands = errands;
    errands.updated_at = String::new();
    repo::put(conn, clock, &mut errands).unwrap();
    let mut visits = Vec::new();
    for week in 1..=5 {
        let at = days_ago(7 * week, 10);
        visits.push(visit(SHOP, at, 45));
        action(conn, clock, &errands, "Groceries", at, ActionStatus::Done);
    }
    location::drain(conn, clock, user, &Drain { visits, ..Drain::default() }, now(), ZONE).unwrap();
    shop
}

#[test]
fn approving_a_pattern_writes_one_series_and_one_link_and_declining_writes_nothing() {
    let (mut conn, mut clock, user, _) = setup();
    grocery_history(&mut conn, &mut clock, &user);
    let view = location::view(&conn, &user, now(), ZONE).unwrap();
    assert_eq!(view.patterns.len(), 1);
    let pattern = &view.patterns[0];
    assert_eq!((pattern.title.as_str(), pattern.place.as_str(), pattern.gap_days, pattern.local_time.as_str()), ("Groceries", "Trader Joe's", 7, "10:00"));
    assert_eq!(pattern.goal, "Routines", "a place with no link files its routine under Routines");
    let rows = synced_rows(&conn);
    assert!(location::decide_pattern(&conn, &mut clock, &user, &pattern.id, false, now(), ZONE).unwrap().is_none());
    assert_eq!(synced_rows(&conn), rows, "declining writes nothing synced");
    location::drain(&mut conn, &mut clock, &user, &Drain::default(), now(), ZONE).unwrap();
    assert!(location::view(&conn, &user, now(), ZONE).unwrap().patterns.is_empty(), "declined, not proposed again for a while");

    let (mut conn, mut clock, user, _) = setup();
    grocery_history(&mut conn, &mut clock, &user);
    let before = spans(&conn);
    let id = location::view(&conn, &user, now(), ZONE).unwrap().patterns[0].id.clone();
    let series_before = repo::list::<ActionSeries>(&conn, &[]).unwrap().len();
    let series = location::decide_pattern(&conn, &mut clock, &user, &id, true, now(), ZONE).unwrap().unwrap();
    assert_eq!(repo::list::<ActionSeries>(&conn, &[]).unwrap().len(), series_before + 1);
    assert_eq!(series.goal_id, aroh_domain::seed::routines_id());
    assert_eq!((series.title.as_str(), series.duration_minutes), ("Groceries", 45));
    assert!(series.rrule.starts_with("FREQ=WEEKLY;BYDAY="), "{}", series.rrule);
    let routines: Goal = repo::get(&conn, &series.goal_id).unwrap();
    assert_eq!((routines.title.as_str(), routines.autonomy), ("Routines", Autonomy::Ask));
    let links: Vec<PlaceLink> = repo::list(&conn, &[("series_id", series.id.as_str())]).unwrap();
    assert_eq!(links.len(), 1);
    assert!(repo::list::<Action>(&conn, &[("series_id", series.id.as_str())]).unwrap().len() >= 2, "occurrences materialize");
    let kept: Vec<_> = spans(&conn).into_iter().filter(|s| before.iter().any(|b| b.0 == s.0)).collect();
    assert_eq!(kept, before, "Q53: no existing action moved");
    let view = location::view(&conn, &user, now(), ZONE).unwrap();
    assert!(view.patterns.is_empty());
    assert_eq!(view.routines.len(), 1);
    assert_eq!((view.routines[0].cadence.as_str(), view.routines[0].streak.as_str(), view.routines[0].place.as_deref()), (series_cadence(&series).as_str(), "0 weeks", Some("Trader Joe's")));
}

fn series_cadence(series: &ActionSeries) -> String {
    aroh_location::streak::cadence(series)
}

#[test]
fn clear_history_keeps_places_links_and_ignored_places() {
    let (mut conn, mut clock, user, goal) = setup();
    let report = location::drain(&mut conn, &mut clock, &user, &Drain { visits: gym_visits(), ..Drain::default() }, now(), ZONE).unwrap();
    location::decide_prompt(&conn, &mut clock, &user, &report.lookups[0].id, PlaceDecision::Link { goal_id: goal.id.clone(), series_id: None }, now()).unwrap();
    location::drain(&mut conn, &mut clock, &user, &Drain { visits: vec![visit(SHOP, days_ago(6, 10), 30), visit(SHOP, days_ago(4, 10), 30), visit(SHOP, days_ago(2, 10), 30)], ..Drain::default() }, now() + Duration::days(1), ZONE).unwrap();
    location::clear_history(&conn).unwrap();
    assert_eq!(count(&conn, "SELECT count(*) FROM visits"), 0);
    assert_eq!(count(&conn, "SELECT count(*) FROM place_candidates WHERE state = 'pending'"), 0);
    assert_eq!(repo::list::<Place>(&conn, &[]).unwrap().len(), 1);
    assert_eq!(repo::list::<PlaceLink>(&conn, &[]).unwrap().len(), 1);
    assert_eq!(location::view(&conn, &user, now(), ZONE).unwrap().log.len(), 0);
}

#[test]
fn nudge_plan_and_watched_set_come_from_stored_rows() {
    let (mut conn, mut clock, user, goal) = setup();
    let due = action(&conn, &mut clock, &goal, "Strength session", now() + Duration::hours(6), ActionStatus::Planned);
    let place = location::add_place(&mut conn, &mut clock, &user, "Iron Works Gym", None, GYM.0, GYM.1).unwrap();
    location::link_place(&conn, &mut clock, &user, &place.id, &goal.id, None).unwrap();
    let before = spans(&conn);
    let watch = location::watch(&conn, &user, now(), ZONE).unwrap();
    assert_eq!(watch.places.len(), 1);
    assert!((watch.places[0].lat - GYM.0).abs() < 1e-6 && watch.places[0].radius_m == 150);
    let today = watch.nudges.iter().find(|n| n.date == now().date_naive()).expect("a nudge today");
    assert_eq!((today.title.as_str(), today.action_id.as_str()), ("Strength session, due 18:00", due.id.as_str()));
    assert_eq!(today.body, "Near Iron Works Gym. Quick workout to stay on track?", "a searched place has no guessed kind");

    let batch = Drain { nudged: vec![NudgedIn { place_id: place.id.clone(), date: now().date_naive() }], ..Drain::default() };
    location::drain(&mut conn, &mut clock, &user, &batch, now(), ZONE).unwrap();
    assert!(location::watch(&conn, &user, now(), ZONE).unwrap().nudges.iter().all(|n| n.date != now().date_naive()), "one per place a day");

    let batch = Drain { responses: vec![ResponseIn { action_id: due.id.clone(), choice: Choice::Done }, ResponseIn { action_id: aroh_domain::new_id(), choice: Choice::Skip }], ..Drain::default() };
    assert_eq!(location::drain(&mut conn, &mut clock, &user, &batch, now(), ZONE).unwrap().statuses, 1);
    assert_eq!(repo::get::<Action>(&conn, &due.id).unwrap().status, ActionStatus::Done);
    assert_eq!(spans(&conn), before, "Q53: a nudge action changes status, never time");

    local::set_setting(&conn, "location_nudges", "false").unwrap();
    let off = location::watch(&conn, &user, now() + Duration::days(1), ZONE).unwrap();
    assert!(off.nudges.is_empty());
    assert_eq!(off.places.len(), 1, "places stay watched for visits");
    local::set_setting(&conn, "location_paused", "true").unwrap();
    assert!(location::watch(&conn, &user, now(), ZONE).unwrap().places.is_empty(), "paused watches nothing");
}
