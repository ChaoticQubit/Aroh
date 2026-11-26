//! Location awareness commands (Q56): turning it on in two permission steps, the screens'
//! reads and answers, and the start and resume drain that moves what the iOS bridge sensed
//! into the local-only tables and hands back the watched places and nudge plan.

use super::with_core;
use crate::commands::app::after_write;
use crate::commands::engine::sync_if_signed_in;
use crate::AppState;
use aroh_app::location::{self, Drain, LocationView, PlaceDecision};
use aroh_domain::rows::{ActionSeries, Place, PlaceLink};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_aroh_ios::IosExt;

const MAX_QUERY: usize = 200;

#[derive(Deserialize)]
struct Authorized {
    status: String,
}

/// A map search hit or reverse lookup from the bridge: point of interest, MapKit category,
/// address and center in degrees.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceHit {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub address: Option<String>,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Deserialize)]
struct Hits {
    hits: Vec<PlaceHit>,
}

#[derive(Deserialize)]
struct Snapshot {
    url: String,
}

fn enabled(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    with_core(&state, |c| aroh_app::local::setting(&c.conn, "location")).ok().flatten().as_deref() == Some("true")
}

async fn push_watch(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let plan = with_core(&state, |c| location::watch(&c.conn, &c.user_id, Utc::now(), crate::zone()))?;
    app.ios().call::<Value>("location_watch", plan).await.map(|_| ())
}

async fn refresh(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let batch: Drain = app.ios().call("location_drain", json!({})).await?;
    let report = with_core(&state, |c| location::drain(&mut c.conn, &mut c.clock, &c.user_id, &batch, Utc::now(), crate::zone()))?;
    lookups(app, &report).await?;
    push_watch(app).await?;
    if report.statuses > 0 || !report.lookups.is_empty() || report.patterns > 0 {
        after_write(app);
    }
    Ok(())
}

async fn lookups(app: &AppHandle, report: &location::DrainReport) -> Result<(), String> {
    let state = app.state::<AppState>();
    for spot in &report.lookups {
        match app.ios().call::<PlaceHit>("location_lookup", json!({"lat": spot.lat, "lon": spot.lon})).await {
            Ok(hit) => with_core(&state, |c| location::set_lookup(&c.conn, &spot.id, hit.name.as_deref(), hit.kind.as_deref(), hit.address.as_deref()))?,
            Err(e) => eprintln!("aroh: place lookup failed: {e}"),
        }
    }
    Ok(())
}

/// Drains the bridge and refreshes the watched places in the background at start and on
/// resume, when location is on (iPhone only). Failures are logged, never shown.
pub fn drain_if_enabled(app: &AppHandle) {
    if cfg!(not(target_os = "ios")) || !enabled(app) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = refresh(&app).await {
            eprintln!("aroh: location drain failed: {e}");
        }
    });
}

/// Turns location awareness on: asks for When In Use, then Always, starts visits and
/// watched places. Returns the status line Settings shows; errors are UI-safe strings.
#[tauri::command]
pub async fn app_location_enable(app: AppHandle) -> Result<String, String> {
    let first: Authorized = app.ios().call("location_authorize", json!({"step": "when_in_use"})).await?;
    if first.status != "when_in_use" && first.status != "always" {
        return Ok("Location is off in iOS Settings. Turn it on there to continue.".into());
    }
    let second: Authorized = app.ios().call("location_authorize", json!({"step": "always"})).await?;
    let state = app.state::<AppState>();
    with_core(&state, |c| aroh_app::local::set_setting(&c.conn, "location", "true"))?;
    app.ios().call::<Value>("location_enable", json!({"on": true})).await?;
    refresh(&app).await?;
    after_write(&app);
    Ok(if second.status == "always" { "Location is on.".into() } else { "Location is on while Aroh is open. Allow Always for nudges.".into() })
}

/// Turns location awareness off: stops visits and every watched place. History stays until cleared.
#[tauri::command]
pub async fn app_location_disable(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    with_core(&state, |c| aroh_app::local::set_setting(&c.conn, "location", "false"))?;
    if cfg!(target_os = "ios") {
        app.ios().call::<Value>("location_enable", json!({"on": false})).await?;
    }
    after_write(&app);
    Ok(())
}

/// Sets `location_paused` or `location_nudges` to `value` and refreshes the watched places.
#[tauri::command]
pub async fn app_location_setting(app: AppHandle, name: String, value: bool) -> Result<(), String> {
    if name != "location_paused" && name != "location_nudges" {
        return Err("invalid location setting".into());
    }
    let state = app.state::<AppState>();
    with_core(&state, |c| aroh_app::local::set_setting(&c.conn, &name, if value { "true" } else { "false" }))?;
    if cfg!(target_os = "ios") && enabled(&app) {
        push_watch(&app).await?;
    }
    after_write(&app);
    Ok(())
}

