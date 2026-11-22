//! Device calendar triage: the event shape read from EventKit, matching events against
//! calendar rules, building rules from triage decisions and kept events as busy blocks.

use crate::rows::{BusyBlock, CalendarRule, DayKind, RuleDecision};
use crate::{derived_id, invalid, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Longest `calendar_rules.pattern`, in characters.
pub const MAX_PATTERN: usize = 200;
/// Longest event title kept, in characters, as `busy_blocks.title` allows.
pub const MAX_TITLE: usize = 500;
/// Longest calendar or event id, in characters.
pub const MAX_ID: usize = 255;
/// Longest calendar name or group name, in characters.
pub const MAX_NAME: usize = 100;
/// Longest recurrence text, in characters.
pub const MAX_RRULE: usize = 500;

/// One occurrence of a device calendar event as the app reads it from EventKit.
/// `event_id` is shared by every occurrence of a recurring event; `rrule` is its
/// RFC 5545 recurrence text, or `None` for a one-off event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceEvent {
    pub calendar_id: String,
    pub calendar: String,
    pub event_id: String,
    pub title: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub all_day: bool,
    pub rrule: Option<String>,
}

fn check_len(field: &'static str, text: &str, min: usize, max: usize) -> Result<()> {
    let n = text.chars().count();
    if n < min || n > max {
        return Err(invalid(field, format!("must have {min} to {max} characters")));
    }
    Ok(())
}

impl DeviceEvent {
    /// Checks the event at the trust boundary: ids 1 to 255 characters, calendar name up
    /// to 100, title up to 500, recurrence up to 500, and an end after the start.
    /// Returns `Invalid` naming the first bad field.
    pub fn validate(&self) -> Result<()> {
        check_len("event.calendar_id", &self.calendar_id, 1, MAX_ID)?;
        check_len("event.event_id", &self.event_id, 1, MAX_ID)?;
        check_len("event.calendar", &self.calendar, 0, MAX_NAME)?;
        check_len("event.title", &self.title, 0, MAX_TITLE)?;
        if let Some(rule) = &self.rrule {
            check_len("event.rrule", rule, 1, MAX_RRULE)?;
        }
        if self.end_at <= self.start_at {
            return Err(invalid("event.end_at", "must be after start_at"));
        }
        Ok(())
    }
}

/// What a rule is built from: one triaged event's ids, title and whether it repeats.
/// The server keeps these per event key of a triage run.
/// `recurring` events get a series rule, one-off events a title rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleSource {
    pub calendar_id: String,
    pub event_id: String,
    pub title: String,
    pub recurring: bool,
}

/// Returns the pattern a title gives: trimmed, lowercased and cut to 200 characters.
/// Matching compares it with titles normalized the same way.
/// Empty for a blank title.
pub fn pattern_of(title: &str) -> String {
    title.trim().to_lowercase().chars().take(MAX_PATTERN).collect()
}

fn applies(rule: &CalendarRule, event: &DeviceEvent) -> bool {
    let calendar = rule.calendar_id.as_ref().map_or(true, |c| *c == event.calendar_id);
    let series = rule.series_key.as_ref().map_or(true, |s| *s == event.event_id);
    let title = event.title.trim().to_lowercase();
    let pattern = rule.pattern.as_ref().map_or(true, |p| !p.is_empty() && title.contains(p.as_str()));
    !rule.deleted && calendar && series && pattern
}

fn rank(rule: &CalendarRule) -> (u8, usize) {
    let score = u8::from(rule.series_key.is_some()) * 4 + u8::from(rule.pattern.is_some()) * 2 + u8::from(rule.calendar_id.is_some());
    (score, rule.pattern.as_ref().map_or(0, |p| p.chars().count()))
}

