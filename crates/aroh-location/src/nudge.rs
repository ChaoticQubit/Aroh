//! Nudges near places (Q56): for each place and each of the next days, the one nudge iOS may
//! show on arrival when a linked action is due that day or the newest one was missed.

use crate::linked;
use aroh_domain::rows::{Action, ActionStatus, Goal, Place, PlaceLink};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Days ahead a nudge plan covers, today included.
pub const PLAN_DAYS: i64 = 7;

/// Days back a missed action still earns a nudge.
pub const MISSED_DAYS: i64 = 7;

/// Seconds the owner must stay near a place before its nudge shows.
pub const DWELL_SECS: i64 = 120;

/// First and last local minute of the day a nudge may show; quiet hours are 22:00 to 06:00.
pub const WINDOW: (u32, u32) = (6 * 60, 22 * 60);

/// Why a place nudges on a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Due,
    Missed,
}

/// One planned nudge: shown on arrival at `place_id` on local day `date` when the dwell
/// ends inside `[from, until)`. `id` is the notification id, unique per place and day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Nudge {
    pub id: String,
    pub place_id: String,
    pub date: NaiveDate,
    pub action_id: String,
    pub reason: Reason,
    pub title: String,
    pub body: String,
    pub from: DateTime<Utc>,
    pub until: DateTime<Utc>,
}

/// The rows a nudge plan reads. `sent` holds the place and local day of every nudge already
/// shown; `kinds` maps a place id to its guessed kind ("gym"); `paused` is the pause switch.
pub struct NudgeIn<'a> {
    pub places: &'a [Place],
    pub links: &'a [PlaceLink],
    pub goals: &'a [Goal],
    pub actions: &'a [Action],
    pub sent: &'a BTreeSet<(String, NaiveDate)>,
    pub kinds: &'a BTreeMap<String, String>,
    pub paused: bool,
}

fn local_instant(zone: Tz, date: NaiveDate, minute: u32) -> DateTime<Utc> {
    let time = NaiveTime::from_hms_opt(minute / 60, minute % 60, 0).expect("a minute of the day");
    let naive = date.and_time(time);
    zone.from_local_datetime(&naive).earliest().unwrap_or_else(|| zone.from_utc_datetime(&naive)).with_timezone(&Utc)
}

fn is_fitness(goals: &[Goal], goal_id: &str) -> bool {
    goals.iter().any(|g| g.id == goal_id && g.category_id.as_deref() == Some(aroh_domain::derived_id(&g.user_id, "health-fitness", 0).as_str()))
}

fn text(input: &NudgeIn, place: &Place, action: &Action, reason: Reason, zone: Tz) -> (String, String) {
    let local = action.start_at.with_timezone(&zone);
    let title = match reason {
        Reason::Due => format!("{}, due {}", action.title, local.format("%H:%M")),
        Reason::Missed => format!("{}, missed {}", action.title, local.format("%a")),
    };
    let near = match input.kinds.get(&place.id) {
        Some(kind) => format!("Near your {kind}."),
        None => format!("Near {}.", place.name),
    };
    let body = if is_fitness(input.goals, &action.goal_id) {
        format!("{near} Quick workout to stay on track?")
    } else {
        format!("{near} Good time for {}?", action.title)
    };
    (title, body)
}

fn pick<'a>(input: &NudgeIn<'a>, place: &'a Place, date: NaiveDate, zone: Tz) -> Option<(&'a Action, Reason)> {
    let day = |a: &Action| a.start_at.with_timezone(&zone).date_naive();
    let mut mine: Vec<&Action> = linked(&place.id, input.links, input.actions).collect();
    mine.sort_by_key(|a| (a.start_at, a.id.clone()));
    if let Some(due) = mine.iter().find(|a| day(a) == date && a.status == ActionStatus::Planned) {
        return Some((due, Reason::Due));
    }
    let since = date - Duration::days(MISSED_DAYS);
    let newest = mine.iter().rev().find(|a| day(a) < date && day(a) >= since)?;
    (newest.status == ActionStatus::Missed).then_some((newest, Reason::Missed))
}

/// The nudges of every place in `input` for the [`PLAN_DAYS`] local days from `now` in
/// `zone`, in place and date order: per place and day the earliest planned linked action of
/// that day, else the newest linked action of the [`MISSED_DAYS`] before it when it was
/// missed. Days already in `sent`, and everything while paused, get none. Never moves an action.
pub fn nudge_plan(input: &NudgeIn, now: DateTime<Utc>, zone: Tz) -> Vec<Nudge> {
    if input.paused {
        return Vec::new();
    }
    let today = now.with_timezone(&zone).date_naive();
    let mut out = Vec::new();
    for place in input.places.iter().filter(|p| !p.deleted) {
        for offset in 0..PLAN_DAYS {
            let date = today + Duration::days(offset);
            if input.sent.contains(&(place.id.clone(), date)) {
                continue;
            }
            let Some((action, reason)) = pick(input, place, date, zone) else { continue };
            let (title, body) = text(input, place, action, reason, zone);
            let (from, until) = (local_instant(zone, date, WINDOW.0), local_instant(zone, date, WINDOW.1));
            assert!(from < until, "the nudge window is part of one day");
            out.push(Nudge { id: format!("aroh.nudge.{}.{date}", place.id), place_id: place.id.clone(), date, action_id: action.id.clone(), reason, title, body, from, until });
        }
    }
    out
}

/// When a nudge shows for an arrival at `entered`: [`DWELL_SECS`] later, if that instant is
/// inside the nudge's window. The iOS bridge applies the same rule to the stored plan.
pub fn fire_time(nudge: &Nudge, entered: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let fire = entered + Duration::seconds(DWELL_SECS);
    (fire >= nudge.from && fire < nudge.until).then_some(fire)
}
