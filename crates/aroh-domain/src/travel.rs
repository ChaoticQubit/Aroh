//! Travel (Q54): when the owner's time zone changes, flexible actions and every series
//! follow local wall time in the new zone, locked actions and busy blocks keep their real
//! instant, and a move into a clash or into the past is not made (Q53).

use crate::recur::{self, occurrence_id};
use crate::rows::{Action, ActionSeries, ActionStatus, BusyBlock};
use chrono::{DateTime, Duration, LocalResult, NaiveDateTime, Offset, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const MINUTES_PER_DAY: i64 = 24 * 60;

/// The instant that shows `local` on the wall clock of `zone`. An ambiguous time (a
/// repeated hour) takes the earlier instant; a time inside a gap takes the first valid
/// minute after the gap. Input: an IANA zone and a local date and time.
/// Output: the UTC instant.
pub fn wall_instant(zone: Tz, local: NaiveDateTime) -> DateTime<Utc> {
    for minute in 0..=MINUTES_PER_DAY {
        let candidate = if minute == 0 { local } else { (local + Duration::minutes(minute)).with_second(0).expect("second 0 exists") };
        match zone.from_local_datetime(&candidate) {
            LocalResult::Single(t) => return t.with_timezone(&Utc),
            LocalResult::Ambiguous(early, _) => return early.with_timezone(&Utc),
            LocalResult::None => {}
        }
    }
    unreachable!("no zone has a gap longer than a day")
}

/// The instant the recurrence engine ([`recur::materialize`]) gives local time `local`
/// in `zone`: as [`wall_instant`], except a time inside a gap is read with the offset
/// before the gap, so it lands the gap's length later (02:30 on a one-hour gap is 03:30).
/// Series occurrences use it so re-keyed rows match what every engine derives.
pub fn rule_instant(zone: Tz, local: NaiveDateTime) -> DateTime<Utc> {
    let first = wall_instant(zone, local);
    if !matches!(zone.from_local_datetime(&local), LocalResult::None) {
        return first;
    }
    let before = zone.offset_from_utc_datetime(&(first - Duration::seconds(1)).naive_utc()).fix();
    let instant = Utc.from_utc_datetime(&(local - Duration::seconds(i64::from(before.local_minus_utc()))));
    assert!(instant >= first, "a gap time never lands before the gap ends");
    instant
}

/// What happened to one action in a shift, for the Inbox note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "with", rename_all = "snake_case")]
pub enum Outcome {
    Moved,
    Fixed,
    Clash(String),
    Past,
    Taken,
    Retimed,
    Kept,
}

/// What [`settle`] writes: occurrence rows to tombstone and derived rows to insert
/// (insert-or-ignore) in their place.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settle {
    pub tombstones: Vec<String>,
    pub inserts: Vec<Action>,
}

/// Plans the clean-up of untouched occurrence rows (planned, unlocked, derived stamp,
/// occurrence at or after `now`) that are not on their series' current rule, such as a
/// row an engine materialized in the old zone and the device pulled after the shift.
/// Each is tombstoned; with the `previous` zone its slot's derived row in the series'
/// zone is inserted unless a live row holds it. Output: the [`Settle`], ids in id order.
pub fn settle(previous: Option<Tz>, now: DateTime<Utc>, actions: &[Action], series: &[ActionSeries]) -> Settle {
    assert!(actions.iter().all(|a| !a.deleted) && series.iter().all(|s| !s.deleted), "settle takes live rows");
    let by_id: BTreeMap<&str, &ActionSeries> = series.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut held: BTreeSet<(String, DateTime<Utc>)> =
        actions.iter().filter_map(|a| Some((a.series_id.clone()?, a.occurrence?))).collect();
    let mut plan = Settle::default();
    for a in actions.iter().filter(|a| a.status == ActionStatus::Planned && !a.locked && a.updated_at == crate::seed::SEED_STAMP) {
        let (Some(id), Some(at)) = (a.series_id.as_deref(), a.occurrence) else { continue };
        let Some(s) = by_id.get(id).copied() else { continue };
        if at < now || !matches!(recur::is_occurrence(s, at), Ok(false)) {
            continue;
        }
        plan.tombstones.push(a.id.clone());
        let (Some(old_zone), Ok(zone)) = (previous, s.timezone.parse::<Tz>()) else { continue };
        let target = rule_instant(zone, at.with_timezone(&old_zone).naive_local());
        if target >= now && matches!(recur::is_occurrence(s, target), Ok(true)) && held.insert((s.id.clone(), target)) {
            plan.inserts.push(recur::occurrence(s, target));
        }
    }
    plan.tombstones.sort();
    plan.inserts.sort_by(|a, b| a.id.cmp(&b.id));
    assert!(plan.inserts.len() <= plan.tombstones.len());
    plan
}

