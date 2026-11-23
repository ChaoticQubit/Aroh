//! Location awareness on the device (Q56): stores what the iOS bridge sensed in the local-only
//! tables, prunes it, turns repeated visits into place prompts and learned patterns, applies
//! the owner's answers, and hands the bridge its watched places and nudge plan. Raw visits,
//! candidates and patterns never leave this phone; only named places, links and approved
//! routines are written to synced tables.

use crate::{invalid, local, write, Error, Result};
use aroh_domain::hlc::Hlc;
use aroh_domain::recur::DEFAULT_WINDOW_DAYS;
use aroh_domain::rows::{Action, ActionSeries, ActionStatus, Goal, Place, PlaceLink, PlaceSource, Record};
use aroh_location::geo::{distance_m, to_e6, NEAR_M};
use aroh_location::nudge::{nudge_plan, Nudge, NudgeIn};
use aroh_location::pattern::{detect_patterns, normalize, Declined, DECLINE_DAYS};
use aroh_location::prompt::{guess, prompt_clusters, usual, LOOKBACK_DAYS};
use aroh_location::watch::choose_watched;
use aroh_location::Visit;
use aroh_store::local::prune_visits;
use aroh_store::repo;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, SecondsFormat, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Most entries of each kind one drain accepts.
pub const MAX_DRAIN: usize = 2000;

/// Radius of a place named from a prompt or a search, in meters.
pub const PLACE_RADIUS_M: i64 = 150;

/// One visit as the iOS bridge reports it: center in degrees, accuracy in meters, arrival and,
/// once known, departure.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisitIn {
    pub lat: f64,
    pub lon: f64,
    pub accuracy_m: f64,
    pub arrived_at: DateTime<Utc>,
    #[serde(default)]
    pub left_at: Option<DateTime<Utc>>,
}

/// A nudge iOS showed: the place and the local day.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NudgedIn {
    pub place_id: String,
    pub date: NaiveDate,
}

/// The owner's answer to a nudge notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    Done,
    Skip,
    Later,
}

/// A nudge answer for action `action_id`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseIn {
    pub action_id: String,
    pub choice: Choice,
}

/// Everything the iOS bridge buffered since the last drain.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drain {
    #[serde(default)]
    pub visits: Vec<VisitIn>,
    #[serde(default)]
    pub nudged: Vec<NudgedIn>,
    #[serde(default)]
    pub responses: Vec<ResponseIn>,
}

/// A new place prompt that needs the bridge's reverse lookup (point of interest and address).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lookup {
    pub id: String,
    pub lat: f64,
    pub lon: f64,
}

/// What one drain did.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DrainReport {
    pub stored: usize,
    pub pruned: usize,
    pub statuses: usize,
    pub patterns: usize,
    pub lookups: Vec<Lookup>,
}

/// The owner's answer to a place prompt: link it to a goal (and optionally one routine of it),
/// name it without a link, or ignore it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlaceDecision {
    Link { goal_id: String, series_id: Option<String> },
    Rename { name: String },
    Ignore,
}

/// A pending place prompt for the Inbox.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PromptView {
    pub id: String,
    pub guess: Option<String>,
    pub name: String,
    pub address: Option<String>,
    pub visits: usize,
    pub weeks: usize,
    pub usual: String,
    pub lat: f64,
    pub lon: f64,
}

/// A learned pattern waiting for approval, with the goal it would join.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PatternView {
    pub id: String,
    pub title: String,
    pub place: String,
    pub gap_days: i64,
    pub weekday: Option<String>,
    pub local_time: String,
    pub duration_minutes: i64,
    pub evidence: usize,
    pub goal: String,
}

/// A link of a place as the Places screen lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LinkView {
    pub goal_id: String,
    pub series_id: Option<String>,
}

/// A named place with its links, visit count and whether iOS watches it now.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlaceView {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub watched: bool,
    pub visits: usize,
    pub links: Vec<LinkView>,
}

/// The next occurrence a routine row shows: its start and status (planned or missed).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NextView {
    pub start_at: DateTime<Utc>,
    pub status: ActionStatus,
}

/// One routine of the Routines goal with cadence, streak label and dates.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoutineView {
    pub series_id: String,
    pub title: String,
    pub place: Option<String>,
    pub cadence: String,
    pub streak: String,
    pub last_done: Option<DateTime<Utc>>,
    pub next: Option<NextView>,
}

/// A raw visit to a named place, for the location log.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogView {
    pub id: String,
    pub place: String,
    pub arrived_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
}

/// An ignored spot, listed in Settings with Stop ignoring.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IgnoredView {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
}

