//! Apple Health import (Q21): maps the HealthKit samples the iOS plugin returns into
//! tracker entries for every field that declares a health source. Entry ids derive from
//! the tracker and the sample uuid (or the local day), so no sample is imported twice.

use crate::{local, Result};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{validate_entry, EntrySource, HealthMetric, Tracker, TrackerEntry, TrackerField};
use aroh_store::repo;
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// A HealthKit type the plugin queries; several metrics can come from one type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthType {
    BodyMass,
    Workout,
    Steps,
    DietaryEnergy,
}

impl HealthType {
    /// The wire name of the type, as serde writes it (`body_mass`, `workout`, ...).
    /// Used in derived entry ids, so it must never change.
    pub fn name(self) -> &'static str {
        match self {
            HealthType::BodyMass => "body_mass",
            HealthType::Workout => "workout",
            HealthType::Steps => "steps",
            HealthType::DietaryEnergy => "dietary_energy",
        }
    }
}

/// One discrete HealthKit sample: body mass `value` in kg, or a workout with its
/// energy in kcal and activity name. Times are the sample's start and end.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub uuid: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub energy_kcal: Option<f64>,
    #[serde(default)]
    pub activity: Option<String>,
}

/// The cumulative total of one local day (`date`, starting at instant `start`): steps, or kcal eaten.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DayTotal {
    pub date: NaiveDate,
    pub start: DateTime<Utc>,
    pub total: f64,
}

/// What one anchored query returned: the new anchor, new samples, uuids of deleted samples
/// and, for cumulative types, the recomputed totals of every day that changed.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeBatch {
    #[serde(default)]
    pub anchor: Option<String>,
    #[serde(default)]
    pub samples: Vec<Sample>,
    #[serde(default)]
    pub deleted: Vec<String>,
    #[serde(default)]
    pub days: Vec<DayTotal>,
}

/// One fetch from the iOS plugin, keyed by HealthKit type.
pub type HealthBatch = BTreeMap<HealthType, TypeBatch>;

/// Counts of one import: entries written, tombstoned, already up to date, and samples
/// skipped because they carried no usable value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub written: usize,
    pub deleted: usize,
    pub unchanged: usize,
    pub skipped: usize,
}

const UUID_MAX: usize = 64;
const ANCHOR_MAX: usize = 64 * 1024;
const TEXT_MAX: usize = 200;

fn source_type(metric: HealthMetric) -> HealthType {
    match metric {
        HealthMetric::BodyMass => HealthType::BodyMass,
        HealthMetric::WorkoutMinutes | HealthMetric::WorkoutEnergy | HealthMetric::WorkoutType => HealthType::Workout,
        HealthMetric::Steps => HealthType::Steps,
        HealthMetric::DietaryEnergy => HealthType::DietaryEnergy,
    }
}

fn round(value: f64, places: i32) -> f64 {
    let scale = 10f64.powi(places);
    (value * scale).round() / scale
}

fn number(value: Option<f64>, places: i32) -> Option<Value> {
    value.filter(|v| v.is_finite() && *v >= 0.0).map(|v| Value::from(round(v, places)))
}

fn sample_value(field: &TrackerField, metric: HealthMetric, sample: &Sample) -> Option<Value> {
    match metric {
        HealthMetric::BodyMass => {
            let per_kg = match field.unit.as_deref().map(str::to_ascii_lowercase).as_deref() {
                Some("lb") | Some("lbs") => 2.204_622_62,
                Some("g") => 1000.0,
                _ => 1.0,
            };
            number(sample.value.map(|kg| kg * per_kg), 2)
        }
        HealthMetric::WorkoutMinutes => number(Some((sample.end - sample.start).num_seconds() as f64 / 60.0), 1),
        HealthMetric::WorkoutEnergy => number(sample.energy_kcal, 1),
        HealthMetric::WorkoutType => sample.activity.as_ref().filter(|a| !a.trim().is_empty()).map(|a| Value::from(a.chars().take(TEXT_MAX).collect::<String>())),
        HealthMetric::Steps | HealthMetric::DietaryEnergy => None,
    }
}

/// An entry the import wants to exist (or to be tombstoned) after it ran.
#[derive(Debug, Clone, PartialEq)]
enum Change {
    Upsert { id: String, tracker: usize, data: Map<String, Value>, logged_at: DateTime<Utc> },
    Delete { id: String },
}

fn health_fields(tracker: &Tracker, kind: HealthType) -> Vec<(&TrackerField, HealthMetric)> {
    tracker.fields.iter().filter_map(|f| f.health.filter(|m| source_type(*m) == kind).map(|m| (f, m))).collect()
}