/// `series` moved to zone `to` with the same local start. A start inside a gap of `to`
/// takes the time occurrence expansion gives it ([`rule_instant`]), and the rule then
/// names the old hour, minute and second so later occurrences keep their wall time. Output: the moved series
/// and whether its start changed, or `None` when the moved series does not validate.
fn relocate(series: &ActionSeries, to: Tz) -> Option<(ActionSeries, bool)> {
    let mut moved = ActionSeries { timezone: to.name().to_string(), ..series.clone() };
    let local = NaiveDateTime::parse_from_str(&series.dtstart_local, recur::LOCAL_FORMAT).ok()?;
    let start = rule_instant(to, local).with_timezone(&to).naive_local();
    let retimed = start != local;
    if retimed {
        moved.dtstart_local = start.format(recur::LOCAL_FORMAT).to_string();
        let keys: BTreeSet<String> =
            series.rrule.split(';').filter_map(|p| p.split('=').next()).map(|k| k.trim().to_ascii_uppercase()).collect();
        for (key, value) in [("BYHOUR", local.hour()), ("BYMINUTE", local.minute()), ("BYSECOND", local.second())] {
            if !keys.contains(key) {
                moved.rrule = format!("{};{key}={value}", moved.rrule.trim_end_matches(';'));
            }
        }
    }
    crate::rows::Record::validate(&moved).ok()?;
    Some((moved, retimed))
}

/// One action named in the Inbox note: its title, the instant it now has and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    pub title: String,
    pub start_at: DateTime<Utc>,
    pub outcome: Outcome,
}

/// The rows a time zone change writes: action rows to put (moved in place or re-keyed
/// copies), action ids to tombstone, series moved to the new zone, and the note lines
/// in start order. Empty when nothing changes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shift {
    pub writes: Vec<Action>,
    pub tombstones: Vec<String>,
    pub series: Vec<ActionSeries>,
    pub lines: Vec<Line>,
}

