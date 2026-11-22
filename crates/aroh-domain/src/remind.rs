//! Reminder planning shared by the app engine (local notifications, Q22 offline fallback)
//! and the server engine (APNs pushes): the MVP reminder timing over the rebuild actions,
//! plus one reminder per tracker check-in time. Pure, so both sides get the same ids.

use crate::rows::{Action, ActionStatus, CadenceFreq, Goal, Tracker};
use aroh_reminders::{plan_reminders, ActionSlot, ReminderSettings};
pub use aroh_reminders::{reminder_id, ReminderKind};
use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use std::collections::HashMap;

/// Most notifications handed to the OS at once (iOS keeps at most 64 pending).
pub const CAP: usize = 60;

/// Hours ahead action and check-in reminders are planned (Q22: the next 24 h).
pub const HORIZON_HOURS: i64 = 24;

const SETTINGS: ReminderSettings = ReminderSettings { enabled: true, lead_min: 10, missed_grace_min: 15 };
const TEXT_MAX_CHARS: usize = 120;

/// What a planned notification is about: an action (soon, start or nudge) or a tracker check-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedKind {
    Action,
    CheckIn,
}

/// One notification to schedule or push: stable id, fire time, text, what it is about and
/// the id of that action or tracker row (`row_id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub id: i32,
    pub fire_at: DateTime<Utc>,
    pub title: String,
    pub body: String,
    pub kind: PlannedKind,
    pub row_id: String,
}

/// Cuts `text` to the 120 characters a notification shows, ending with an ellipsis when cut.
/// Shared by local reminders and server pushes so both read the same.
pub fn truncate(text: String) -> String {
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
        (due && fire > now && fire <= now + Duration::hours(HORIZON_HOURS)).then_some(fire)
    })
}

/// Plans the notifications after `now` from live rows: soon, start and nudge for planned
/// actions and the next check-in of each tracker with a cadence, all within
/// [`HORIZON_HOURS`] (local times in `zone`). Output: at most [`CAP`], earliest first.
pub fn plan(goals: &[Goal], actions: &[Action], trackers: &[Tracker], now: DateTime<Utc>, zone: Tz) -> Vec<Planned> {
    let titles: HashMap<&str, &str> = goals.iter().filter(|g| !g.deleted).map(|g| (g.id.as_str(), g.title.as_str())).collect();
    let goal_title = |id: &str| truncate(titles.get(id).copied().unwrap_or_default().to_string());
    let until = now + Duration::hours(HORIZON_HOURS) + Duration::minutes(i64::from(SETTINGS.lead_min));
    let live: Vec<&Action> = actions
        .iter()
        .filter(|a| !a.deleted && a.status == ActionStatus::Planned)
        .filter(|a| a.end_at + Duration::minutes(i64::from(SETTINGS.missed_grace_min)) > now && a.start_at < until)
        .collect();
    let slots: Vec<ActionSlot> = live
        .iter()
        .map(|a| ActionSlot { id: a.id.clone(), start_at: a.start_at, end_at: a.end_at, status: aroh_reminders::ActionStatus::Planned })
        .collect();
    let by_id: HashMap<&str, &Action> = live.iter().map(|a| (a.id.as_str(), *a)).collect();
    let mut out: Vec<Planned> = plan_reminders(&slots, &SETTINGS, now, Duration::hours(HORIZON_HOURS), CAP)
        .into_iter()
        .map(|r| {
            let action = by_id[r.action_id.as_str()];
            let title = match r.kind {
                ReminderKind::Soon => format!("In {} min: {}", SETTINGS.lead_min, action.title),
                ReminderKind::Start => format!("Now: {}", action.title),
                ReminderKind::Nudge => format!("Did you finish {}?", action.title),
            };
            Planned { id: r.id, fire_at: r.fire_at, title: truncate(title), body: goal_title(&action.goal_id), kind: PlannedKind::Action, row_id: r.action_id }
        })
        .collect();
    for tracker in trackers.iter().filter(|t| !t.deleted) {
        if let Some(fire_at) = next_check_in(tracker, now, zone) {
            out.push(Planned {
                id: reminder_id(&tracker.id, ReminderKind::Start),
                fire_at,
                title: truncate(format!("Check-in: {}", tracker.name)),
                body: goal_title(&tracker.goal_id),
                kind: PlannedKind::CheckIn,
                row_id: tracker.id.clone(),
            });
        }
    }
    out.sort_by(|a, b| a.fire_at.cmp(&b.fire_at).then(a.id.cmp(&b.id)));
    out.truncate(CAP);
    assert!(out.windows(2).all(|w| w[0].fire_at <= w[1].fire_at));
    assert!(out.iter().all(|p| p.fire_at > now && p.fire_at <= now + Duration::hours(HORIZON_HOURS)));
    out
}