fn valid_uuid(uuid: &str) -> bool {
    !uuid.is_empty() && uuid.len() <= UUID_MAX && uuid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn plan(trackers: &[Tracker], batch: &HealthBatch) -> (Vec<Change>, usize) {
    let mut changes = Vec::new();
    let mut skipped = 0;
    for (index, tracker) in trackers.iter().enumerate() {
        for (kind, part) in batch {
            let fields = health_fields(tracker, *kind);
            if fields.is_empty() {
                continue;
            }
            for sample in &part.samples {
                let data: Map<String, Value> = fields.iter().filter_map(|(f, m)| sample_value(f, *m, sample).map(|v| (f.key.clone(), v))).collect();
                if data.is_empty() || !valid_uuid(&sample.uuid) || sample.end < sample.start {
                    skipped += 1;
                    continue;
                }
                let id = aroh_domain::derived_id(&tracker.id, &format!("health:{}", sample.uuid), 0);
                changes.push(Change::Upsert { id, tracker: index, data, logged_at: sample.start });
            }
            for day in &part.days {
                let data: Map<String, Value> = fields.iter().filter_map(|(f, _)| number(Some(day.total), 0).map(|v| (f.key.clone(), v))).collect();
                if data.is_empty() {
                    skipped += 1;
                    continue;
                }
                let key = format!("health:{}:{}", kind.name(), day.date);
                let id = aroh_domain::derived_id(&tracker.id, &key, 0);
                changes.push(Change::Upsert { id, tracker: index, data, logged_at: day.start });
            }
            for uuid in part.deleted.iter().filter(|u| valid_uuid(u)) {
                changes.push(Change::Delete { id: aroh_domain::derived_id(&tracker.id, &format!("health:{uuid}"), 0) });
            }
        }
    }
    (changes, skipped)
}

/// The HealthKit types some live tracker of `user_id` declares, so the plugin queries
/// and asks permission only for what is used.
pub fn wanted_types(conn: &Connection, user_id: &str) -> Result<BTreeSet<HealthType>> {
    let trackers = repo::list::<Tracker>(conn, &[("user_id", user_id)])?;
    Ok(trackers.iter().flat_map(|t| t.fields.iter().filter_map(|f| f.health.map(source_type))).collect())
}

enum Stored {
    Missing,
    Tombstone,
    Live(TrackerEntry),
}

fn stored(conn: &Connection, id: &str) -> Result<Stored> {
    let deleted: Option<bool> = conn.query_row("SELECT deleted FROM tracker_entries WHERE id = ?1", [id], |r| r.get(0)).optional()?;
    Ok(match deleted {
        None => Stored::Missing,
        Some(true) => Stored::Tombstone,
        Some(false) => Stored::Live(repo::get::<TrackerEntry>(conn, id)?),
    })
}

/// Imports one plugin fetch for `user_id` in a single transaction: writes an entry per
/// sample (or day) and declaring tracker, tombstones deleted samples, leaves entries the
/// owner deleted alone, and stores the batch's anchors. Errors roll everything back.
pub fn import(conn: &mut Connection, clock: &mut Hlc, user_id: &str, batch: &HealthBatch) -> Result<ImportReport> {
    if batch.values().any(|b| b.anchor.as_ref().is_some_and(|a| a.len() > ANCHOR_MAX)) {
        return Err(crate::invalid("health anchor too long"));
    }
    let tx = conn.transaction()?;
    let trackers = repo::list::<Tracker>(&tx, &[("user_id", user_id)])?;
    let (changes, skipped) = plan(&trackers, batch);
    let mut report = ImportReport { skipped, ..ImportReport::default() };
    for change in changes {
        match change {
            Change::Upsert { id, tracker, data, logged_at } => {
                let tracker = &trackers[tracker];
                let mut row = match stored(&tx, &id)? {
                    Stored::Tombstone => {
                        report.unchanged += 1;
                        continue;
                    }
                    Stored::Live(row) if row.data == data && row.logged_at == logged_at => {
                        report.unchanged += 1;
                        continue;
                    }
                    Stored::Live(row) => TrackerEntry { data, logged_at, ..row },
                    Stored::Missing => TrackerEntry {
                        id,
                        user_id: user_id.to_string(),
                        tracker_id: tracker.id.clone(),
                        data,
                        source: EntrySource::Health,
                        raw_text: None,
                        logged_at,
                        updated_at: String::new(),
                        deleted: false,
                    },
                };
                validate_entry(tracker, &row)?;
                repo::put(&tx, clock, &mut row)?;
                report.written += 1;
            }
            Change::Delete { id } => {
                if let Stored::Live(_) = stored(&tx, &id)? {
                    repo::delete::<TrackerEntry>(&tx, clock, &id)?;
                    report.deleted += 1;
                }
            }
        }
    }
    let mut anchors = local::health_anchors(&tx)?;
    for (kind, part) in batch {
        if let Some(anchor) = &part.anchor {
            anchors.insert(*kind, anchor.clone());
        }
    }
    local::set_health_anchors(&tx, &anchors)?;
    tx.commit()?;
    Ok(report)
}
