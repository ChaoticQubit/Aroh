//! Local notification planning (Q22 offline fallback): the MVP reminder timing over the
//! rebuild actions, plus one reminder per tracker check-in time.

use crate::Result;
use aroh_domain::rows::{Action, ActionStatus, CadenceFreq, Goal, Tracker};
use aroh_reminders::{plan_reminders, reminder_id, ActionSlot, ReminderKind, ReminderSettings};
use aroh_store::repo;
use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::Connection;
use std::collections::HashMap;

/// Most notifications handed to the OS at once (iOS keeps at most 64 pending).
pub const CAP: usize = 60;

/// Hours ahead action reminders are planned.
pub const HORIZON_HOURS: i64 = 48;

/// Hours ahead check-in reminders are planned.
pub const CHECK_IN_HOURS: i64 = 24;

const SETTINGS: ReminderSettings = ReminderSettings { enabled: true, lead_min: 10, missed_grace_min: 15 };
const TEXT_MAX_CHARS: usize = 120;

/// One notification to schedule: stable id, fire time and text. `action_id` is set for
/// action reminders and empty for check-ins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub id: i32,
    pub fire_at: DateTime<Utc>,
    pub title: String,
    pub body: String,
    pub action_id: Option<String>,
}

fn truncate(text: String) -> String {
    if text.chars().count() <= TEXT_MAX_CHARS {
        return text;
    }
    let mut cut: String = text.chars().take(TEXT_MAX_CHARS - 1).collect();
    cut.push('…');
    cut
}

fn next_check_in(tracker: &Tracker, now: DateTime<Utc>, zone: Tz) -> Option<DateTime<Utc>> {
    let cadence = tracker.cadence.as_ref()?;
    let at = NaiveTime::parse_from_str(&cadence.at, "%H:%M").ok()?;
    let today = now.with_timezone(&zone).date_naive();
    (0..=1).find_map(|offset| {
        let date = today + Duration::days(offset);
        let due = cadence.freq == CadenceFreq::Daily || cadence.weekdays.contains(&date.weekday());
        let fire = zone.from_local_datetime(&date.and_time(at)).earliest()?.with_timezone(&Utc);
        (due && fire > now && fire <= now + Duration::hours(CHECK_IN_HOURS)).then_some(fire)
    })
}

/// Plans the notifications of `user_id` after `now`: soon, start and nudge for planned actions
/// in the next 48 h, and the next check-in of each tracker with a cadence in the next 24 h
/// (local times in `zone`). Output: at most [`CAP`] reminders, earliest first.
pub fn plan(conn: &Connection, user_id: &str, now: DateTime<Utc>, zone: Tz) -> Result<Vec<Planned>> {
    let goals: HashMap<String, String> = repo::list::<Goal>(conn, &[("user_id", user_id)])?.into_iter().map(|g| (g.id, g.title)).collect();
    let until = now + Duration::hours(HORIZON_HOURS) + Duration::minutes(i64::from(SETTINGS.lead_min));
    let actions: Vec<Action> = repo::list::<Action>(conn, &[("user_id", user_id), ("status", "planned")])?
        .into_iter()
        .filter(|a| a.end_at + Duration::minutes(i64::from(SETTINGS.missed_grace_min)) > now && a.start_at < until)
        .collect();
    let slots: Vec<ActionSlot> = actions
        .iter()
        .map(|a| ActionSlot { id: a.id.clone(), start_at: a.start_at, end_at: a.end_at, status: aroh_reminders::ActionStatus::Planned })
        .collect();
    let by_id: HashMap<&str, &Action> = actions.iter().map(|a| (a.id.as_str(), a)).collect();
    let mut out: Vec<Planned> = plan_reminders(&slots, &SETTINGS, now, Duration::hours(HORIZON_HOURS), CAP)
        .into_iter()
        .map(|r| {
            let action = by_id[r.action_id.as_str()];
            assert_eq!(action.status, ActionStatus::Planned);
            let title = match r.kind {
                ReminderKind::Soon => format!("In {} min: {}", SETTINGS.lead_min, action.title),
                ReminderKind::Start => format!("Now: {}", action.title),
                ReminderKind::Nudge => format!("Did you finish {}?", action.title),
            };
            Planned { id: r.id, fire_at: r.fire_at, title: truncate(title), body: truncate(goals.get(&action.goal_id).cloned().unwrap_or_default()), action_id: Some(r.action_id) }
        })
        .collect();
    for tracker in repo::list::<Tracker>(conn, &[("user_id", user_id)])? {
        if let Some(fire_at) = next_check_in(&tracker, now, zone) {
            out.push(Planned {
                id: reminder_id(&tracker.id, ReminderKind::Start),
                fire_at,
                title: truncate(format!("Check-in: {}", tracker.name)),
                body: truncate(goals.get(&tracker.goal_id).cloned().unwrap_or_default()),
                action_id: None,
            });
        }
    }
    out.sort_by(|a, b| a.fire_at.cmp(&b.fire_at).then(a.id.cmp(&b.id)));
    out.truncate(CAP);
    assert!(out.windows(2).all(|w| w[0].fire_at <= w[1].fire_at));
    Ok(out)
}

/// Every notification id any action or tracker row (live or deleted) could own, so a
/// reschedule cancels stale ones without reading the OS pending list.
pub fn all_ids(conn: &Connection) -> Result<Vec<i32>> {
    let mut ids = Vec::new();
    let kinds = [ReminderKind::Soon, ReminderKind::Start, ReminderKind::Nudge];
    for table in ["actions", "trackers"] {
        let mut stmt = conn.prepare(&format!("SELECT id FROM {table}"))?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        ids.extend(rows.iter().flat_map(|id| kinds.map(|k| reminder_id(id, k))));
    }
    Ok(ids)
}
