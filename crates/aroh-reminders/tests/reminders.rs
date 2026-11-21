use aroh_reminders::{
    local_day_bounds, plan_reminders, reminder_id, ActionSlot, ActionStatus, Reminder, ReminderKind,
    ReminderSettings,
};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn slot(id: &str, start: DateTime<Utc>, minutes: i64, status: ActionStatus) -> ActionSlot {
    ActionSlot {
        id: id.to_string(),
        start_at: start,
        end_at: start + Duration::minutes(minutes),
        status,
    }
}

fn settings(lead_min: u32) -> ReminderSettings {
    ReminderSettings {
        enabled: true,
        lead_min,
        missed_grace_min: 15,
    }
}

fn plan(actions: &[ActionSlot], s: &ReminderSettings, now: DateTime<Utc>) -> Vec<Reminder> {
    plan_reminders(actions, s, now, Duration::hours(48), 60)
}

fn kinds_at(out: &[Reminder]) -> Vec<(ReminderKind, DateTime<Utc>)> {
    out.iter().map(|r| (r.kind, r.fire_at)).collect()
}

#[test]
fn lead_time_gives_soon_start_and_nudge() {
    let a = [slot("a1", at("2026-11-20T09:00:00Z"), 45, ActionStatus::Planned)];
    let now = at("2026-11-20T06:00:00Z");
    let out = plan(&a, &settings(10), now);
    assert_eq!(
        kinds_at(&out),
        vec![
            (ReminderKind::Soon, at("2026-11-20T08:50:00Z")),
            (ReminderKind::Start, at("2026-11-20T09:00:00Z")),
            (ReminderKind::Nudge, at("2026-11-20T10:00:00Z")),
        ]
    );
    let zero = plan(&a, &settings(0), now);
    assert_eq!(
        kinds_at(&zero),
        vec![
            (ReminderKind::Start, at("2026-11-20T09:00:00Z")),
            (ReminderKind::Nudge, at("2026-11-20T10:00:00Z")),
        ]
    );
}

#[test]
fn past_fire_times_are_dropped() {
    let a = [slot("a1", at("2026-11-20T09:00:00Z"), 45, ActionStatus::Planned)];
    let out = plan(&a, &settings(10), at("2026-11-20T08:55:00Z"));
    assert_eq!(
        kinds_at(&out),
        vec![
            (ReminderKind::Start, at("2026-11-20T09:00:00Z")),
            (ReminderKind::Nudge, at("2026-11-20T10:00:00Z")),
        ]
    );
    let at_start = plan(&a, &settings(10), at("2026-11-20T09:00:00Z"));
    assert_eq!(
        kinds_at(&at_start),
        vec![(ReminderKind::Nudge, at("2026-11-20T10:00:00Z"))]
    );
}

#[test]
fn dst_day_in_london_is_23_hours_and_local_nine_maps_right() {
    let tz = chrono_tz::Europe::London;
    let day = NaiveDate::from_ymd_opt(2026, 3, 29).unwrap();
    let (from, to) = local_day_bounds(day, tz);
    assert_eq!(from, at("2026-03-29T00:00:00Z"));
    assert_eq!(to, at("2026-03-29T23:00:00Z"));
    assert_eq!(to - from, Duration::hours(23));

    let nine = tz
        .from_local_datetime(&day.and_time(NaiveTime::from_hms_opt(9, 0, 0).unwrap()))
        .single()
        .unwrap()
        .with_timezone(&Utc);
    assert_eq!(nine, at("2026-03-29T08:00:00Z"));
    let a = [slot("a1", nine, 30, ActionStatus::Planned)];
    let out = plan(&a, &settings(10), at("2026-03-29T00:00:00Z"));
    let start = out.iter().find(|r| r.kind == ReminderKind::Start).unwrap();
    assert_eq!(start.fire_at, at("2026-03-29T08:00:00Z"));

    let winter = local_day_bounds(NaiveDate::from_ymd_opt(2026, 10, 25).unwrap(), tz);
    assert_eq!(winter.1 - winter.0, Duration::hours(25));
}

#[test]
fn reschedule_after_move_keeps_ids_and_drops_old_times() {
    let now = at("2026-11-20T06:00:00Z");
    let before = plan(
        &[slot("a1", at("2026-11-20T09:00:00Z"), 45, ActionStatus::Planned)],
        &settings(10),
        now,
    );
    let after = plan(
        &[slot("a1", at("2026-11-20T11:00:00Z"), 45, ActionStatus::Planned)],
        &settings(10),
        now,
    );
    assert_eq!(before.len(), 3);
    assert_eq!(after.len(), 3);
    for (b, a) in before.iter().zip(after.iter()) {
        assert_eq!(b.id, a.id);
        assert_eq!(b.kind, a.kind);
        assert_eq!(a.fire_at, b.fire_at + Duration::hours(2));
    }
    for b in &before {
        assert!(after.iter().all(|a| a.fire_at != b.fire_at));
    }
    assert_eq!(before[0].id, reminder_id("a1", ReminderKind::Soon));
    assert_ne!(
        reminder_id("a1", ReminderKind::Soon),
        reminder_id("a1", ReminderKind::Start)
    );
    assert!(before.iter().all(|r| r.id >= 0 && (r.id as u32) < aroh_reminders::REMINDER_ID_LIMIT));
}

#[test]
fn only_planned_actions_get_reminders() {
    let start = at("2026-11-20T09:00:00Z");
    let now = at("2026-11-20T06:00:00Z");
    for status in [ActionStatus::Done, ActionStatus::Skipped, ActionStatus::Missed] {
        assert!(plan(&[slot("a1", start, 30, status)], &settings(10), now).is_empty());
    }
    let moved_back = plan(&[slot("a1", start, 30, ActionStatus::Planned)], &settings(10), now);
    assert_eq!(moved_back.len(), 3);
}

#[test]
fn cap_and_horizon_keep_earliest_sixty_within_48h() {
    let now = at("2026-11-20T00:00:00Z");
    let actions: Vec<ActionSlot> = (0..100)
        .map(|i| {
            slot(
                &format!("a{i}"),
                now + Duration::hours(1) + Duration::minutes(40 * i),
                30,
                ActionStatus::Planned,
            )
        })
        .collect();
    let out = plan(&actions, &settings(10), now);
    assert_eq!(out.len(), 60);
    assert!(out.iter().all(|r| r.fire_at > now && r.fire_at <= now + Duration::hours(48)));
    assert!(out.windows(2).all(|w| w[0].fire_at <= w[1].fire_at));
    assert_eq!(out[0].fire_at, now + Duration::minutes(50));

    let far = [slot("far", now + Duration::hours(60), 30, ActionStatus::Planned)];
    assert!(plan(&far, &settings(10), now).is_empty());

    let off = ReminderSettings {
        enabled: false,
        ..settings(10)
    };
    assert!(plan(&actions, &off, now).is_empty());
}
