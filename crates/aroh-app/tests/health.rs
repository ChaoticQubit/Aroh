use aroh_app::health::{self, HealthBatch, HealthType};
use aroh_app::local;
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{EntrySource, FieldKind, Goal, GoalStatus, HealthMetric, Tracker, TrackerEntry, TrackerField};
use aroh_store::{db, repo};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::Connection;
use serde_json::json;
use std::collections::BTreeSet;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 21, 9, 0, 0).unwrap()
}

fn field(key: &str, kind: FieldKind, unit: Option<&str>, health: Option<HealthMetric>) -> TrackerField {
    TrackerField { key: key.into(), label: key.into(), kind, unit: unit.map(Into::into), choices: vec![], health }
}

fn setup() -> (Connection, Hlc, String, Goal) {
    let conn = db::open_in_memory().unwrap();
    let user = local::user_id(&conn).unwrap();
    let mut clock = Hlc::new("00000000000000d2");
    let mut goal = Goal {
        id: aroh_domain::new_id(),
        user_id: user.clone(),
        category_id: None,
        title: "Fit by spring".into(),
        context: None,
        deadline: None,
        status: GoalStatus::Active,
        autonomy: aroh_domain::rows::Autonomy::Balanced,
        created_at: now(),
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(&conn, &mut clock, &mut goal).unwrap();
    (conn, clock, user, goal)
}

fn tracker(conn: &Connection, clock: &mut Hlc, user: &str, goal: &Goal, name: &str, fields: Vec<TrackerField>) -> Tracker {
    let mut t = Tracker {
        id: aroh_domain::new_id(),
        user_id: user.into(),
        goal_id: goal.id.clone(),
        plugin_id: None,
        name: name.into(),
        fields,
        cadence: None,
        rules: vec![],
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut t).unwrap();
    t
}

fn batch(value: serde_json::Value) -> HealthBatch {
    serde_json::from_value(value).unwrap()
}

fn entries(conn: &Connection, tracker: &Tracker) -> Vec<TrackerEntry> {
    repo::list::<TrackerEntry>(conn, &[("tracker_id", tracker.id.as_str())]).unwrap()
}

fn weight_batch() -> HealthBatch {
    batch(json!({
        "body_mass": {"anchor": "YW5jaG9yLTE=", "samples": [
            {"uuid": "6F9619FF-8B86-D011-B42D-00C04FC964FF", "start": "2026-11-20T07:00:00Z", "end": "2026-11-20T07:00:00Z", "value": 82.4},
            {"uuid": "7F9619FF-8B86-D011-B42D-00C04FC964FF", "start": "2026-11-21T07:00:00Z", "end": "2026-11-21T07:00:00Z", "value": 82.0}
        ]}
    }))
}

#[test]
fn body_mass_maps_to_kg_and_lb_fields() {
    let (mut conn, mut clock, user, goal) = setup();
    let kg = tracker(&conn, &mut clock, &user, &goal, "Weight", vec![field("weight", FieldKind::Number, Some("kg"), Some(HealthMetric::BodyMass))]);
    let lb = tracker(&conn, &mut clock, &user, &goal, "Weight lb", vec![field("lb", FieldKind::Number, Some("lb"), Some(HealthMetric::BodyMass))]);
    let report = health::import(&mut conn, &mut clock, &user, &weight_batch()).unwrap();
    assert_eq!(report.written, 4);
    let kg_rows = entries(&conn, &kg);
    assert_eq!(kg_rows.len(), 2);
    assert!(kg_rows.iter().all(|e| e.source == EntrySource::Health && e.user_id == user));
    let first = kg_rows.iter().find(|e| e.logged_at == Utc.with_ymd_and_hms(2026, 11, 20, 7, 0, 0).unwrap()).unwrap();
    assert_eq!(first.data["weight"], json!(82.4));
    let lb_rows = entries(&conn, &lb);
    let lb_first = lb_rows.iter().find(|e| e.logged_at == first.logged_at).unwrap();
    assert_eq!(lb_first.data["lb"], json!(181.66));
}

#[test]
fn importing_the_same_samples_twice_writes_once() {
    let (mut conn, mut clock, user, goal) = setup();
    let t = tracker(&conn, &mut clock, &user, &goal, "Weight", vec![field("weight", FieldKind::Number, Some("kg"), Some(HealthMetric::BodyMass))]);
    assert_eq!(health::import(&mut conn, &mut clock, &user, &weight_batch()).unwrap().written, 2);
    let again = health::import(&mut conn, &mut clock, &user, &weight_batch()).unwrap();
    assert_eq!((again.written, again.unchanged), (0, 2));
    assert_eq!(entries(&conn, &t).len(), 2);
    let ids: BTreeSet<String> = entries(&conn, &t).into_iter().map(|e| e.id).collect();
    assert!(ids.contains(&aroh_domain::derived_id(&t.id, "health:6F9619FF-8B86-D011-B42D-00C04FC964FF", 0)));
}

#[test]
fn workouts_fill_minutes_energy_and_type() {
    let (mut conn, mut clock, user, goal) = setup();
    let t = tracker(
        &conn,
        &mut clock,
        &user,
        &goal,
        "Workouts",
        vec![
            field("minutes", FieldKind::Number, Some("min"), Some(HealthMetric::WorkoutMinutes)),
            field("kcal", FieldKind::Number, Some("kcal"), Some(HealthMetric::WorkoutEnergy)),
            field("kind", FieldKind::Text, None, Some(HealthMetric::WorkoutType)),
            field("felt", FieldKind::Text, None, None),
        ],
    );
    let b = batch(json!({"workout": {"anchor": "YQ==", "samples": [
        {"uuid": "A1", "start": "2026-11-21T06:00:00Z", "end": "2026-11-21T06:45:00Z", "energy_kcal": 412.37, "activity": "Running"}
    ]}}));
    health::import(&mut conn, &mut clock, &user, &b).unwrap();
    let rows = entries(&conn, &t);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].data, json!({"minutes": 45.0, "kcal": 412.4, "kind": "Running"}).as_object().unwrap().clone());
}