/// Plans the change from zone `from` to zone `to` at `now` over the owner's live
/// `actions`, `series` and `busy` blocks. Flexible planned actions starting at or after
/// `now` keep their local wall time; series move to `to` and their occurrence rows are
/// re-keyed to the ids every engine derives; locked, finished and clashing rows keep
/// their instant. Output: the [`Shift`] to write; empty when `from == to`.
pub fn plan_shift(from: Tz, to: Tz, now: DateTime<Utc>, actions: &[Action], series: &[ActionSeries], busy: &[BusyBlock]) -> Shift {
    assert!(actions.iter().all(|a| !a.deleted) && series.iter().all(|s| !s.deleted) && busy.iter().all(|b| !b.deleted), "plan_shift takes live rows");
    if from == to {
        return Shift::default();
    }
    let zones: BTreeMap<&str, Tz> = series.iter().filter_map(|s| s.timezone.parse().ok().map(|tz| (s.id.as_str(), tz))).collect();
    let mut shift = Shift::default();
    let mut moving: BTreeMap<&str, Tz> = BTreeMap::new();
    let mut kept: BTreeSet<&str> = BTreeSet::new();
    for s in series.iter().filter(|s| zones.get(s.id.as_str()).is_some_and(|tz| *tz != to)) {
        let old_zone = zones[s.id.as_str()];
        match relocate(s, to) {
            Some((moved, retimed)) => {
                if retimed {
                    let start = NaiveDateTime::parse_from_str(&moved.dtstart_local, recur::LOCAL_FORMAT).expect("validated by relocate");
                    shift.lines.push(Line { title: s.title.clone(), start_at: rule_instant(to, start), outcome: Outcome::Retimed });
                }
                moving.insert(s.id.as_str(), old_zone);
                shift.series.push(moved);
            }
            None => {
                let start = NaiveDateTime::parse_from_str(&s.dtstart_local, recur::LOCAL_FORMAT).map(|l| rule_instant(old_zone, l));
                shift.lines.push(Line { title: s.title.clone(), start_at: start.unwrap_or(now), outcome: Outcome::Kept });
                kept.insert(s.id.as_str());
            }
        }
    }
    let wall_zone = |a: &Action| a.series_id.as_deref().and_then(|id| zones.get(id).copied()).unwrap_or(from);
    let rekey = rekeyed(to, now, actions, &moving);

    let mut writes: BTreeMap<String, Action> = BTreeMap::new();
    for action in actions {
        if action.series_id.as_deref().is_some_and(|id| kept.contains(id)) {
            continue;
        }
        let mut row = action.clone();
        let mut changed = false;
        if let Some((id, target)) = rekey.get(action.id.as_str()) {
            row.id = id.clone();
            row.occurrence = Some(*target);
            changed = true;
        } else if needs_rekey(to, now, action, &moving) {
            shift.lines.push(Line { title: action.title.clone(), start_at: action.start_at, outcome: Outcome::Taken });
            continue;
        }
        let future = action.status == ActionStatus::Planned && action.start_at >= now;
        if future && action.locked {
            shift.lines.push(Line { title: action.title.clone(), start_at: action.start_at, outcome: Outcome::Fixed });
        } else if future {
            let place = if action.series_id.is_some() { rule_instant } else { wall_instant };
            let start = place(to, action.start_at.with_timezone(&wall_zone(action)).naive_local());
            if start != action.start_at {
                let end = start + (action.end_at - action.start_at);
                let outcome = if start < now {
                    Outcome::Past
                } else if let Some(title) = clash(start, end, &action.id, actions, busy) {
                    Outcome::Clash(title)
                } else {
                    row.start_at = start;
                    row.end_at = end;
                    changed = true;
                    Outcome::Moved
                };
                shift.lines.push(Line { title: action.title.clone(), start_at: row.start_at, outcome });
            }
        }
        if changed {
            let previous = writes.insert(row.id.clone(), row);
            assert!(previous.is_none(), "one write per id");
        }
    }
    shift.tombstones = rekey.keys().filter(|id| !writes.contains_key(**id)).map(|id| id.to_string()).collect();
    shift.writes = writes.into_values().collect();
    shift.writes.sort_by(|a, b| a.start_at.cmp(&b.start_at).then_with(|| a.id.cmp(&b.id)));
    assert!(shift.series.iter().all(|s| s.timezone == to.name()), "moved series follow the new zone");
    shift.lines.sort_by(|a, b| a.start_at.cmp(&b.start_at).then_with(|| a.title.cmp(&b.title)));
    shift
}

fn new_occurrence(to: Tz, action: &Action, moving: &BTreeMap<&str, Tz>) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let old_zone = *moving.get(action.series_id.as_deref()?)?;
    let old = action.occurrence.expect("series rows carry their occurrence");
    Some((old, rule_instant(to, old.with_timezone(&old_zone).naive_local())))
}

fn needs_rekey(to: Tz, now: DateTime<Utc>, action: &Action, moving: &BTreeMap<&str, Tz>) -> bool {
    new_occurrence(to, action, moving).is_some_and(|(old, new)| old != new && (old >= now || new >= now))
}

