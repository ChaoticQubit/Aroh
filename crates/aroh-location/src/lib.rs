//! Aroh location rules (Q56): pure functions over visits, places, links and actions that
//! decide place prompts, nudges, the watched set, learned patterns and routine streaks.
//! No I/O; the app layer reads the rows and the iOS bridge only senses and delivers.

pub mod geo;
pub mod nudge;
pub mod pattern;
pub mod prompt;
pub mod streak;
pub mod watch;

use aroh_domain::rows::{Action, PlaceLink};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One raw visit from the phone's local log: center in microdegrees, arrival and, once the
/// owner left, departure. `place_id` is the named place it fell inside, if any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Visit {
    pub id: String,
    pub lat_e6: i64,
    pub lon_e6: i64,
    pub arrived_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
    pub place_id: Option<String>,
}

/// Whether `action` belongs to `link`: the link's series when it names one, else any
/// action of the link's goal.
pub fn links_to(link: &PlaceLink, action: &Action) -> bool {
    match &link.series_id {
        Some(series) => action.series_id.as_deref() == Some(series.as_str()),
        None => action.goal_id == link.goal_id,
    }
}

/// The live actions linked to place `place_id` through any of `links`, each once.
pub fn linked<'a>(place_id: &'a str, links: &'a [PlaceLink], actions: &'a [Action]) -> impl Iterator<Item = &'a Action> + 'a {
    actions
        .iter()
        .filter(move |a| !a.deleted && links.iter().any(|l| !l.deleted && l.place_id == place_id && links_to(l, a)))
}
