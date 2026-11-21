use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionStatus {
    Planned,
    Done,
    Skipped,
    Missed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSlot {
    pub id: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub status: ActionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReminderSettings {
    pub enabled: bool,
    pub lead_min: u32,
    pub missed_grace_min: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReminderKind {
    Soon,
    Start,
    Nudge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reminder {
    pub id: i32,
    pub action_id: String,
    pub kind: ReminderKind,
    pub fire_at: DateTime<Utc>,
}

/// Plans the local notifications for `actions` that fire after `now` and no later than `now + horizon`.
/// Only planned actions get reminders; `Soon` is dropped when the lead is 0.
/// Returns at most `cap` reminders sorted by fire time, earliest first.
pub fn plan_reminders(
    actions: &[ActionSlot],
    settings: &ReminderSettings,
    now: DateTime<Utc>,
    horizon: Duration,
    cap: usize,
) -> Vec<Reminder> {
    if !settings.enabled || cap == 0 {
        return Vec::new();
    }
    let lead = Duration::minutes(i64::from(settings.lead_min.min(LEAD_MAX_MIN)));
    let grace = Duration::minutes(i64::from(settings.missed_grace_min.min(GRACE_MAX_MIN)));
    let until = now + horizon;
    let mut out: Vec<Reminder> = Vec::new();
    for action in actions.iter().filter(|a| a.status == ActionStatus::Planned) {
        let mut candidates = vec![
            (ReminderKind::Start, action.start_at),
            (ReminderKind::Nudge, action.end_at + grace),
        ];
        if lead > Duration::zero() {
            candidates.push((ReminderKind::Soon, action.start_at - lead));
        }
        for (kind, fire_at) in candidates {
            if fire_at > now && fire_at <= until {
                out.push(Reminder {
                    id: reminder_id(&action.id, kind),
                    action_id: action.id.clone(),
                    kind,
                    fire_at,
                });
            }
        }
    }
    out.sort_by(|a, b| a.fire_at.cmp(&b.fire_at).then(a.id.cmp(&b.id)));
    out.truncate(cap);
    assert!(out.len() <= cap);
    assert!(out.windows(2).all(|w| w[0].fire_at <= w[1].fire_at));
    assert!(out.iter().all(|r| r.fire_at > now && r.fire_at <= until && r.id >= 0 && (r.id as u32) < REMINDER_ID_LIMIT));
    out
}

/// Stable notification id for one reminder of one action (FNV-1a 32 over id and kind, modulo `REMINDER_ID_LIMIT`).
/// The same inputs give the same id on every run and device, so a reschedule replaces.
/// Returns an `i32` in `0..REMINDER_ID_LIMIT`; ids at or above the limit belong to other features.
pub fn reminder_id(action_id: &str, kind: ReminderKind) -> i32 {
    let tag: u8 = match kind {
        ReminderKind::Soon => b'S',
        ReminderKind::Start => b'T',
        ReminderKind::Nudge => b'N',
    };
    let mut hash: u32 = 0x811c_9dc5;
    for byte in action_id.bytes().chain([0u8, tag]) {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    let id = (hash % REMINDER_ID_LIMIT) as i32;
    assert!((0..REMINDER_ID_LIMIT as i32).contains(&id));
    id
}

/// UTC bounds `[start, end)` of the local calendar day `date` in `tz`.
/// A DST day is 23 or 25 hours long; a midnight that does not exist starts at the first valid instant.
/// Returns the pair (local day start, next local day start) in UTC.
pub fn local_day_bounds(date: NaiveDate, tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
    let next = date.succ_opt().expect("date before NaiveDate::MAX");
    let from = local_start(date, tz);
    let to = local_start(next, tz);
    assert!(to > from);
    (from, to)
}

/// Calendar reminder ids stay below this; other notification owners use ids at or above it.
pub const REMINDER_ID_LIMIT: u32 = 2_000_000_000;

const LEAD_MAX_MIN: u32 = 120;
const GRACE_MAX_MIN: u32 = 240;

fn local_start(date: NaiveDate, tz: Tz) -> DateTime<Utc> {
    let midnight = date.and_hms_opt(0, 0, 0).expect("midnight is valid");
    for minutes in (0..=180).step_by(15) {
        let local = midnight + Duration::minutes(minutes);
        if let Some(t) = tz.from_local_datetime(&local).earliest() {
            return t.with_timezone(&Utc);
        }
    }
    unreachable!("no valid local time in the first 3 hours of {date} in {tz}")
}
