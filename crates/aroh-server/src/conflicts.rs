//! Owner edit conflicts (Q77): whether one moved or resized action now
//! overlaps other planned actions or busy time, or pushes its goal's day
//! over the daily minutes (Q64), and which goals a replan must touch.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use std::collections::BTreeSet;

/// One planned action as the conflict check reads it: id, goal, title,
/// span and whether it is fixed (`locked`, never moved by an agent).
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: String,
    pub goal_id: String,
    pub title: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub locked: bool,
}

/// What a replan has to fix after an owner edit: the goals to replan and
/// the plain reason handed to each run.
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    pub goals: BTreeSet<String>,
    pub reason: String,
}

fn overlaps(a: (DateTime<Utc>, DateTime<Utc>), b: (DateTime<Utc>, DateTime<Utc>)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

fn quoted(titles: &[&str]) -> String {
    titles.iter().map(|t| format!("\"{t}\"")).collect::<Vec<_>>().join(", ")
}

/// Checks `moved` (the owner's edited action) against `others` (the
/// owner's other planned actions, any goal) and `busy` (device busy spans)
/// in zone `tz`, with `cap` the owner's daily minutes when known.
/// Returns `None` when nothing overlaps and the goal's day stays within
/// the cap; else the goals of the flexible overlapping actions (or the
/// moved goal when only fixed items clash or the cap broke) and a reason.
pub fn check(moved: &Item, others: &[Item], busy: &[(DateTime<Utc>, DateTime<Utc>)], tz: Tz, cap: Option<i64>) -> Option<Conflict> {
    assert!(moved.end_at > moved.start_at, "an action ends after it starts");
    let span = (moved.start_at, moved.end_at);
    let hit: Vec<&Item> = others.iter().filter(|o| o.id != moved.id && overlaps(span, (o.start_at, o.end_at))).collect();
    let busy_hit = busy.iter().any(|b| overlaps(span, *b));
    let day = moved.start_at.with_timezone(&tz).date_naive();
    let goal_minutes: i64 = others
        .iter()
        .filter(|o| o.id != moved.id && o.goal_id == moved.goal_id && o.start_at.with_timezone(&tz).date_naive() == day)
        .map(|o| (o.end_at - o.start_at).num_minutes())
        .sum::<i64>()
        + (moved.end_at - moved.start_at).num_minutes();
    let over_cap = cap.filter(|c| goal_minutes > *c);
    if hit.is_empty() && !busy_hit && over_cap.is_none() {
        return None;
    }
    let mut goals: BTreeSet<String> = hit.iter().filter(|o| !o.locked).map(|o| o.goal_id.clone()).collect();
    if goals.is_empty() || over_cap.is_some() {
        goals.insert(moved.goal_id.clone());
    }
    let local = |t: DateTime<Utc>| t.with_timezone(&tz).format("%a %H:%M").to_string();
    let mut reason = format!("The owner moved \"{}\" to {} - {} and fixed it there.", moved.title, local(moved.start_at), moved.end_at.with_timezone(&tz).format("%H:%M"));
    if !hit.is_empty() {
        let titles: Vec<&str> = hit.iter().map(|o| o.title.as_str()).collect();
        reason.push_str(&format!(" It now overlaps {}.", quoted(&titles)));
    }
    if busy_hit {
        reason.push_str(" It now overlaps busy time from the owner's calendar.");
    }
    if let Some(c) = over_cap {
        reason.push_str(&format!(" That day now holds {goal_minutes} min of this goal, over the {c} min daily limit."));
    }
    reason.push_str(" Move only other, flexible actions so nothing overlaps and the day stays within the limit; never move fixed actions.");
    assert!(!goals.is_empty(), "a conflict names at least one goal");
    Some(Conflict { goals, reason })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text).unwrap().with_timezone(&Utc)
    }

    fn item(id: &str, goal: &str, start: &str, end: &str, locked: bool) -> Item {
        Item { id: id.into(), goal_id: goal.into(), title: id.into(), start_at: at(start), end_at: at(end), locked }
    }

    #[test]
    fn no_overlap_no_conflict() {
        let moved = item("run", "g1", "2026-11-24T10:00:00Z", "2026-11-24T10:30:00Z", true);
        let touching = item("read", "g2", "2026-11-24T10:30:00Z", "2026-11-24T11:00:00Z", false);
        assert_eq!(check(&moved, &[moved.clone(), touching], &[], Tz::UTC, Some(60)), None);
    }

    #[test]
    fn overlap_replans_the_flexible_actions_goal() {
        let moved = item("run", "g1", "2026-11-24T10:00:00Z", "2026-11-24T10:30:00Z", true);
        let other = item("read", "g2", "2026-11-24T10:15:00Z", "2026-11-24T10:45:00Z", false);
        let c = check(&moved, &[other], &[], Tz::UTC, None).unwrap();
        assert_eq!(c.goals.into_iter().collect::<Vec<_>>(), vec!["g2".to_string()]);
        assert!(c.reason.contains("overlaps \"read\""), "{}", c.reason);
        assert!(c.reason.contains("Tue 10:00 - 10:30"), "{}", c.reason);
    }

    #[test]
    fn clash_with_fixed_or_busy_replans_the_moved_goal() {
        let moved = item("run", "g1", "2026-11-24T10:00:00Z", "2026-11-24T10:30:00Z", true);
        let fixed = item("call", "g2", "2026-11-24T10:00:00Z", "2026-11-24T11:00:00Z", true);
        let c = check(&moved, &[fixed], &[], Tz::UTC, None).unwrap();
        assert_eq!(c.goals.into_iter().collect::<Vec<_>>(), vec!["g1".to_string()]);
        let busy = [(at("2026-11-24T10:20:00Z"), at("2026-11-24T12:00:00Z"))];
        let c = check(&moved, &[], &busy, Tz::UTC, None).unwrap();
        assert!(c.goals.contains("g1") && c.reason.contains("busy time"));
    }

    #[test]
    fn cap_counts_the_goals_local_day() {
        let moved = item("run", "g1", "2026-11-24T22:30:00Z", "2026-11-24T23:00:00Z", true);
        let same_day = item("run2", "g1", "2026-11-24T08:00:00Z", "2026-11-24T08:30:00Z", false);
        let next_local_day = item("run3", "g1", "2026-11-25T08:00:00Z", "2026-11-25T08:30:00Z", false);
        let others = [same_day.clone(), next_local_day.clone()];
        let c = check(&moved, &others, &[], Tz::UTC, Some(45)).unwrap();
        assert!(c.reason.contains("60 min of this goal, over the 45 min"), "{}", c.reason);
        assert_eq!(check(&moved, &others, &[], Tz::UTC, Some(60)), None);
        let tokyo: Tz = "Asia/Tokyo".parse().unwrap();
        assert!(check(&moved, &others, &[], tokyo, Some(45)).is_some(), "07:30 and 17:00 Tokyo share Nov 25");
        assert_eq!(check(&moved, &[same_day], &[], tokyo, Some(45)), None);
    }
}
