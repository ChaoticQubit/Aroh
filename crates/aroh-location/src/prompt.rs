//! Place prompts (Q56): a spot visited on several days that is not a named or ignored place
//! becomes one "Is this your gym?" question in the Inbox.

use crate::geo::{distance_m, NEAR_M};
use crate::Visit;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc, Weekday};
use chrono_tz::Tz;
use std::collections::BTreeSet;

/// Distinct local days of visits a spot needs before Aroh asks about it.
pub const PROMPT_DAYS: usize = 3;

/// Days of visits a prompt looks back on.
pub const LOOKBACK_DAYS: i64 = 21;

/// Shortest stay that counts as a visit; shorter ones are passing by.
pub const MIN_STAY_MIN: i64 = 5;

/// A spot worth asking about: its center, how many visits on which local days, and the
/// arrival instants, oldest first.
#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    pub lat_e6: i64,
    pub lon_e6: i64,
    pub visits: usize,
    pub days: Vec<NaiveDate>,
    pub arrivals: Vec<DateTime<Utc>>,
}

fn stayed(v: &Visit, now: DateTime<Utc>) -> bool {
    v.left_at.unwrap_or(now) - v.arrived_at >= Duration::minutes(MIN_STAY_MIN)
}

/// Groups the visits of the last [`LOOKBACK_DAYS`] days (stays of at least [`MIN_STAY_MIN`]
/// minutes) into spots of [`NEAR_M`] around their first visit, and keeps spots visited on at
/// least [`PROMPT_DAYS`] local days in `zone` that are not near any of `known` (named or
/// ignored places, pending prompts). Output: spots, most days first.
pub fn prompt_clusters(visits: &[Visit], known: &[(i64, i64)], now: DateTime<Utc>, zone: Tz) -> Vec<Cluster> {
    let since = now - Duration::days(LOOKBACK_DAYS);
    let mut recent: Vec<&Visit> = visits.iter().filter(|v| v.arrived_at >= since && v.arrived_at <= now && stayed(v, now)).collect();
    recent.sort_by_key(|v| v.arrived_at);
    let mut groups: Vec<((i64, i64), Vec<&Visit>)> = Vec::new();
    for v in recent {
        let at = (v.lat_e6, v.lon_e6);
        match groups.iter_mut().find(|(center, _)| distance_m(*center, at) <= NEAR_M) {
            Some((_, members)) => members.push(v),
            None => groups.push((at, vec![v])),
        }
    }
    let mut out: Vec<Cluster> = groups
        .into_iter()
        .filter(|(center, _)| known.iter().all(|k| distance_m(*k, *center) > NEAR_M))
        .map(|((lat_e6, lon_e6), members)| {
            let days: BTreeSet<NaiveDate> = members.iter().map(|v| v.arrived_at.with_timezone(&zone).date_naive()).collect();
            Cluster { lat_e6, lon_e6, visits: members.len(), days: days.into_iter().collect(), arrivals: members.iter().map(|v| v.arrived_at).collect() }
        })
        .filter(|c| c.days.len() >= PROMPT_DAYS)
        .collect();
    out.sort_by(|a, b| b.days.len().cmp(&a.days.len()).then(b.visits.cmp(&a.visits)));
    out
}

/// The word Aroh guesses for a place from its MapKit point-of-interest category, as in
/// "Is this your gym?". `None` when the category gives no good guess.
pub fn guess(poi_kind: Option<&str>) -> Option<&'static str> {
    Some(match poi_kind? {
        "MKPOICategoryFitnessCenter" => "gym",
        "MKPOICategoryFoodMarket" => "grocery store",
        "MKPOICategoryLibrary" => "library",
        "MKPOICategoryCafe" => "cafe",
        "MKPOICategoryPark" => "park",
        "MKPOICategorySchool" | "MKPOICategoryUniversity" => "school",
        _ => return None,
    })
}

fn weekday_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

/// Joins words as "a", "a and b" or "a, b and c".
pub fn and_list(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// When the owner usually comes, from arrival instants in `zone`: the weekdays seen at least
/// twice (all of them when none repeats or most days repeat) and the median arrival rounded
/// to 15 minutes, as "usually Monday, Wednesday and Friday around 18:00".
pub fn usual(arrivals: &[DateTime<Utc>], zone: Tz) -> String {
    assert!(!arrivals.is_empty(), "a usual time needs visits");
    let local: Vec<_> = arrivals.iter().map(|a| a.with_timezone(&zone)).collect();
    let mut counts = [0usize; 7];
    for l in &local {
        counts[l.weekday().num_days_from_monday() as usize] += 1;
    }
    let repeated: Vec<usize> = (0..7).filter(|d| counts[*d] >= 2).collect();
    let mut minutes: Vec<u32> = local.iter().map(|l| l.hour() * 60 + l.minute()).collect();
    minutes.sort_unstable();
    let median = minutes[minutes.len() / 2];
    let rounded = ((median + 7) / 15 * 15) % 1440;
    let time = format!("{:02}:{:02}", rounded / 60, rounded % 60);
    if repeated.is_empty() || repeated.len() >= 5 {
        return format!("usually around {time}");
    }
    let names: Vec<&str> = repeated.iter().map(|d| weekday_name(Weekday::try_from(*d as u8).expect("0 to 6 is a weekday"))).collect();
    format!("usually {} around {time}", and_list(&names))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn usual_names_repeated_weekdays_and_the_rounded_median() {
        let at = |d: u32, h: u32, m: u32| Utc.with_ymd_and_hms(2026, 11, d, h, m, 0).unwrap();
        let arrivals = [at(16, 18, 2), at(18, 17, 58), at(20, 18, 5), at(23, 18, 1), at(25, 18, 9), at(27, 17, 55)];
        assert_eq!(usual(&arrivals, Tz::UTC), "usually Monday, Wednesday and Friday around 18:00");
        assert_eq!(usual(&[at(16, 9, 0)], Tz::UTC), "usually around 09:00");
        assert_eq!(and_list(&["a", "b"]), "a and b");
    }
}