/// Finds the rule that decides `event`: among live rules whose set fields all match
/// (same calendar, same series, pattern inside the lowercased title), the most specific:
/// series, then pattern, then calendar, then the longer pattern, then the smaller id.
pub fn rule_for<'a>(rules: &'a [CalendarRule], event: &DeviceEvent) -> Option<&'a CalendarRule> {
    rules
        .iter()
        .filter(|r| applies(r, event))
        .min_by(|a, b| rank(b).cmp(&rank(a)).then_with(|| a.id.cmp(&b.id)))
}

/// Builds the rules one triage decision gives for `members` of group `group`: a series
/// rule per recurring or untitled event, a calendar plus title rule per titled one-off.
/// Ids derive from the user and the rule key, so a later decision overwrites; one per key.
pub fn rules_for(user_id: &str, group: &str, decision: RuleDecision, members: &[RuleSource]) -> Vec<CalendarRule> {
    let group: String = group.trim().chars().take(MAX_NAME).collect();
    assert!(!group.is_empty(), "triage groups have a name");
    let mut out: Vec<CalendarRule> = Vec::new();
    for m in members {
        let pattern = pattern_of(&m.title);
        let (key, calendar_id, series_key, pattern) = if m.recurring || pattern.is_empty() {
            (format!("series:{}", m.event_id), None, Some(m.event_id.clone()), None)
        } else {
            (format!("title:{}:{pattern}", m.calendar_id), Some(m.calendar_id.clone()), None, Some(pattern))
        };
        let id = derived_id(user_id, &key, 0);
        if out.iter().any(|r| r.id == id) {
            continue;
        }
        out.push(CalendarRule {
            id,
            user_id: user_id.to_string(),
            calendar_id,
            series_key,
            pattern,
            group_name: group.clone(),
            decision,
            day_kind: None,
            updated_at: String::new(),
            deleted: false,
        });
    }
    out
}

/// Most local days one all-day block spans; longer blocks are cut.
pub const MAX_ALL_DAY_DAYS: usize = 62;

/// The local dates in `zone` an all-day span `[start, end)` covers, first to last: EventKit
/// gives midnight to the next midnight for one day, or 23:59:59 of the last day.
/// At most `MAX_ALL_DAY_DAYS` dates.
pub fn local_days(start: DateTime<Utc>, end: DateTime<Utc>, zone: chrono_tz::Tz) -> Vec<chrono::NaiveDate> {
    assert!(end > start, "spans are not empty");
    let first = start.with_timezone(&zone).date_naive();
    let last = (end - chrono::Duration::seconds(1)).with_timezone(&zone).date_naive().max(first);
    let days: Vec<chrono::NaiveDate> = first.iter_days().take_while(|d| *d <= last).take(MAX_ALL_DAY_DAYS).collect();
    assert!(!days.is_empty() && days.len() <= MAX_ALL_DAY_DAYS);
    days
}

/// The rule that gives one device event `kind` for every occurrence: a series rule on the
/// event id, kept, in group `group`. Id derived like triage's series rules, so the owner's
/// choice and a later triage answer write the same row. Unstamped.
pub fn day_rule(user_id: &str, calendar_id: &str, event_id: &str, title: &str, group: &str, kind: DayKind) -> CalendarRule {
    let source = RuleSource { calendar_id: calendar_id.to_string(), event_id: event_id.to_string(), title: title.to_string(), recurring: true };
    let mut rules = rules_for(user_id, group, RuleDecision::Keep, &[source]);
    assert_eq!(rules.len(), 1, "one event gives one series rule");
    let mut rule = rules.remove(0);
    rule.day_kind = Some(kind);
    rule
}