#[test]
fn day_totals_update_in_place_and_deleted_samples_soft_delete() {
    let (mut conn, mut clock, user, goal) = setup();
    let steps = tracker(&conn, &mut clock, &user, &goal, "Steps", vec![field("steps", FieldKind::Number, None, Some(HealthMetric::Steps))]);
    let weight = tracker(&conn, &mut clock, &user, &goal, "Weight", vec![field("weight", FieldKind::Number, Some("kg"), Some(HealthMetric::BodyMass))]);
    let day = |total: f64| batch(json!({"steps": {"anchor": "cw==", "days": [{"date": "2026-11-21", "start": "2026-11-20T23:00:00Z", "total": total}]}}));
    health::import(&mut conn, &mut clock, &user, &day(4200.0)).unwrap();
    let report = health::import(&mut conn, &mut clock, &user, &day(8150.4)).unwrap();
    assert_eq!(report.written, 1);
    let rows = entries(&conn, &steps);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].data["steps"], json!(8150.0));

    health::import(&mut conn, &mut clock, &user, &weight_batch()).unwrap();
    let gone = batch(json!({"body_mass": {"anchor": "Yg==", "deleted": ["6F9619FF-8B86-D011-B42D-00C04FC964FF"]}}));
    assert_eq!(health::import(&mut conn, &mut clock, &user, &gone).unwrap().deleted, 1);
    assert_eq!(entries(&conn, &weight).len(), 1);
    let back = health::import(&mut conn, &mut clock, &user, &weight_batch()).unwrap();
    assert_eq!(back.written, 0);
    assert_eq!(entries(&conn, &weight).len(), 1);
}

#[test]
fn anchors_are_stored_with_the_entries_and_bad_samples_are_skipped() {
    let (mut conn, mut clock, user, goal) = setup();
    assert!(local::health_anchors(&conn).unwrap().is_empty());
    tracker(&conn, &mut clock, &user, &goal, "Weight", vec![field("weight", FieldKind::Number, Some("kg"), Some(HealthMetric::BodyMass))]);
    health::import(&mut conn, &mut clock, &user, &weight_batch()).unwrap();
    assert_eq!(local::health_anchors(&conn).unwrap().get(&HealthType::BodyMass).map(String::as_str), Some("YW5jaG9yLTE="));
    let bad = batch(json!({"body_mass": {"anchor": "Yw==", "samples": [
        {"uuid": "B1", "start": "2026-11-21T08:00:00Z", "end": "2026-11-21T08:00:00Z"}
    ]}}));
    let report = health::import(&mut conn, &mut clock, &user, &bad).unwrap();
    assert_eq!((report.written, report.skipped), (0, 1));
    assert_eq!(local::health_anchors(&conn).unwrap()[&HealthType::BodyMass], "Yw==");
}

#[test]
fn only_declared_types_are_wanted_and_undeclared_trackers_get_nothing() {
    let (mut conn, mut clock, user, goal) = setup();
    let plain = tracker(&conn, &mut clock, &user, &goal, "Mood", vec![field("mood", FieldKind::Number, None, None)]);
    assert!(health::wanted_types(&conn, &user).unwrap().is_empty());
    tracker(&conn, &mut clock, &user, &goal, "Food", vec![field("kcal", FieldKind::Number, Some("kcal"), Some(HealthMetric::DietaryEnergy))]);
    tracker(&conn, &mut clock, &user, &goal, "Runs", vec![field("kind", FieldKind::Text, None, Some(HealthMetric::WorkoutType))]);
    let wanted = health::wanted_types(&conn, &user).unwrap();
    assert_eq!(wanted, BTreeSet::from([HealthType::Workout, HealthType::DietaryEnergy]));
    health::import(&mut conn, &mut clock, &user, &weight_batch()).unwrap();
    assert!(entries(&conn, &plain).is_empty());
}