/// Reads every location row the screens show.
#[tauri::command(async)]
pub fn app_location_view(state: State<'_, AppState>) -> Result<LocationView, String> {
    with_core(&state, |c| location::view(&c.conn, &c.user_id, Utc::now(), crate::zone()))
}

async fn changed(app: &AppHandle) {
    if cfg!(target_os = "ios") && enabled(app) {
        if let Err(e) = push_watch(app).await {
            eprintln!("aroh: watched places not refreshed: {e}");
        }
    }
    after_write(app);
}

/// Answers place prompt `id` with a link, a name or ignore. Returns the new place, if any.
#[tauri::command]
pub async fn app_location_decide_prompt(app: AppHandle, id: String, decision: PlaceDecision) -> Result<Option<Place>, String> {
    let state = app.state::<AppState>();
    let place = with_core(&state, |c| location::decide_prompt(&c.conn, &mut c.clock, &c.user_id, &id, decision, Utc::now()))?;
    changed(&app).await;
    Ok(place)
}

/// Stops ignoring spot `id`.
#[tauri::command(async)]
pub fn app_location_unignore(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_core(&state, |c| location::unignore(&c.conn, &id))?;
    after_write(&app);
    Ok(())
}

/// Searches MapKit for `query` near the device. Returns up to 10 hits.
#[tauri::command]
pub async fn app_location_search(app: AppHandle, query: String) -> Result<Vec<PlaceHit>, String> {
    let query = query.trim();
    if query.is_empty() || query.chars().count() > MAX_QUERY {
        return Err("invalid search".into());
    }
    let hits: Hits = app.ios().call("location_search", json!({"query": query})).await?;
    Ok(hits.hits)
}

/// A map image of a spot in degrees, as a PNG data URL of `width` by `height` points.
#[tauri::command]
pub async fn app_location_snapshot(app: AppHandle, lat: f64, lon: f64, width: f64, height: f64) -> Result<String, String> {
    if !lat.is_finite() || !lon.is_finite() || lat.abs() > 90.0 || lon.abs() > 180.0 || !width.is_finite() || !height.is_finite() {
        return Err("invalid map".into());
    }
    let shot: Snapshot = app.ios().call("location_snapshot", json!({"lat": lat, "lon": lon, "width": width, "height": height})).await?;
    Ok(shot.url)
}

/// Names a place found by search.
#[tauri::command]
pub async fn app_location_add_place(app: AppHandle, name: String, address: Option<String>, lat: f64, lon: f64) -> Result<Place, String> {
    let state = app.state::<AppState>();
    let place = with_core(&state, |c| location::add_place(&c.conn, &mut c.clock, &c.user_id, &name, address.as_deref(), lat, lon))?;
    changed(&app).await;
    Ok(place)
}

/// Links place `place_id` to goal `goal_id` and, optionally, one routine of it.
#[tauri::command]
pub async fn app_location_link(app: AppHandle, place_id: String, goal_id: String, series_id: Option<String>) -> Result<PlaceLink, String> {
    let state = app.state::<AppState>();
    let link = with_core(&state, |c| location::link_place(&c.conn, &mut c.clock, &c.user_id, &place_id, &goal_id, series_id.as_deref()))?;
    changed(&app).await;
    Ok(link)
}

/// Deletes named place `place_id` and its links.
#[tauri::command]
pub async fn app_location_delete_place(app: AppHandle, place_id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    with_core(&state, |c| location::delete_place(&c.conn, &mut c.clock, &c.user_id, &place_id))?;
    changed(&app).await;
    Ok(())
}

/// Applies or declines learned pattern `id`. Returns the new routine when applied.
#[tauri::command]
pub async fn app_location_decide_pattern(app: AppHandle, id: String, apply: bool) -> Result<Option<ActionSeries>, String> {
    let state = app.state::<AppState>();
    let series = with_core(&state, |c| location::decide_pattern(&c.conn, &mut c.clock, &c.user_id, &id, apply, Utc::now(), crate::zone()))?;
    changed(&app).await;
    Ok(series)
}

/// Deletes routine `series_id` from now (Q93): the series ends before its next date, its
/// open sessions from now go, done and logged ones stay. Reminders are rescheduled and the
/// rows pushed. Output: the series as stored.
#[tauri::command(async)]
pub fn app_location_end_routine(app: AppHandle, state: State<'_, AppState>, series_id: String) -> Result<ActionSeries, String> {
    let series = with_core(&state, |c| aroh_app::write::end_series(&c.conn, &mut c.clock, &series_id, Utc::now()))?;
    after_write(&app);
    sync_if_signed_in(&app);
    Ok(series)
}