/// How far the device's local schedule reaches, reported to the server so it pushes only
/// what the device will not show itself. `granted` is the notification permission.
/// Output: `now` when nothing is scheduled locally, the last fire time when the plan hit
/// [`CAP`], else `now` plus [`HORIZON_HOURS`].
pub fn scheduled_until(planned: &[Planned], now: DateTime<Utc>, granted: bool) -> DateTime<Utc> {
    if !granted {
        return now;
    }
    match planned.last() {
        Some(last) if planned.len() >= CAP => last.fire_at,
        _ => now + Duration::hours(HORIZON_HOURS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rows::samples;
    use crate::rows::{Cadence, CadenceFreq};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 21, 8, 0, 0).unwrap()
    }

    fn action_at(n: u8, hours: i64) -> Action {
        let mut a = samples::action();
        a.id = format!("0190f2a0-0000-7000-8000-0000000001{n:02}");
        a.start_at = now() + Duration::hours(hours);
        a.end_at = a.start_at + Duration::minutes(30);
        a
    }

    #[test]
    fn plans_actions_and_check_ins_within_24_hours() {
        let goal = samples::goal();
        let mut tracker = samples::tracker();
        tracker.cadence = Some(Cadence { freq: CadenceFreq::Daily, weekdays: vec![], at: "20:00".into() });
        let soon = action_at(1, 2);
        let late = action_at(2, 30);
        let planned = plan(&[goal.clone()], &[soon.clone(), late], &[tracker.clone()], now(), Tz::UTC);
        let for_soon: Vec<&Planned> = planned.iter().filter(|p| p.row_id == soon.id).collect();
        assert_eq!(for_soon.len(), 3);
        assert!(for_soon.iter().all(|p| p.kind == PlannedKind::Action && p.body == goal.title));
        assert_eq!(for_soon[1].id, reminder_id(&soon.id, ReminderKind::Start));
        assert_eq!(for_soon[1].title, format!("Now: {}", soon.title));
        let check_in: Vec<&Planned> = planned.iter().filter(|p| p.kind == PlannedKind::CheckIn).collect();
        assert_eq!(check_in.len(), 1);
        assert_eq!(check_in[0].row_id, tracker.id);
        assert_eq!(check_in[0].fire_at, Utc.with_ymd_and_hms(2026, 11, 21, 20, 0, 0).unwrap());
        assert_eq!(planned.len(), 4);
    }

    #[test]
    fn deleted_and_done_rows_get_nothing() {
        let mut gone = action_at(1, 2);
        gone.deleted = true;
        let mut done = action_at(2, 3);
        done.status = ActionStatus::Done;
        done.done_at = Some(now());
        assert!(plan(&[], &[gone, done], &[], now(), Tz::UTC).is_empty());
    }

    #[test]
    fn scheduled_until_covers_permission_cap_and_horizon() {
        let planned = plan(&[], &[action_at(1, 2)], &[], now(), Tz::UTC);
        assert_eq!(scheduled_until(&planned, now(), false), now());
        assert_eq!(scheduled_until(&planned, now(), true), now() + Duration::hours(24));
        let many: Vec<Action> = (0..30).map(|n| action_at(n, 1 + i64::from(n) / 3)).collect();
        let full = plan(&[], &many, &[], now(), Tz::UTC);
        assert_eq!(full.len(), CAP);
        assert_eq!(scheduled_until(&full, now(), true), full[CAP - 1].fire_at);
    }
}
