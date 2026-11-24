//! The watched set: iOS monitors at most 20 places at once, so Aroh picks the most relevant.

use crate::linked;
use aroh_domain::rows::{Action, ActionStatus, Place, PlaceLink};
use chrono::{DateTime, Duration, Utc};
use std::collections::BTreeMap;

/// Most places iOS lets one app monitor at once.
pub const MAX_WATCHED: usize = 20;

/// Days ahead a linked action makes its place urgent.
pub const SOON_DAYS: i64 = 7;

/// The ids of at most [`MAX_WATCHED`] live places to monitor, most relevant first: places with
/// a planned linked action in the next [`SOON_DAYS`] days (soonest first), then by number of
/// links, then by `visits` (visit counts by place id), then by id.
pub fn choose_watched(places: &[Place], links: &[PlaceLink], actions: &[Action], visits: &BTreeMap<String, usize>, now: DateTime<Utc>) -> Vec<String> {
    let horizon = now + Duration::days(SOON_DAYS);
    let mut ranked: Vec<(Option<DateTime<Utc>>, usize, usize, &str)> = places
        .iter()
        .filter(|p| !p.deleted)
        .map(|p| {
            let soonest = linked(&p.id, links, actions).filter(|a| a.status == ActionStatus::Planned && a.end_at > now && a.start_at < horizon).map(|a| a.start_at).min();
            let count = links.iter().filter(|l| !l.deleted && l.place_id == p.id).count();
            (soonest, count, visits.get(&p.id).copied().unwrap_or(0), p.id.as_str())
        })
        .collect();
    ranked.sort_by(|a, b| match (a.0, b.0) {
        (Some(x), Some(y)) => x.cmp(&y).then(a.3.cmp(b.3)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(a.3.cmp(b.3)),
    });
    let out: Vec<String> = ranked.into_iter().take(MAX_WATCHED).map(|r| r.3.to_string()).collect();
    assert!(out.len() <= MAX_WATCHED);
    out
}