/// Deletes the Routines goal (Q93): every routine ends from now and the goal is tombstoned; it
/// comes back when a new routine is approved. Reminders are rescheduled and the rows pushed.
#[tauri::command(async)]
pub fn app_location_delete_routines(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    with_core(&state, |c| aroh_app::remove::delete_routines(&c.conn, &mut c.clock, &c.user_id, Utc::now()))?;
    after_write(&app);
    sync_if_signed_in(&app);
    Ok(())
}

/// Clears the location history on this phone.
#[tauri::command(async)]
pub fn app_location_clear(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    with_core(&state, |c| location::clear_history(&c.conn))?;
    after_write(&app);
    Ok(())
}

/// Debug builds only, for simulator checks. `prompt` adds four evening stays of the last two
/// weeks at the spot; `pattern` names an unlinked "City pool" there with five Saturday
/// morning stays, each with a done "Swim laps", so a learned routine shows up; `due` plans
/// a "Strength session" in 20 minutes in the goal of the first linked place.
#[tauri::command]
pub async fn app_dev_location_seed(app: AppHandle, kind: String, lat: f64, lon: f64) -> Result<(), String> {
    if !cfg!(debug_assertions) {
        return Err("debug builds only".into());
    }
    use aroh_domain::rows::{Action, ActionStatus, Goal};
    use chrono::{Datelike, Duration, NaiveTime, TimeZone, Weekday};
    let zone = crate::zone();
    let now = Utc::now();
    let today = now.with_timezone(&zone).date_naive();
    let at = |days: i64, hour: u32| zone.from_local_datetime(&(today - Duration::days(days)).and_time(NaiveTime::from_hms_opt(hour, 0, 0).expect("an hour"))).earliest().expect("a local time").with_timezone(&Utc);
    let visit = |start: chrono::DateTime<Utc>, minutes: i64| location::VisitIn { lat, lon, accuracy_m: 20.0, arrived_at: start, left_at: Some(start + Duration::minutes(minutes)) };
    let state = app.state::<AppState>();
    let report = with_core(&state, |c| {
        let batch = match kind.as_str() {
            "prompt" => Drain { visits: [12, 9, 5, 2].iter().map(|d| visit(at(*d, 18), 60)).collect(), ..Drain::default() },
            "due" => {
                let link = aroh_store::repo::list::<PlaceLink>(&c.conn, &[("user_id", &c.user_id)])?.into_iter().next().ok_or_else(|| aroh_app::Error::Invalid("no linked place".into()))?;
                let start = now + Duration::minutes(20);
                let mut action = Action {
                    id: aroh_domain::new_id(),
                    user_id: c.user_id.clone(),
                    goal_id: link.goal_id,
                    series_id: None,
                    occurrence: None,
                    title: "Strength session".into(),
                    notes: None,
                    start_at: start,
                    end_at: start + Duration::minutes(45),
                    status: ActionStatus::Planned,
                    locked: false,
                    done_at: None,
                    card: None,
                    updated_at: String::new(),
                    deleted: false,
                };
                aroh_store::repo::put(&c.conn, &mut c.clock, &mut action)?;
                Drain::default()
            }
            "pattern" => {
                let place = location::add_place(&c.conn, &mut c.clock, &c.user_id, "City pool", Some("19th and Linda St"), lat, lon)?;
                let goal: Goal = location::ensure_routines(&c.conn, &mut c.clock, &c.user_id)?;
                let back = (today.weekday().num_days_from_monday() as i64 + 7 - Weekday::Sat.num_days_from_monday() as i64) % 7;
                let mut visits = Vec::new();
                for week in 0..5 {
                    let start = at(back + 7 * week + if back == 0 { 7 } else { 0 }, 10);
                    let mut action = Action {
                        id: aroh_domain::new_id(),
                        user_id: c.user_id.clone(),
                        goal_id: goal.id.clone(),
                        series_id: None,
                        occurrence: None,
                        title: "Swim laps".into(),
                        notes: None,
                        start_at: start,
                        end_at: start + Duration::minutes(45),
                        status: ActionStatus::Done,
                        locked: false,
                        done_at: Some(start + Duration::minutes(40)),
                        card: None,
                        updated_at: String::new(),
                        deleted: false,
                    };
                    aroh_store::repo::put(&c.conn, &mut c.clock, &mut action)?;
                    visits.push(location::VisitIn { lat: place.lat_e6 as f64 / 1e6, lon: place.lon_e6 as f64 / 1e6, ..visit(start, 50) });
                }
                Drain { visits, ..Drain::default() }
            }
            _ => return Err(aroh_app::Error::Invalid("seed kind".into())),
        };
        location::drain(&mut c.conn, &mut c.clock, &c.user_id, &batch, now, zone)
    })?;
    lookups(&app, &report).await?;
    changed(&app).await;
    Ok(())
}