/// The busy block `event` becomes under `rule`: id derived from the user, the calendar, the
/// event and its start, so re-imports collapse; title cut to 500 characters. A timed event
/// needs a keep rule; an all-day event may have none and takes its rule's day kind.
/// Unstamped: the store stamps it on write.
pub fn busy_block(user_id: &str, event: &DeviceEvent, rule: Option<&CalendarRule>) -> BusyBlock {
    assert!(event.all_day || rule.is_some_and(|r| r.decision == RuleDecision::Keep), "only kept or all-day events become busy time");
    let start_ms = event.start_at.timestamp_millis();
    let key = format!("busy:{}:{}:{start_ms}", event.calendar_id, event.event_id);
    BusyBlock {
        id: derived_id(user_id, &key, u64::try_from(start_ms).unwrap_or(0)),
        user_id: user_id.to_string(),
        calendar_id: event.calendar_id.clone(),
        event_id: event.event_id.clone(),
        title: event.title.chars().take(MAX_TITLE).collect(),
        start_at: event.start_at,
        end_at: event.end_at,
        all_day: event.all_day,
        group_name: rule.map(|r| r.group_name.clone()),
        day_kind: if event.all_day { rule.and_then(|r| r.day_kind) } else { None },
        updated_at: String::new(),
        deleted: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rows::Record;
    use chrono::TimeZone;

    const USER: &str = "0190f2a0-0000-7000-8000-00000000000a";

    fn event(calendar: &str, id: &str, title: &str, hour: u32) -> DeviceEvent {
        let start = Utc.with_ymd_and_hms(2026, 11, 23, hour, 0, 0).unwrap();
        DeviceEvent {
            calendar_id: calendar.into(),
            calendar: "Work".into(),
            event_id: id.into(),
            title: title.into(),
            start_at: start,
            end_at: start + chrono::Duration::minutes(30),
            all_day: false,
            rrule: None,
        }
    }

    fn source(calendar: &str, id: &str, title: &str, recurring: bool) -> RuleSource {
        RuleSource { calendar_id: calendar.into(), event_id: id.into(), title: title.into(), recurring }
    }

    #[test]
    fn series_rule_beats_title_rule() {
        let mut rules = rules_for(USER, "Noise", RuleDecision::Ignore, &[source("work", "other", "Standup", false)]);
        rules.extend(rules_for(USER, "Work", RuleDecision::Keep, &[source("work", "evt-1", "Standup", true)]));
        let found = rule_for(&rules, &event("work", "evt-1", "Standup", 9)).unwrap();
        assert_eq!((found.group_name.as_str(), found.decision), ("Work", RuleDecision::Keep));
        let other = rule_for(&rules, &event("work", "evt-9", "Daily standup", 9)).unwrap();
        assert_eq!(other.group_name, "Noise");
    }

    #[test]
    fn pattern_is_case_insensitive_substring() {
        let rules = rules_for(USER, "Fitness", RuleDecision::Keep, &[source("gym", "a", "  Spin Class ", false)]);
        assert_eq!(rules[0].pattern.as_deref(), Some("spin class"));
        assert!(rule_for(&rules, &event("gym", "b", "SPIN CLASS with Ana", 18)).is_some());
        assert!(rule_for(&rules, &event("gym", "c", "Spin", 18)).is_none());
    }

    #[test]
    fn calendar_must_match_when_set() {
        let rules = rules_for(USER, "Work", RuleDecision::Keep, &[source("work", "a", "Review", false)]);
        assert!(rule_for(&rules, &event("work", "b", "Review", 10)).is_some());
        assert!(rule_for(&rules, &event("home", "b", "Review", 10)).is_none());
        let mut gone = rules.clone();
        gone[0].deleted = true;
        assert!(rule_for(&gone, &event("work", "b", "Review", 10)).is_none());
    }

    #[test]
    fn rules_for_builds_series_and_title_rules_with_stable_ids() {
        let long = "x".repeat(450);
        let members = [
            source("work", "evt-1", "Standup", true),
            source("work", "evt-2", "Standup", true),
            source("work", "evt-3", &long, false),
            source("work", "evt-4", &long, false),
            source("work", "evt-5", "   ", false),
        ];
        let rules = rules_for(USER, "Work", RuleDecision::Keep, &members);
        assert_eq!(rules.len(), 4);
        assert_eq!(rules[0].series_key.as_deref(), Some("evt-1"));
        assert_eq!(rules[2].pattern.as_ref().unwrap().chars().count(), MAX_PATTERN);
        assert_eq!(rules[3].series_key.as_deref(), Some("evt-5"));
        let again = rules_for(USER, "Meetings", RuleDecision::Ignore, &members[..1]);
        assert_eq!(again[0].id, rules[0].id);
        for r in &rules {
            let mut r = r.clone();
            r.updated_at = crate::seed::SEED_STAMP.into();
            r.validate().unwrap();
        }
    }

    #[test]
    fn busy_block_ids_are_stable_and_valid() {
        let rules = rules_for(USER, "Work", RuleDecision::Keep, &[source("work", "evt-1", "Standup", true)]);
        let e = event("work", "evt-1", "Standup", 9);
        let mut block = busy_block(USER, &e, Some(&rules[0]));
        assert_eq!(block.id, busy_block(USER, &e, Some(&rules[0])).id);
        assert_ne!(block.id, busy_block(USER, &event("work", "evt-1", "Standup", 10), Some(&rules[0])).id);
        assert_eq!(block.group_name.as_deref(), Some("Work"));
        block.updated_at = crate::seed::SEED_STAMP.into();
        block.validate().unwrap();
    }

    #[test]
    fn local_days_count_eventkit_spans_inclusively() {
        let berlin = chrono_tz::Europe::Berlin;
        let at = |text: &str| text.parse::<DateTime<Utc>>().unwrap();
        let day = |d: u32| chrono::NaiveDate::from_ymd_opt(2026, 11, d).unwrap();
        assert_eq!(local_days(at("2026-11-26T23:00:00Z"), at("2026-11-27T23:00:00Z"), berlin), [day(27)]);
        assert_eq!(local_days(at("2026-11-26T23:00:00Z"), at("2026-11-29T22:59:59Z"), berlin), [day(27), day(28), day(29)]);
        assert_eq!(local_days(at("2026-11-26T23:00:00Z"), at("2026-11-29T23:00:00Z"), berlin), [day(27), day(28), day(29)]);
        assert_eq!(local_days(at("2026-11-27T00:00:00Z"), at("2026-11-27T00:00:30Z"), chrono_tz::UTC), [day(27)]);
        assert_eq!(local_days(at("2026-01-01T00:00:00Z"), at("2027-01-01T00:00:00Z"), chrono_tz::UTC).len(), MAX_ALL_DAY_DAYS);
    }

    #[test]
    fn day_rules_share_the_series_id_and_carry_the_kind() {
        let rule = day_rule(USER, "home", "trip-1", "Trip", "All day", DayKind::Away);
        let series = &rules_for(USER, "Travel", RuleDecision::Ignore, &[source("home", "trip-1", "Trip", true)])[0];
        assert_eq!((rule.id.as_str(), rule.series_key.as_deref()), (series.id.as_str(), Some("trip-1")));
        assert_eq!((rule.decision, rule.day_kind), (RuleDecision::Keep, Some(DayKind::Away)));
        let mut trip = event("home", "trip-1", "Trip", 0);
        trip.all_day = true;
        let block = busy_block(USER, &trip, Some(&rule));
        assert_eq!(block.day_kind, Some(DayKind::Away));
        assert_eq!(busy_block(USER, &trip, None).group_name, None);
        let timed = busy_block(USER, &event("home", "trip-1", "Trip", 9), Some(&rule));
        assert_eq!(timed.day_kind, None, "timed events never carry a day kind");
    }

    #[test]
    fn device_events_are_checked() {
        let ok = event("work", "evt-1", "Standup", 9);
        ok.validate().unwrap();
        let mut bad = ok.clone();
        bad.end_at = bad.start_at;
        assert!(bad.validate().is_err());
        let mut bad = ok.clone();
        bad.event_id = String::new();
        assert!(bad.validate().is_err());
        let mut bad = ok;
        bad.title = "t".repeat(MAX_TITLE + 1);
        assert!(bad.validate().is_err());
    }
}