/// The Settings > Location switches and counts.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettingsView {
    pub enabled: bool,
    pub paused: bool,
    pub nudges: bool,
    pub watched: usize,
    pub visits: usize,
    pub ignored: Vec<IgnoredView>,
}

/// Every location row the screens show, in one read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocationView {
    pub routines_goal: String,
    pub settings: SettingsView,
    pub prompts: Vec<PromptView>,
    pub patterns: Vec<PatternView>,
    pub places: Vec<PlaceView>,
    pub routines: Vec<RoutineView>,
    pub log: Vec<LogView>,
}

/// A place iOS should monitor, center in degrees.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WatchedPlace {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub radius_m: i64,
}

/// What the iOS bridge monitors and may show: the watched places and their nudge plan.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Watch {
    pub places: Vec<WatchedPlace>,
    pub nudges: Vec<Nudge>,
}

fn text_time(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_time(t: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(t).map(|d| d.with_timezone(&Utc)).map_err(|_| invalid("stored location time"))
}

fn degrees(e6: i64) -> f64 {
    e6 as f64 / 1e6
}

fn paused(conn: &Connection) -> Result<bool> {
    Ok(local::setting(conn, "location_paused")?.as_deref() == Some("true"))
}

fn nudges_on(conn: &Connection) -> Result<bool> {
    Ok(local::setting(conn, "location_nudges")?.as_deref() != Some("false"))
}

fn visits(conn: &Connection) -> Result<Vec<Visit>> {
    let mut stmt = conn.prepare("SELECT id, lat_e6, lon_e6, arrived_at, left_at, place_id FROM visits ORDER BY arrived_at")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?, r.get(5)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (id, lat_e6, lon_e6, arrived, left, place_id) = row?;
        out.push(Visit { id, lat_e6, lon_e6, arrived_at: parse_time(&arrived)?, left_at: left.as_deref().map(parse_time).transpose()?, place_id });
    }
    Ok(out)
}

