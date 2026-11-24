//! Apple Health commands (Q21): connect (permission, then import) from Settings and the
//! start and resume import. HealthKit work runs in the Swift bridge; mapping, dedupe and
//! anchors run in `aroh_app::health`.

use super::with_core;
use crate::{notify, AppState};
use aroh_app::health::{self, HealthBatch, HealthType, ImportReport};
use aroh_app::local;
use chrono::{Duration, SecondsFormat, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::{AppHandle, Manager};
use tauri_plugin_aroh_ios::IosExt;

const ALL: [HealthType; 4] = [HealthType::BodyMass, HealthType::Workout, HealthType::Steps, HealthType::DietaryEnergy];
const FIRST_IMPORT_DAYS: i64 = 30;

#[derive(Serialize)]
struct AuthorizeArgs {
    read: Vec<HealthType>,
    share: Vec<HealthType>,
}

#[derive(Serialize)]
struct FetchArgs {
    types: Vec<HealthType>,
    anchors: BTreeMap<HealthType, String>,
    since: String,
}

async fn authorize(app: &AppHandle) -> Result<(), String> {
    let share = if cfg!(debug_assertions) { ALL.to_vec() } else { Vec::new() };
    app.ios().call::<serde_json::Value>("health_authorize", AuthorizeArgs { read: ALL.to_vec(), share }).await.map(|_| ())
}

async fn import(app: &AppHandle) -> Result<ImportReport, String> {
    let state = app.state::<AppState>();
    let (types, anchors) = with_core(&state, |c| Ok((health::wanted_types(&c.conn, &c.user_id)?, local::health_anchors(&c.conn)?)))?;
    if types.is_empty() {
        return Ok(ImportReport::default());
    }
    let since = (Utc::now() - Duration::days(FIRST_IMPORT_DAYS)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let batch: HealthBatch = app.ios().call("health_fetch", FetchArgs { types: types.into_iter().collect(), anchors, since }).await?;
    let report = with_core(&state, |c| health::import(&mut c.conn, &mut c.clock, &c.user_id, &batch))?;
    if report.written + report.deleted > 0 {
        notify::sync(app)?;
    }
    Ok(report)
}

/// Connect Apple Health: shows the HealthKit sheet (read access to body mass, workouts,
/// steps and dietary energy), turns the `health` setting on and imports. Returns the
/// status line Settings shows; errors are UI-safe strings.
#[tauri::command]
pub async fn app_health_connect(app: AppHandle) -> Result<String, String> {
    authorize(&app).await?;
    let state = app.state::<AppState>();
    with_core(&state, |c| local::set_setting(&c.conn, "health", "true"))?;
    let report = import(&app).await?;
    let any = with_core(&state, |c| health::wanted_types(&c.conn, &c.user_id))?;
    Ok(match (any.is_empty(), report.written) {
        (true, _) => "Connected. No tracker imports from Apple Health yet.".to_string(),
        (false, 0) => "Connected. Nothing new to import.".to_string(),
        (false, 1) => "Connected. Imported 1 entry.".to_string(),
        (false, n) => format!("Connected. Imported {n} entries."),
    })
}

/// Imports new Health samples in the background when the `health` setting is on, at start
/// and on resume (iPhone only). Failures are logged, never shown.
pub fn import_if_enabled(app: &AppHandle) {
    if cfg!(not(target_os = "ios")) {
        return;
    }
    let state = app.state::<AppState>();
    if with_core(&state, |c| local::setting(&c.conn, "health")).ok().flatten().as_deref() != Some("true") {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = import(&app).await {
            eprintln!("aroh: health import failed: {e}");
        }
    });
}

#[derive(Serialize)]
struct SeedSample {
    r#type: HealthType,
    value: f64,
    start: String,
    end: String,
    activity: Option<String>,
}

#[derive(Serialize)]
struct SeedArgs {
    samples: Vec<SeedSample>,
}

/// Debug builds only: asks for Health access (with write, which only debug requests),
/// adds an "Apple Health" tracker to the first goal and writes weight, steps and a run
/// to HealthKit, so a simulator check can import them through the real flow.
#[tauri::command]
pub async fn app_dev_health_seed(app: AppHandle) -> Result<(), String> {
    if !cfg!(debug_assertions) {
        return Err("debug builds only".into());
    }
    authorize(&app).await?;
    let state = app.state::<AppState>();
    with_core(&state, |c| {
        use aroh_domain::rows::{FieldKind, Goal, HealthMetric, Tracker, TrackerField};
        aroh_app::sample::seed(&c.conn, &mut c.clock, &c.user_id, Utc::now(), crate::zone())?;
        let goal = aroh_store::repo::list::<Goal>(&c.conn, &[("user_id", &c.user_id)])?.into_iter().next().ok_or_else(|| aroh_app::Error::Invalid("no goal".into()))?;
        let field = |key: &str, label: &str, kind: FieldKind, unit: Option<&str>, metric: HealthMetric| TrackerField {
            key: key.into(),
            label: label.into(),
            kind,
            unit: unit.map(Into::into),
            choices: vec![],
            health: Some(metric),
        };
        let mut tracker = Tracker {
            id: aroh_domain::new_id(),
            user_id: c.user_id.clone(),
            goal_id: goal.id,
            plugin_id: None,
            name: "Apple Health".into(),
            fields: vec![
                field("weight", "Weight", FieldKind::Number, Some("kg"), HealthMetric::BodyMass),
                field("steps", "Steps", FieldKind::Number, None, HealthMetric::Steps),
                field("minutes", "Workout", FieldKind::Number, Some("min"), HealthMetric::WorkoutMinutes),
                field("workout", "Workout type", FieldKind::Text, None, HealthMetric::WorkoutType),
            ],
            cadence: None,
            rules: vec![],
            updated_at: String::new(),
            deleted: false,
        };
        aroh_store::repo::put(&c.conn, &mut c.clock, &mut tracker)?;
        Ok(())
    })?;
    let now = Utc::now();
    let at = |hours: i64| (now - Duration::hours(hours)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let sample = |kind: HealthType, value: f64, from: i64, to: i64, activity: Option<&str>| SeedSample { r#type: kind, value, start: at(from), end: at(to), activity: activity.map(Into::into) };
    let samples = vec![
        sample(HealthType::BodyMass, 82.4, 26, 26, None),
        sample(HealthType::BodyMass, 81.9, 2, 2, None),
        sample(HealthType::Steps, 3200.0, 4, 3, None),
        sample(HealthType::Steps, 2150.0, 2, 1, None),
        sample(HealthType::Workout, 410.0, 3, 2, Some("Running")),
    ];
    app.ios().call::<serde_json::Value>("health_seed", SeedArgs { samples }).await?;
    Ok(())
}