/// The new occurrence instant of every series row that must be re-keyed, by old id.
/// A target id held by a live row that is not re-keyed itself, or claimed by an
/// earlier row, is dropped until none is left, so that row keeps its id and the note
/// reports it. Output: old id to new occurrence instant.
fn rekeyed<'a>(to: Tz, now: DateTime<Utc>, actions: &'a [Action], moving: &BTreeMap<&str, Tz>) -> BTreeMap<&'a str, (String, DateTime<Utc>)> {
    let mut targets: BTreeMap<&'a str, (String, DateTime<Utc>)> = BTreeMap::new();
    for a in actions.iter().filter(|a| needs_rekey(to, now, a, moving)) {
        let (_, new) = new_occurrence(to, a, moving).expect("checked by needs_rekey");
        targets.insert(a.id.as_str(), (occurrence_id(a.series_id.as_deref().expect("series row"), &new), new));
    }
    let live: BTreeSet<&str> = actions.iter().map(|a| a.id.as_str()).collect();
    loop {
        let blocked: Vec<&'a str> = {
            let mut claimed: BTreeSet<&str> = BTreeSet::new();
            targets
                .iter()
                .filter(|(_, (target, _))| {
                    let held = live.contains(target.as_str()) && !targets.contains_key(target.as_str());
                    !claimed.insert(target.as_str()) || held
                })
                .map(|(id, _)| *id)
                .collect()
        };
        if blocked.is_empty() {
            return targets;
        }
        for id in blocked {
            targets.remove(id);
        }
    }
}