fn store_visit(conn: &Connection, v: &VisitIn) -> Result<bool> {
    let (Some(lat), Some(lon)) = (to_e6(v.lat, true), to_e6(v.lon, false)) else { return Ok(false) };
    if !v.accuracy_m.is_finite() || v.accuracy_m < 0.0 || v.left_at.is_some_and(|l| l < v.arrived_at) {
        return Ok(false);
    }
    let accuracy = v.accuracy_m.min(100_000.0).round() as i64;
    let window = (text_time(v.arrived_at - Duration::seconds(60)), text_time(v.arrived_at + Duration::seconds(60)));
    let mut stmt = conn.prepare("SELECT id, lat_e6, lon_e6 FROM visits WHERE arrived_at BETWEEN ?1 AND ?2")?;
    let same: Option<String> = stmt
        .query_map([&window.0, &window.1], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .find(|(_, la, lo)| distance_m((*la, *lo), (lat, lon)) <= NEAR_M)
        .map(|(id, _, _)| id);
    match same {
        Some(id) => {
            if let Some(left) = v.left_at {
                conn.execute("UPDATE visits SET left_at = ?2 WHERE id = ?1", params![id, text_time(left)])?;
            }
        }
        None => {
            conn.execute(
                "INSERT INTO visits (id, lat_e6, lon_e6, accuracy_m, arrived_at, left_at, place_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
                params![aroh_domain::new_id(), lat, lon, accuracy, text_time(v.arrived_at), v.left_at.map(text_time)],
            )?;
        }
    }
    Ok(true)
}

fn live_places(conn: &Connection, user_id: &str) -> Result<Vec<Place>> {
    Ok(repo::list::<Place>(conn, &[("user_id", user_id)])?)
}

fn live_links(conn: &Connection, user_id: &str) -> Result<Vec<PlaceLink>> {
    Ok(repo::list::<PlaceLink>(conn, &[("user_id", user_id)])?)
}

fn assign_places(conn: &Connection, user_id: &str) -> Result<()> {
    let places = live_places(conn, user_id)?;
    for v in visits(conn)?.into_iter().filter(|v| v.place_id.is_none()) {
        let at = (v.lat_e6, v.lon_e6);
        if let Some(p) = places.iter().find(|p| distance_m((p.lat_e6, p.lon_e6), at) <= p.radius_m as f64) {
            conn.execute("UPDATE visits SET place_id = ?2 WHERE id = ?1", [&v.id, &p.id])?;
        }
    }
    Ok(())
}

struct Candidate {
    id: String,
    lat_e6: i64,
    lon_e6: i64,
    poi_name: Option<String>,
    poi_kind: Option<String>,
    address: Option<String>,
    prompted_at: Option<String>,
}

fn candidates(conn: &Connection, state: Option<&str>) -> Result<Vec<Candidate>> {
    let mut stmt = conn.prepare(
        "SELECT id, lat_e6, lon_e6, poi_name, poi_kind, address, prompted_at FROM place_candidates WHERE ?1 IS NULL OR state = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([state], |r| {
        Ok(Candidate { id: r.get(0)?, lat_e6: r.get(1)?, lon_e6: r.get(2)?, poi_name: r.get(3)?, poi_kind: r.get(4)?, address: r.get(5)?, prompted_at: r.get(6)? })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn refresh_prompts(conn: &Connection, user_id: &str, now: DateTime<Utc>, zone: Tz) -> Result<Vec<Lookup>> {
    let all = candidates(conn, None)?;
    let today = now.with_timezone(&zone).date_naive();
    let asked_today = all.iter().filter_map(|c| c.prompted_at.as_deref()).filter_map(|t| parse_time(t).ok()).any(|t| t.with_timezone(&zone).date_naive() == today);
    if asked_today {
        return Ok(Vec::new());
    }
    let mut known: Vec<(i64, i64)> = all.iter().map(|c| (c.lat_e6, c.lon_e6)).collect();
    known.extend(live_places(conn, user_id)?.iter().map(|p| (p.lat_e6, p.lon_e6)));
    let unnamed: Vec<Visit> = visits(conn)?.into_iter().filter(|v| v.place_id.is_none()).collect();
    let Some(spot) = prompt_clusters(&unnamed, &known, now, zone).into_iter().next() else { return Ok(Vec::new()) };
    let id = aroh_domain::new_id();
    let last = *spot.arrivals.last().expect("a cluster has visits");
    conn.execute(
        "INSERT INTO place_candidates (id, lat_e6, lon_e6, poi_name, poi_kind, address, state, visit_count, last_at, prompted_at) VALUES (?1, ?2, ?3, NULL, NULL, NULL, 'pending', ?4, ?5, ?6)",
        params![id, spot.lat_e6, spot.lon_e6, spot.visits as i64, text_time(last), text_time(now)],
    )?;
    Ok(vec![Lookup { id, lat: degrees(spot.lat_e6), lon: degrees(spot.lon_e6) }])
}

struct PatternRow {
    id: String,
    place_id: String,
    title: String,
    gap_days: i64,
    weekday: Option<i64>,
    local_time: String,
    duration_minutes: i64,
    evidence: i64,
    state: String,
    declined_until: Option<String>,
}

fn pattern_rows(conn: &Connection) -> Result<Vec<PatternRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, place_id, title, gap_days, weekday, local_time, duration_minutes, evidence, state, declined_until FROM pattern_candidates ORDER BY created_at, id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PatternRow {
            id: r.get(0)?,
            place_id: r.get(1)?,
            title: r.get(2)?,
            gap_days: r.get(3)?,
            weekday: r.get(4)?,
            local_time: r.get(5)?,
            duration_minutes: r.get(6)?,
            evidence: r.get(7)?,
            state: r.get(8)?,
            declined_until: r.get(9)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn refresh_patterns(conn: &Connection, user_id: &str, now: DateTime<Utc>, zone: Tz) -> Result<usize> {
    let rows = pattern_rows(conn)?;
    let far = now + Duration::days(36_500);
    let declined: Vec<Declined> = rows
        .iter()
        .filter(|r| r.state != "pending")
        .map(|r| {
            let until = if r.state == "approved" { Some(far) } else { r.declined_until.as_deref().and_then(|t| parse_time(t).ok()) };
            Declined { place_id: r.place_id.clone(), key: normalize(&r.title), until: until.unwrap_or(far) }
        })
        .collect();
    let mine = [("user_id", user_id)];
    let actions = repo::list::<Action>(conn, &mine)?;
    let series = repo::list::<ActionSeries>(conn, &mine)?;
    let found = detect_patterns(&visits(conn)?, &actions, &series, &declined, now, zone);
    for p in &found {
        let weekday = p.weekday.map(|d| i64::from(d.num_days_from_monday()));
        let time = p.local_time.format("%H:%M").to_string();
        let pending = rows.iter().find(|r| r.state == "pending" && r.place_id == p.place_id && normalize(&r.title) == p.key);
        match pending {
            Some(r) => {
                conn.execute(
                    "UPDATE pattern_candidates SET title = ?2, gap_days = ?3, weekday = ?4, local_time = ?5, duration_minutes = ?6, evidence = ?7 WHERE id = ?1",
                    params![r.id, p.title, p.gap_days, weekday, time, p.duration_minutes, p.evidence as i64],
                )?;
            }
            None => {
                conn.execute(
                    "INSERT INTO pattern_candidates (id, place_id, title, gap_days, weekday, local_time, duration_minutes, evidence, state, declined_until, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', NULL, ?9)",
                    params![aroh_domain::new_id(), p.place_id, p.title, p.gap_days, weekday, time, p.duration_minutes, p.evidence as i64, text_time(now)],
                )?;
            }
        }
    }
    Ok(conn.query_row("SELECT count(*) FROM pattern_candidates WHERE state = 'pending'", [], |r| r.get::<_, i64>(0))? as usize)
}

/// Stores one bridge drain for `user_id` in a single transaction: visits (none while paused,
/// a repeated arrival only updates its departure, unusable ones are skipped), the shown
/// nudges, and Mark done or Skip today answers as status changes (Q53: never a time change).
/// Then deletes visits older than 90 days, files visits under named places, asks about at
/// most one new place a day and refreshes the learned patterns.
pub fn drain(conn: &mut Connection, clock: &mut Hlc, user_id: &str, batch: &Drain, now: DateTime<Utc>, zone: Tz) -> Result<DrainReport> {
    if batch.visits.len() > MAX_DRAIN || batch.nudged.len() > MAX_DRAIN || batch.responses.len() > MAX_DRAIN {
        return Err(invalid("location drain too large"));
    }
    let tx = conn.transaction()?;
    let mut report = DrainReport::default();
    if !paused(&tx)? {
        for v in &batch.visits {
            report.stored += usize::from(store_visit(&tx, v)?);
        }
    }
    for n in &batch.nudged {
        aroh_domain::rows::check_uuid("nudged.place_id", &n.place_id)?;
        tx.execute("INSERT OR IGNORE INTO nudge_sent (place_id, local_date) VALUES (?1, ?2)", params![n.place_id, n.date.to_string()])?;
    }
    for r in &batch.responses {
        let status = match r.choice {
            Choice::Done => ActionStatus::Done,
            Choice::Skip => ActionStatus::Skipped,
            Choice::Later => continue,
        };
        match write::set_action_status(&tx, clock, &r.action_id, status, now) {
            Ok(_) => report.statuses += 1,
            Err(Error::Store(aroh_store::Error::NotFound(_))) => {}
            Err(e) => return Err(e),
        }
    }
    let week_ago = (now - Duration::days(14)).with_timezone(&zone).date_naive().to_string();
    tx.execute("DELETE FROM nudge_sent WHERE local_date < ?1", [week_ago])?;
    report.pruned = prune_visits(&tx, now)?;
    assign_places(&tx, user_id)?;
    report.lookups = refresh_prompts(&tx, user_id, now, zone)?;
    report.patterns = refresh_patterns(&tx, user_id, now, zone)?;
    tx.commit()?;
    Ok(report)
}

/// Stores the bridge's reverse lookup for prompt `id`: point-of-interest name and MapKit
/// category, and the address. Errors with `Invalid` for overlong text or a decided prompt.
pub fn set_lookup(conn: &Connection, id: &str, name: Option<&str>, kind: Option<&str>, address: Option<&str>) -> Result<()> {
    let too_long = |s: Option<&str>, max: usize| s.is_some_and(|s| s.chars().count() > max || s.chars().any(char::is_control));
    if too_long(name, 100) || too_long(kind, 100) || too_long(address, 300) {
        return Err(invalid("place lookup"));
    }
    let clean = |s: Option<&str>| s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let changed = conn.execute(
        "UPDATE place_candidates SET poi_name = ?2, poi_kind = ?3, address = ?4 WHERE id = ?1 AND state = 'pending'",
        params![id, clean(name), clean(kind), clean(address)],
    )?;
    if changed != 1 {
        return Err(invalid("place prompt was already answered"));
    }
    Ok(())
}

/// The built-in Routines goal of `user_id`, written with this device's stamp the first time
/// a routine or place needs it, so it syncs like any goal.
pub fn ensure_routines(conn: &Connection, clock: &mut Hlc, user_id: &str) -> Result<Goal> {
    let id = aroh_domain::seed::routines_id();
    match repo::get::<Goal>(conn, &id) {
        Ok(goal) => Ok(goal),
        Err(aroh_store::Error::NotFound(_)) => {
            let mut goal = aroh_domain::seed::routines_goal(user_id);
            repo::put(conn, clock, &mut goal)?;
            Ok(goal)
        }
        Err(e) => Err(e.into()),
    }
}

fn target_goal(conn: &Connection, clock: &mut Hlc, user_id: &str, goal_id: &str) -> Result<Goal> {
    if goal_id == aroh_domain::seed::routines_id() {
        return ensure_routines(conn, clock, user_id);
    }
    let goal: Goal = repo::get(conn, goal_id)?;
    if goal.user_id != user_id {
        return Err(invalid("goal of another user"));
    }
    Ok(goal)
}

fn new_place(conn: &Connection, clock: &mut Hlc, user_id: &str, name: &str, address: Option<&str>, at: (i64, i64), source: PlaceSource) -> Result<Place> {
    let mut place = Place {
        id: aroh_domain::new_id(),
        user_id: user_id.to_string(),
        name: name.trim().to_string(),
        address: address.map(str::trim).filter(|a| !a.is_empty()).map(str::to_string),
        lat_e6: at.0,
        lon_e6: at.1,
        radius_m: PLACE_RADIUS_M,
        source,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut place)?;
    Ok(place)
}

/// Links named place `place_id` to goal `goal_id` (the Routines id creates that goal) and,
/// with `series_id`, to one routine of that goal. Errors with `Invalid` or `NotFound` for a
/// missing place, goal or series, or a series of another goal.
pub fn link_place(conn: &Connection, clock: &mut Hlc, user_id: &str, place_id: &str, goal_id: &str, series_id: Option<&str>) -> Result<PlaceLink> {
    let place: Place = repo::get(conn, place_id)?;
    let goal = target_goal(conn, clock, user_id, goal_id)?;
    if let Some(series_id) = series_id {
        let series: ActionSeries = repo::get(conn, series_id)?;
        if series.goal_id != goal.id {
            return Err(invalid("routine of another goal"));
        }
    }
    let mut link = PlaceLink {
        id: aroh_domain::new_id(),
        user_id: user_id.to_string(),
        place_id: place.id,
        goal_id: goal.id,
        series_id: series_id.map(str::to_string),
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(conn, clock, &mut link)?;
    Ok(link)
}

/// Names a place found by map search: `lat` and `lon` in degrees, a 150 m radius.
/// Errors with `Invalid` for a bad name or a point off the globe.
pub fn add_place(conn: &Connection, clock: &mut Hlc, user_id: &str, name: &str, address: Option<&str>, lat: f64, lon: f64) -> Result<Place> {
    let (Some(lat), Some(lon)) = (to_e6(lat, true), to_e6(lon, false)) else { return Err(invalid("place position")) };
    let tx = conn.unchecked_transaction()?;
    let place = new_place(&tx, clock, user_id, name, address, (lat, lon), PlaceSource::Search)?;
    assign_places(&tx, user_id)?;
    tx.commit()?;
    Ok(place)
}

/// Answers place prompt `id`. Link writes one place and one link and keeps the guessed kind
/// ("gym") on this phone for nudges, Rename one place, Ignore only marks the prompt ignored.
/// Output: the new place, or `None` for Ignore.
/// Errors with `Invalid` for a prompt already answered or a bad name; nothing changes then.
pub fn decide_prompt(conn: &Connection, clock: &mut Hlc, user_id: &str, id: &str, decision: PlaceDecision, _now: DateTime<Utc>) -> Result<Option<Place>> {
    let candidate = candidates(conn, Some("pending"))?.into_iter().find(|c| c.id == id).ok_or_else(|| invalid("place prompt was already answered"))?;
    let tx = conn.unchecked_transaction()?;
    let fallback = candidate.poi_name.clone().or_else(|| candidate.address.clone()).unwrap_or_else(|| "Place".to_string());
    let at = (candidate.lat_e6, candidate.lon_e6);
    let place = match decision {
        PlaceDecision::Ignore => {
            tx.execute("UPDATE place_candidates SET state = 'ignored' WHERE id = ?1", [id])?;
            None
        }
        PlaceDecision::Rename { name } => Some(new_place(&tx, clock, user_id, &name, candidate.address.as_deref(), at, PlaceSource::Prompt)?),
        PlaceDecision::Link { goal_id, series_id } => {
            let place = new_place(&tx, clock, user_id, &fallback, candidate.address.as_deref(), at, PlaceSource::Prompt)?;
            link_place(&tx, clock, user_id, &place.id, &goal_id, series_id.as_deref())?;
            if let Some(kind) = guess(candidate.poi_kind.as_deref()) {
                tx.execute("INSERT OR REPLACE INTO place_kinds (place_id, kind) VALUES (?1, ?2)", [&place.id, kind])?;
            }
            Some(place)
        }
    };
    if place.is_some() {
        tx.execute("DELETE FROM place_candidates WHERE id = ?1", [id])?;
        assign_places(&tx, user_id)?;
    }
    tx.commit()?;
    Ok(place)
}

/// Stops ignoring spot `id`, so Aroh may notice it again. Errors with `Invalid` when it is not ignored.
pub fn unignore(conn: &Connection, id: &str) -> Result<()> {
    if conn.execute("DELETE FROM place_candidates WHERE id = ?1 AND state = 'ignored'", [id])? != 1 {
        return Err(invalid("place is not ignored"));
    }
    Ok(())
}

/// Clears the raw history on this phone: every visit, pending place prompt and pending
/// pattern. Named places, links, routines and ignored spots stay.
pub fn clear_history(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM visits", [])?;
    tx.execute("DELETE FROM place_candidates WHERE state = 'pending'", [])?;
    tx.execute("DELETE FROM pattern_candidates WHERE state = 'pending'", [])?;
    tx.commit()?;
    Ok(())
}

fn rrule(gap: i64, weekday: Option<Weekday>) -> String {
    let code = |d: Weekday| ["MO", "TU", "WE", "TH", "FR", "SA", "SU"][d.num_days_from_monday() as usize];
    match (weekday, gap) {
        (Some(d), g) if g % 7 == 0 && g > 7 => format!("FREQ=WEEKLY;INTERVAL={};BYDAY={}", g / 7, code(d)),
        (Some(d), _) => format!("FREQ=WEEKLY;BYDAY={}", code(d)),
        (None, 7) => "FREQ=WEEKLY".into(),
        (None, g) if g % 7 == 0 => format!("FREQ=WEEKLY;INTERVAL={}", g / 7),
        (None, 1) => "FREQ=DAILY".into(),
        (None, g) => format!("FREQ=DAILY;INTERVAL={g}"),
    }
}

fn first_start(now: DateTime<Utc>, zone: Tz, time: NaiveTime, weekday: Option<Weekday>) -> String {
    let today = now.with_timezone(&zone).date_naive();
    let date = (0..=7)
        .map(|d| today + Duration::days(d))
        .find(|d| {
            let fits = weekday.is_none_or(|w| d.weekday() == w);
            let at = zone.from_local_datetime(&d.and_time(time)).earliest();
            fits && at.is_some_and(|t| t.with_timezone(&Utc) > now)
        })
        .expect("a matching day within a week");
    date.and_time(time).format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Applies or declines learned pattern `id`. Apply writes one repeating action in the place's
/// linked goal (else the Routines goal) and one place link naming it, then materializes its
/// occurrences; Decline keeps it on the phone and silences it for 60 days. Output: the new
/// series, or `None` when declined. Errors with `Invalid` for a pattern already decided.
pub fn decide_pattern(conn: &Connection, clock: &mut Hlc, user_id: &str, id: &str, apply: bool, now: DateTime<Utc>, zone: Tz) -> Result<Option<ActionSeries>> {
    let row = pattern_rows(conn)?.into_iter().find(|r| r.id == id && r.state == "pending").ok_or_else(|| invalid("pattern was already decided"))?;
    if !apply {
        conn.execute(
            "UPDATE pattern_candidates SET state = 'declined', declined_until = ?2 WHERE id = ?1",
            params![id, text_time(now + Duration::days(DECLINE_DAYS))],
        )?;
        return Ok(None);
    }
    let tx = conn.unchecked_transaction()?;
    let place: Place = repo::get(&tx, &row.place_id)?;
    let linked_goal = live_links(&tx, user_id)?.into_iter().find(|l| l.place_id == place.id).map(|l| l.goal_id);
    let goal = match linked_goal {
        Some(goal_id) => target_goal(&tx, clock, user_id, &goal_id)?,
        None => ensure_routines(&tx, clock, user_id)?,
    };
    let weekday = row.weekday.map(|d| Weekday::try_from(d as u8).map_err(|_| invalid("stored weekday"))).transpose()?;
    let time = NaiveTime::parse_from_str(&row.local_time, "%H:%M").map_err(|_| invalid("stored pattern time"))?;
    let mut series = ActionSeries {
        id: aroh_domain::new_id(),
        user_id: user_id.to_string(),
        goal_id: goal.id.clone(),
        title: row.title.clone(),
        notes: Some(format!("Learned at {}.", place.name)),
        rrule: rrule(row.gap_days, weekday),
        dtstart_local: first_start(now, zone, time, weekday),
        timezone: zone.name().to_string(),
        duration_minutes: row.duration_minutes.clamp(1, 1440),
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(&tx, clock, &mut series)?;
    link_place(&tx, clock, user_id, &place.id, &goal.id, Some(&series.id))?;
    tx.execute("UPDATE pattern_candidates SET state = 'approved' WHERE id = ?1", [id])?;
    tx.commit()?;
    repo::materialize(conn, &series.id, now, DEFAULT_WINDOW_DAYS)?;
    Ok(Some(series))
}

fn visit_counts(all: &[Visit]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for v in all {
        if let Some(p) = &v.place_id {
            *counts.entry(p.clone()).or_insert(0) += 1;
        }
    }
    counts
}

/// The places iOS should monitor and their nudge plan for the next days: none while paused,
/// and no nudges while the owner turned them off.
pub fn watch(conn: &Connection, user_id: &str, now: DateTime<Utc>, zone: Tz) -> Result<Watch> {
    if paused(conn)? {
        return Ok(Watch { places: Vec::new(), nudges: Vec::new() });
    }
    let places = live_places(conn, user_id)?;
    let links = live_links(conn, user_id)?;
    let actions = repo::list::<Action>(conn, &[("user_id", user_id)])?;
    let ids = choose_watched(&places, &links, &actions, &visit_counts(&visits(conn)?), now);
    let watched: Vec<Place> = ids.iter().filter_map(|id| places.iter().find(|p| &p.id == id).cloned()).collect();
    let nudges = if nudges_on(conn)? {
        let goals = repo::list::<Goal>(conn, &[("user_id", user_id)])?;
        let mut stmt = conn.prepare("SELECT place_id, local_date FROM nudge_sent")?;
        let sent: BTreeSet<(String, NaiveDate)> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter_map(|(p, d)| d.parse().ok().map(|d| (p, d)))
            .collect();
        let mut stmt = conn.prepare("SELECT place_id, kind FROM place_kinds")?;
        let kinds: BTreeMap<String, String> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        nudge_plan(&NudgeIn { places: &watched, links: &links, goals: &goals, actions: &actions, sent: &sent, kinds: &kinds, paused: false }, now, zone)
    } else {
        Vec::new()
    };
    let places = watched.iter().map(|p| WatchedPlace { id: p.id.clone(), name: p.name.clone(), lat: degrees(p.lat_e6), lon: degrees(p.lon_e6), radius_m: p.radius_m }).collect();
    Ok(Watch { places, nudges })
}

fn prompt_views(conn: &Connection, all: &[Visit], now: DateTime<Utc>, zone: Tz) -> Result<Vec<PromptView>> {
    let since = now - Duration::days(LOOKBACK_DAYS);
    let mut out = Vec::new();
    for c in candidates(conn, Some("pending"))? {
        let near: Vec<DateTime<Utc>> = all.iter().filter(|v| v.arrived_at >= since && distance_m((v.lat_e6, v.lon_e6), (c.lat_e6, c.lon_e6)) <= NEAR_M).map(|v| v.arrived_at).collect();
        let Some(first) = near.first() else { continue };
        let weeks = ((now - *first).num_days().max(0) as usize).div_ceil(7).max(1);
        out.push(PromptView {
            id: c.id.clone(),
            guess: guess(c.poi_kind.as_deref()).map(str::to_string),
            name: c.poi_name.clone().or_else(|| c.address.clone()).unwrap_or_else(|| "A place you visit often".to_string()),
            address: c.address.clone(),
            visits: near.len(),
            weeks,
            usual: usual(&near, zone),
            lat: degrees(c.lat_e6),
            lon: degrees(c.lon_e6),
        });
    }
    Ok(out)
}

fn weekday_name(d: i64) -> Option<String> {
    Weekday::try_from(d as u8).ok().map(|w| ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"][w.num_days_from_monday() as usize].to_string())
}

fn routine_views(conn: &Connection, user_id: &str, places: &[Place], links: &[PlaceLink], now: DateTime<Utc>) -> Result<Vec<RoutineView>> {
    let routines = aroh_domain::seed::routines_id();
    let series = repo::list::<ActionSeries>(conn, &[("user_id", user_id), ("goal_id", routines.as_str())])?;
    let mut out = Vec::new();
    for s in series {
        let actions = repo::list::<Action>(conn, &[("series_id", s.id.as_str())])?;
        let place = links.iter().find(|l| l.series_id.as_deref() == Some(s.id.as_str())).and_then(|l| places.iter().find(|p| p.id == l.place_id)).map(|p| p.name.clone());
        let last_done = actions.iter().filter_map(|a| a.done_at).max();
        let recent = now - Duration::days(7);
        let next = actions
            .iter()
            .filter(|a| (a.status == ActionStatus::Missed && a.start_at >= recent) || (a.status == ActionStatus::Planned && a.end_at > now - Duration::hours(1)))
            .min_by_key(|a| a.start_at)
            .map(|a| NextView { start_at: a.start_at, status: a.status });
        out.push(RoutineView {
            series_id: s.id.clone(),
            title: s.title.clone(),
            place,
            cadence: aroh_location::streak::cadence(&s),
            streak: aroh_location::streak::streak(&s, &actions, now).label,
            last_done,
            next,
        });
    }
    Ok(out)
}

/// Reads every location row the screens show for `user_id` at `now` in `zone`: switches and
/// counts, pending place prompts, learned patterns, named places, the Routines dashboard
/// rows and the visit log (named places only, newest first).
pub fn view(conn: &Connection, user_id: &str, now: DateTime<Utc>, zone: Tz) -> Result<LocationView> {
    let all = visits(conn)?;
    let places = live_places(conn, user_id)?;
    let links = live_links(conn, user_id)?;
    let watched: BTreeSet<String> = watch(conn, user_id, now, zone)?.places.into_iter().map(|p| p.id).collect();
    let counts = visit_counts(&all);
    let goals = repo::list::<Goal>(conn, &[("user_id", user_id)])?;
    let routines_title = |goal_id: &str| goals.iter().find(|g| g.id == goal_id).map(|g| g.title.clone()).unwrap_or_else(|| "Routines".to_string());
    let patterns = pattern_rows(conn)?
        .into_iter()
        .filter(|r| r.state == "pending")
        .filter_map(|r| {
            let place = places.iter().find(|p| p.id == r.place_id)?;
            let goal = links.iter().find(|l| l.place_id == place.id).map(|l| routines_title(&l.goal_id)).unwrap_or_else(|| "Routines".to_string());
            Some(PatternView {
                id: r.id,
                title: r.title,
                place: place.name.clone(),
                gap_days: r.gap_days,
                weekday: r.weekday.and_then(weekday_name),
                local_time: r.local_time,
                duration_minutes: r.duration_minutes,
                evidence: r.evidence as usize,
                goal,
            })
        })
        .collect();
    let place_views = places
        .iter()
        .map(|p| PlaceView {
            id: p.id.clone(),
            name: p.name.clone(),
            address: p.address.clone(),
            lat: degrees(p.lat_e6),
            lon: degrees(p.lon_e6),
            watched: watched.contains(&p.id),
            visits: counts.get(&p.id).copied().unwrap_or(0),
            links: links.iter().filter(|l| l.place_id == p.id).map(|l| LinkView { goal_id: l.goal_id.clone(), series_id: l.series_id.clone() }).collect(),
        })
        .collect();
    let mut log: Vec<LogView> = all
        .iter()
        .filter_map(|v| {
            let place = places.iter().find(|p| Some(&p.id) == v.place_id.as_ref())?;
            Some(LogView { id: v.id.clone(), place: place.name.clone(), arrived_at: v.arrived_at, left_at: v.left_at })
        })
        .collect();
    log.sort_by(|a, b| b.arrived_at.cmp(&a.arrived_at));
    log.truncate(200);
    let ignored = candidates(conn, Some("ignored"))?
        .into_iter()
        .map(|c| IgnoredView { id: c.id, name: c.poi_name.or_else(|| c.address.clone()).unwrap_or_else(|| "Unnamed place".to_string()), address: c.address })
        .collect();
    let settings = SettingsView {
        enabled: local::setting(conn, "location")?.as_deref() == Some("true"),
        paused: paused(conn)?,
        nudges: nudges_on(conn)?,
        watched: watched.len(),
        visits: all.len(),
        ignored,
    };
    assert!(settings.watched <= aroh_location::watch::MAX_WATCHED);
    Ok(LocationView {
        routines_goal: aroh_domain::seed::routines_id(),
        settings,
        prompts: prompt_views(conn, &all, now, zone)?,
        patterns,
        places: place_views,
        routines: routine_views(conn, user_id, &places, &links, now)?,
        log,
    })
}

/// Tombstones a named place and its links; its visits stay in the local log unnamed.
/// Errors with `NotFound` for a missing place.
pub fn delete_place(conn: &Connection, clock: &mut Hlc, user_id: &str, place_id: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let place: Place = repo::get(&tx, place_id)?;
    for link in live_links(&tx, user_id)?.into_iter().filter(|l| l.place_id == place.id) {
        repo::delete::<PlaceLink>(&tx, clock, link.id())?;
    }
    repo::delete::<Place>(&tx, clock, &place.id)?;
    tx.execute("UPDATE visits SET place_id = NULL WHERE place_id = ?1", [&place.id])?;
    tx.execute("DELETE FROM place_kinds WHERE place_id = ?1", [&place.id])?;
    tx.commit()?;
    Ok(())
}
