//! Following the device time zone (Q54): on start and resume the app compares the OS
//! zone with the zone the rows were last shifted to and, on a change, writes the shift,
//! one Inbox note and the new zone in one transaction.

use crate::{local, Result};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{Action, ActionSeries, BusyBlock};
use aroh_domain::travel::{plan_shift, Line, Outcome};
use aroh_store::repo;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use rusqlite::Connection;

/// Title of the Inbox note a zone change writes.
pub const NOTE_TITLE: &str = "Time zone changed";

/// Most actions one list of the note names before "and N more".
pub const NOTE_LIST_MAX: usize = 10;

/// What one [`follow`] call did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Follow {
    Unknown,
    Recorded,
    Same,
    Shifted { moved: usize, fixed: usize, left: usize },
}

/// Follows the device zone `zone` for `user_id` at `now`. `None` (the OS reported no
/// known zone) does nothing; the first known zone is only recorded; a changed zone
/// shifts the rows (see [`aroh_domain::travel::plan_shift`]), adds one note and stores
/// the zone with the pending flag for the engine, all in one transaction.
pub fn follow(conn: &Connection, clock: &mut Hlc, user_id: &str, now: DateTime<Utc>, zone: Option<Tz>) -> Result<Follow> {
    let Some(to) = zone else { return Ok(Follow::Unknown) };
    let from = match local::zone(conn)?.and_then(|z| z.parse::<Tz>().ok()) {
        None => {
            local::set_zone(conn, to.name(), false)?;
            return Ok(Follow::Recorded);
        }
        Some(from) if from == to => return Ok(Follow::Same),
        Some(from) => from,
    };
    let mine = [("user_id", user_id)];
    let actions = repo::list::<Action>(conn, &mine)?;
    let series = repo::list::<ActionSeries>(conn, &mine)?;
    let busy = repo::list::<BusyBlock>(conn, &mine)?;
    let shift = plan_shift(from, to, now, &actions, &series, &busy);

    let tx = conn.unchecked_transaction()?;
    for id in &shift.tombstones {
        repo::delete::<Action>(&tx, clock, id)?;
    }
    for row in &shift.writes {
        repo::put(&tx, clock, &mut row.clone())?;
    }
    for row in &shift.series {
        repo::put(&tx, clock, &mut row.clone())?;
    }
    local::add_note(&tx, NOTE_TITLE, &note_body(from, to, &shift.lines), now)?;
    local::set_zone(&tx, to.name(), true)?;
    tx.commit()?;

    let count = |f: fn(&Outcome) -> bool| shift.lines.iter().filter(|l| f(&l.outcome)).count();
    Ok(Follow::Shifted {
        moved: count(|o| *o == Outcome::Moved),
        fixed: count(|o| *o == Outcome::Fixed),
        left: count(|o| !matches!(o, Outcome::Moved | Outcome::Fixed)),
    })
}

fn listed(head: &str, items: Vec<String>) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    let more = items.len().saturating_sub(NOTE_LIST_MAX);
    let mut shown: Vec<String> = items.into_iter().take(NOTE_LIST_MAX).collect();
    if more > 0 {
        shown.push(format!("and {more} more"));
    }
    Some(format!("{head}: {}.", shown.join(", ")))
}

/// The note body: the new and old zone, the actions moved and the fixed actions kept
/// (title and local time in `to`), and the actions not moved with the reason.
/// Each list names at most [`NOTE_LIST_MAX`] actions.
pub fn note_body(from: Tz, to: Tz, lines: &[Line]) -> String {
    let when = |l: &Line| l.start_at.with_timezone(&to).format("%a %H:%M").to_string();
    let pick = |want: fn(&Outcome) -> bool| -> Vec<&Line> { lines.iter().filter(|l| want(&l.outcome)).collect() };
    let moved = pick(|o| *o == Outcome::Moved).into_iter().map(|l| format!("{} {}", l.title, when(l))).collect();
    let fixed = pick(|o| *o == Outcome::Fixed).into_iter().map(|l| format!("{} {}", l.title, when(l))).collect();
    let left = pick(|o| !matches!(o, Outcome::Moved | Outcome::Fixed))
        .into_iter()
        .map(|l| match &l.outcome {
            Outcome::Clash(with) => format!("{} (clashes with {with})", l.title),
            Outcome::Past => format!("{} (would be in the past)", l.title),
            _ => format!("{} (its new slot is taken)", l.title),
        })
        .collect();
    let mut parts = vec![format!("Now in {}, was {}.", to.name(), from.name())];
    parts.extend(listed("Moved to the same local time", moved));
    parts.extend(listed("Fixed, kept at their time", fixed));
    parts.extend(listed("Not moved", left));
    if parts.len() == 1 {
        parts.push("Nothing needed to move.".to_string());
    }
    parts.join(" ")
}