/// The title of the first live locked action other than `id`, or else timed busy
/// block, that overlaps `[start, end)`. A busy block with no title reads "Busy".
/// Output: `None` when the span is free.
fn clash(start: DateTime<Utc>, end: DateTime<Utc>, id: &str, actions: &[Action], busy: &[BusyBlock]) -> Option<String> {
    let overlaps = |a: DateTime<Utc>, b: DateTime<Utc>| start < b && a < end;
    if let Some(a) = actions.iter().find(|a| a.locked && a.id != id && overlaps(a.start_at, a.end_at)) {
        return Some(a.title.clone());
    }
    busy.iter()
        .find(|b| !b.all_day && overlaps(b.start_at, b.end_at))
        .map(|b| if b.title.trim().is_empty() { "Busy".to_string() } else { b.title.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recur::materialize;
    use crate::rows::samples;
    use chrono::{NaiveDate, Timelike};
    use chrono_tz::{America::New_York, Europe::Berlin};

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap()
    }

    fn action(n: u32, title: &str, start: DateTime<Utc>, minutes: i64) -> Action {
        let mut a = samples::action();
        a.id = format!("0190f2a0-0000-7000-8000-{:012x}", 0x500 + n);
        a.title = title.into();
        a.start_at = start;
        a.end_at = start + Duration::minutes(minutes);
        a
    }

    fn block(title: &str, start: DateTime<Utc>, minutes: i64, all_day: bool) -> BusyBlock {
        BusyBlock {
            id: crate::new_id(),
            user_id: samples::USER.into(),
            calendar_id: "cal".into(),
            event_id: title.into(),
            title: title.into(),
            start_at: start,
            end_at: start + Duration::minutes(minutes),
            all_day,
            group_name: None,
            day_kind: None,
            updated_at: samples::STAMP.into(),
            deleted: false,
        }
    }

    fn rows(series: &ActionSeries, from: DateTime<Utc>, days: u32) -> Vec<Action> {
        materialize(series, from, days, &BTreeSet::new()).unwrap()
    }

    fn apply(actions: &[Action], shift: &Shift) -> Vec<Action> {
        let gone: BTreeSet<&str> = shift.tombstones.iter().map(String::as_str).collect();
        let written: BTreeSet<&str> = shift.writes.iter().map(|a| a.id.as_str()).collect();
        let mut out: Vec<Action> = actions.iter().filter(|a| !gone.contains(a.id.as_str()) && !written.contains(a.id.as_str())).cloned().collect();
        out.extend(shift.writes.iter().cloned());
        out.sort_by_key(|a| a.start_at);
        out
    }

    fn moved_series(series: &ActionSeries, shift: &Shift) -> ActionSeries {
        shift.series.iter().find(|s| s.id == series.id).cloned().unwrap_or_else(|| series.clone())
    }

    fn line<'a>(shift: &'a Shift, title: &str) -> &'a Line {
        shift.lines.iter().find(|l| l.title == title).unwrap_or_else(|| panic!("no line {title}"))
    }

    #[test]
    fn wall_instant_handles_gaps_and_repeats() {
        assert_eq!(wall_instant(New_York, local(2026, 11, 23, 7, 0)), utc(2026, 11, 23, 12, 0));
        assert_eq!(wall_instant(New_York, local(2027, 3, 14, 2, 30)), utc(2027, 3, 14, 7, 0));
        assert_eq!(wall_instant(New_York, local(2026, 11, 1, 1, 30)), utc(2026, 11, 1, 5, 30));
        assert_eq!(wall_instant(Berlin, local(2027, 3, 28, 2, 15)), utc(2027, 3, 28, 1, 0));
        assert_eq!(rule_instant(New_York, local(2027, 3, 14, 2, 30)), utc(2027, 3, 14, 7, 30));
        assert_eq!(rule_instant(Berlin, local(2027, 3, 28, 2, 15)), utc(2027, 3, 28, 1, 15));
        assert_eq!(rule_instant(New_York, local(2026, 11, 23, 7, 0)), utc(2026, 11, 23, 12, 0));
    }

    #[test]
    fn flexible_follows_wall_time_and_locked_keeps_instant() {
        let now = utc(2026, 11, 22, 20, 0);
        let gym = action(1, "Gym", wall_instant(Berlin, local(2026, 11, 23, 7, 0)), 60);
        let mut call = action(2, "Call", wall_instant(New_York, local(2026, 11, 23, 10, 0)), 30);
        call.locked = true;
        let shift = plan_shift(Berlin, New_York, now, &[gym.clone(), call.clone()], &[], &[]);
        let after = apply(&[gym.clone(), call.clone()], &shift);
        let gym_after = after.iter().find(|a| a.id == gym.id).unwrap();
        assert_eq!(gym_after.start_at, utc(2026, 11, 23, 12, 0));
        assert_eq!(gym_after.end_at - gym_after.start_at, Duration::minutes(60));
        assert_eq!(after.iter().find(|a| a.id == call.id).unwrap().start_at, call.start_at);
        assert_eq!(line(&shift, "Gym").outcome, Outcome::Moved);
        assert_eq!(line(&shift, "Call").outcome, Outcome::Fixed);
        assert!(shift.tombstones.is_empty());
    }

    #[test]
    fn series_moves_to_the_new_zone_and_rows_are_rekeyed() {
        let series = samples::series();
        let now = utc(2026, 11, 22, 20, 0);
        let before = rows(&series, now, 3);
        let shift = plan_shift(Berlin, New_York, now, &before, std::slice::from_ref(&series), &[]);
        assert_eq!(shift.series.len(), 1);
        assert_eq!(shift.series[0].timezone, "America/New_York");
        assert_eq!(shift.series[0].dtstart_local, series.dtstart_local);
        assert_eq!(shift.tombstones.len(), 3);
        assert_eq!(shift.writes.len(), 3);
        for row in &shift.writes {
            let occ = row.occurrence.unwrap();
            assert_eq!(occ.with_timezone(&New_York).hour(), 7);
            assert_eq!(row.id, occurrence_id(&series.id, &occ));
            assert_eq!(row.start_at, occ);
            assert_eq!(row.end_at - row.start_at, Duration::minutes(45));
            assert!(crate::rows::Record::validate(row).is_ok());
        }
    }

    #[test]
    fn series_across_the_new_york_dst_end_keeps_seven_local() {
        let series = samples::series();
        let now = utc(2026, 10, 28, 0, 0);
        let before = rows(&series, now, 7);
        assert_eq!(before.len(), 7);
        let shift = plan_shift(Berlin, New_York, now, &before, std::slice::from_ref(&series), &[]);
        let after = apply(&before, &shift);
        assert_eq!(after.len(), 7);
        assert!(after.iter().all(|a| a.start_at.with_timezone(&New_York).hour() == 7));
        let hours: Vec<u32> = after.iter().map(|a| a.start_at.hour()).collect();
        assert_eq!(hours, vec![11, 11, 11, 11, 12, 12, 12]);
        let next = moved_series(&series, &shift);
        let existing: BTreeSet<_> = after.iter().chain(&before).filter_map(|a| a.occurrence).collect();
        assert!(materialize(&next, now, 7, &existing).unwrap().is_empty());
    }

    #[test]
    fn spring_gap_lands_on_the_first_valid_minute() {
        let now = utc(2027, 3, 1, 0, 0);
        let late = action(3, "Late", wall_instant(Berlin, local(2027, 3, 14, 2, 30)), 30);
        let shift = plan_shift(Berlin, New_York, now, &[late.clone()], &[], &[]);
        assert_eq!(shift.writes[0].start_at, utc(2027, 3, 14, 7, 0));
        assert_eq!(shift.writes[0].start_at.with_timezone(&New_York).hour(), 3);
    }

    #[test]
    fn clashes_and_the_past_are_left_and_reported() {
        let now = utc(2026, 11, 23, 11, 0);
        let gym = action(1, "Gym", wall_instant(Berlin, local(2026, 11, 23, 7, 0)), 60);
        let read = action(2, "Read", wall_instant(Berlin, local(2026, 11, 23, 13, 0)), 30);
        let walk = action(3, "Walk", wall_instant(Berlin, local(2026, 11, 23, 15, 0)), 30);
        let early = action(4, "Stretch", wall_instant(New_York, local(2026, 11, 23, 13, 0)), 15);
        let mut call = action(5, "Call", wall_instant(New_York, local(2026, 11, 23, 7, 30)), 30);
        call.locked = true;
        let dentist = block("Dentist", wall_instant(New_York, local(2026, 11, 23, 13, 0)), 60, false);
        let holiday = block("Holiday", wall_instant(New_York, local(2026, 11, 23, 0, 0)), 24 * 60, true);
        let mut list = vec![read.clone(), walk.clone(), call.clone()];
        let shift = plan_shift(Berlin, New_York, now, &list, &[], &[dentist.clone(), holiday.clone()]);
        assert_eq!(line(&shift, "Read").outcome, Outcome::Clash("Dentist".into()));
        assert_eq!(line(&shift, "Read").start_at, read.start_at);
        assert_eq!(line(&shift, "Walk").outcome, Outcome::Moved);
        assert!(shift.writes.iter().all(|a| a.id != read.id));
        list.push(gym.clone());
        let shift = plan_shift(Berlin, New_York, utc(2026, 11, 23, 5, 0), &list, &[], &[]);
        assert_eq!(line(&shift, "Gym").outcome, Outcome::Clash("Call".into()));
        let east = plan_shift(New_York, Berlin, utc(2026, 11, 23, 15, 0), std::slice::from_ref(&early), &[], &[]);
        assert_eq!(line(&east, "Stretch").outcome, Outcome::Past);
        assert!(east.writes.is_empty());
    }

    #[test]
    fn locked_occurrence_is_rekeyed_and_keeps_its_instant() {
        let series = samples::series();
        let now = utc(2026, 11, 22, 20, 0);
        let mut before = rows(&series, now, 2);
        before[0].locked = true;
        let shift = plan_shift(Berlin, New_York, now, &before, std::slice::from_ref(&series), &[]);
        let locked = shift.writes.iter().find(|a| a.locked).unwrap();
        assert_eq!(locked.start_at, before[0].start_at);
        assert_eq!(locked.occurrence.unwrap(), utc(2026, 11, 23, 12, 0));
        assert_eq!(locked.id, occurrence_id(&series.id, &utc(2026, 11, 23, 12, 0)));
        assert!(shift.tombstones.contains(&before[0].id));
        assert_eq!(line(&shift, "Core workout").outcome, Outcome::Fixed);
    }

    #[test]
    fn flying_west_today_done_occurrence_leaves_no_twin() {
        let series = samples::series();
        let now = utc(2026, 11, 23, 10, 0);
        let mut before = rows(&series, utc(2026, 11, 23, 0, 0), 15);
        before[0].status = ActionStatus::Done;
        before[0].done_at = Some(before[0].end_at);
        let shift = plan_shift(Berlin, New_York, now, &before, std::slice::from_ref(&series), &[]);
        let after = apply(&before, &shift);
        let done = after.iter().find(|a| a.status == ActionStatus::Done).unwrap();
        assert_eq!(done.start_at, before[0].start_at);
        assert_eq!(done.occurrence.unwrap(), utc(2026, 11, 23, 12, 0));
        let existing: BTreeSet<_> = before.iter().chain(&after).filter_map(|a| a.occurrence).collect();
        let fresh = materialize(&moved_series(&series, &shift), now, 14, &existing).unwrap();
        let mut days = BTreeMap::new();
        for a in after.iter().chain(&fresh) {
            *days.entry(a.occurrence.unwrap().with_timezone(&New_York).date_naive()).or_insert(0) += 1;
        }
        assert!(days.values().all(|n| *n == 1), "{days:?}");
    }

    #[test]
    fn same_shift_id_collision_keeps_the_live_copy() {
        let mut series = samples::series();
        series.rrule = "FREQ=DAILY;BYHOUR=7,13".into();
        let now = utc(2026, 11, 22, 20, 0);
        let before = rows(&series, now, 2);
        let shift = plan_shift(Berlin, New_York, now, &before, std::slice::from_ref(&series), &[]);
        let ids: BTreeSet<&str> = shift.writes.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids.len(), shift.writes.len());
        assert!(shift.tombstones.iter().all(|t| !ids.contains(t.as_str())));
        let after = apply(&before, &shift);
        let live: BTreeSet<&str> = after.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(live.len(), after.len());
        assert!(after.iter().all(|a| [7, 13].contains(&a.start_at.with_timezone(&New_York).hour())));
    }

    #[test]
    fn flying_back_restores_the_original_instants() {
        let series = samples::series();
        let now = utc(2026, 11, 22, 20, 0);
        let gym = action(1, "Gym", wall_instant(Berlin, local(2026, 11, 24, 18, 0)), 60);
        let mut before = rows(&series, now, 4);
        before.push(gym);
        let out = plan_shift(Berlin, New_York, now, &before, std::slice::from_ref(&series), &[]);
        let away = apply(&before, &out);
        let back = plan_shift(New_York, Berlin, now, &away, &[moved_series(&series, &out)], &[]);
        let home = apply(&away, &back);
        let key = |list: &[Action]| list.iter().map(|a| (a.id.clone(), a.start_at, a.occurrence)).collect::<BTreeSet<_>>();
        assert_eq!(key(&home), key(&before));
        assert_eq!(moved_series(&moved_series(&series, &out), &back).timezone, "Europe/Berlin");
    }

    #[test]
    fn series_start_in_the_destination_gap_moves_to_the_first_valid_minute() {
        let mut series = samples::series();
        series.dtstart_local = "2027-03-14T02:30:00".into();
        let now = utc(2027, 3, 10, 0, 0);
        let before = rows(&series, now, 7);
        assert_eq!(before.len(), 3);
        let shift = plan_shift(Berlin, New_York, now, &before, std::slice::from_ref(&series), &[]);
        let next = moved_series(&series, &shift);
        assert_eq!(next.timezone, "America/New_York");
        assert_eq!(next.dtstart_local, "2027-03-14T03:30:00");
        assert!(crate::rows::Record::validate(&next).is_ok());
        assert!(shift.writes.iter().all(|a| crate::rows::Record::validate(a).is_ok()));
        let retimed = line(&shift, "Core workout");
        assert_eq!(retimed.outcome, Outcome::Retimed);
        assert_eq!(retimed.start_at, utc(2027, 3, 14, 7, 30));
        let after = apply(&before, &shift);
        let existing: BTreeSet<_> = before.iter().chain(&after).filter_map(|a| a.occurrence).collect();
        assert!(materialize(&next, now, 7, &existing).unwrap().is_empty());
        let wall: Vec<(u32, u32)> = after.iter().map(|a| a.start_at.with_timezone(&New_York)).map(|t| (t.hour(), t.minute())).collect();
        assert_eq!(wall, vec![(3, 30), (2, 30), (2, 30)]);
        assert_eq!(rows(&next, now, 7).iter().map(|a| a.start_at).collect::<Vec<_>>(), after.iter().map(|a| a.start_at).collect::<Vec<_>>());
    }

    #[test]
    fn gap_day_occurrence_of_a_plain_series_is_rekeyed_where_materialize_puts_it() {
        let mut series = samples::series();
        series.dtstart_local = "2027-03-01T02:30:00".into();
        let now = utc(2027, 3, 10, 0, 0);
        let before = rows(&series, now, 7);
        let shift = plan_shift(Berlin, New_York, now, &before, std::slice::from_ref(&series), &[]);
        let next = moved_series(&series, &shift);
        assert_eq!(next.dtstart_local, series.dtstart_local);
        let after = apply(&before, &shift);
        let existing: BTreeSet<_> = before.iter().chain(&after).filter_map(|a| a.occurrence).collect();
        assert!(materialize(&next, now, 7, &existing).unwrap().is_empty());
        assert_eq!(after.iter().map(|a| a.start_at).collect::<Vec<_>>(), rows(&next, now, 7).iter().map(|a| a.start_at).collect::<Vec<_>>());
    }

    #[test]
    fn settle_rekeys_only_untouched_future_rows_off_the_rule() {
        let series = samples::series();
        let now = utc(2026, 11, 22, 20, 0);
        let moved = ActionSeries { timezone: "America/New_York".into(), ..series.clone() };
        let current = rows(&moved, now, 2);
        let mut old = rows(&series, now - Duration::days(1), 5);
        assert_eq!(settle(Some(Berlin), now, &current, std::slice::from_ref(&moved)), Settle::default());
        old[2].locked = true;
        old[3].updated_at = samples::STAMP.into();
        let all: Vec<Action> = current.iter().chain(&old).cloned().collect();
        let plan = settle(Some(Berlin), now, &all, std::slice::from_ref(&moved));
        assert_eq!(plan.tombstones, vec![old[1].id.clone(), old[4].id.clone()]);
        let target = wall_instant(New_York, local(2026, 11, 26, 7, 0));
        assert_eq!(plan.inserts, vec![crate::recur::occurrence(&moved, target)]);
        let blind = settle(None, now, &all, std::slice::from_ref(&moved));
        assert_eq!(blind.tombstones, plan.tombstones);
        assert!(blind.inserts.is_empty());
    }

    #[test]
    fn same_zone_is_an_empty_shift() {
        let series = samples::series();
        let now = utc(2026, 11, 22, 20, 0);
        let before = rows(&series, now, 3);
        assert_eq!(plan_shift(Berlin, Berlin, now, &before, std::slice::from_ref(&series), &[]), Shift::default());
    }
}
