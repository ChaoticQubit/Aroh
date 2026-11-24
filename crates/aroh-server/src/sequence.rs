//! Replan checks on a goal's sessions (Q64, Q79, B63): no catch-up that repeats a
//! missed session, the daily cap on every day a change touches, and for an ordered
//! goal every missed session moved ahead in sequence with the rest shifted forward.

use crate::evals::parse_local;
use chrono::{NaiveDate, NaiveDateTime};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// One calendar entry of the replanned goal, in the goal's local wall time.
/// `status` is `planned`, `missed`, `done` or `skipped`.
/// Built from the replan's `calendar` state key.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub start: NaiveDateTime,
    pub minutes: i64,
    pub status: String,
    pub locked: bool,
}

/// What a replan of one goal is checked against: its sessions, whether the goal
/// keeps them in order, the owner's daily minutes and the local time now.
/// The default has no sessions, so every check passes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bounds {
    pub sessions: Vec<Session>,
    pub ordered: bool,
    pub cap: Option<i64>,
    pub now: NaiveDateTime,
}

impl Bounds {
    /// Reads the sessions from `calendar` (the replan's state list of `{id, title,
    /// start, minutes, status, locked}`); entries without an id are skipped.
    /// Returns the bounds, or why an entry does not parse.
    pub fn from_calendar(calendar: &Value, ordered: bool, cap: Option<i64>, now: NaiveDateTime) -> Result<Bounds, String> {
        let mut sessions = Vec::new();
        for entry in calendar.as_array().into_iter().flatten() {
            let Some(id) = entry.get("id").and_then(Value::as_str) else { continue };
            let text = |key: &str| entry.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
            sessions.push(Session {
                id: id.to_string(),
                title: text("title"),
                start: parse_local(&text("start"))?,
                minutes: entry.get("minutes").and_then(Value::as_i64).ok_or_else(|| format!("`{}` has no minutes", text("title")))?,
                status: text("status"),
                locked: entry.get("locked").and_then(Value::as_bool).unwrap_or(false),
            });
        }
        Ok(Bounds { sessions, ordered, cap: cap.filter(|c| *c > 0), now })
    }
}

fn norm(title: &str) -> String {
    title.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn open(s: &Session) -> bool {
    matches!(s.status.as_str(), "planned" | "missed")
}

/// Checks `changes` (each `{op, action_id, title, start, minutes}`, local starts)
/// against `bounds`. Rejects a move into the past, a create that repeats a missed
/// session's title, a day a change touches going over the cap, and for an ordered
/// goal a missed session left behind or any session landing out of sequence.
/// Returns why the changes fail, worded for the analyst's repair round.
pub fn check(bounds: &Bounds, changes: &[Value]) -> Result<(), String> {
    let mut after: BTreeMap<String, Option<(NaiveDateTime, Option<i64>)>> = BTreeMap::new();
    let mut created: Vec<(String, NaiveDateTime, i64)> = Vec::new();
    for c in changes {
        let field = |key: &str| c.get(key).and_then(Value::as_str);
        let title = field("title").unwrap_or("session");
        let start = field("start").map(parse_local).transpose()?;
        let minutes = c.get("minutes").and_then(Value::as_i64);
        match field("op").unwrap_or_default() {
            "create" => {
                let (Some(start), Some(minutes)) = (start, minutes) else {
                    return Err(format!("`{title}` needs start and minutes"));
                };
                created.push((title.to_string(), start, minutes));
            }
            op @ ("move" | "resize" | "delete") => {
                let id = field("action_id").ok_or_else(|| format!("`{title}` needs its action_id"))?;
                let span = match (op, start) {
                    ("delete", _) => None,
                    (_, Some(start)) => Some((start, minutes)),
                    (_, None) => return Err(format!("`{title}` needs a start")),
                };
                if op == "move" && span.is_some_and(|(start, _)| start < bounds.now) {
                    return Err(format!("`{title}` moves into the past; place it after now"));
                }
                if after.insert(id.to_string(), span).is_some() {
                    return Err(format!("`{title}` is changed twice; give each session one change"));
                }
            }
            other => return Err(format!("unknown op `{other}`")),
        }
    }
    let missed: Vec<&Session> = bounds.sessions.iter().filter(|s| s.status == "missed").collect();
    for (title, _, _) in &created {
        let t = norm(title);
        if let Some(s) = missed.iter().find(|s| {
            let n = norm(&s.title);
            !n.is_empty() && !t.is_empty() && (t.contains(&n) || n.contains(&t))
        }) {
            return Err(format!(
                "`{title}` repeats the missed session `{}`: move that session (action_id {}) instead of creating a catch-up",
                s.title, s.id
            ));
        }
    }
    let placed = |s: &Session| -> Option<(NaiveDateTime, i64)> {
        match after.get(&s.id) {
            Some(None) => None,
            Some(Some((start, minutes))) => Some((*start, minutes.unwrap_or(s.minutes))),
            None if s.status == "missed" || s.status == "skipped" => None,
            None => Some((s.start, s.minutes)),
        }
    };
    if bounds.ordered {
        for s in missed.iter().filter(|s| !s.locked) {
            match after.get(&s.id) {
                Some(None) => {}
                Some(Some((start, _))) if *start >= bounds.now => {}
                Some(Some(_)) => return Err(format!("`{}` moves into the past; place it after now", s.title)),
                None => {
                    return Err(format!(
                        "`{}` was missed: this goal keeps its sessions in order, so move it to the next free slot and shift the later sessions forward",
                        s.title
                    ))
                }
            }
        }
        let mut sequence: Vec<&Session> = bounds.sessions.iter().filter(|s| open(s) && !s.locked).collect();
        sequence.sort_by(|a, b| (a.start, &a.id).cmp(&(b.start, &b.id)));
        let landed: Vec<(&Session, NaiveDateTime)> = sequence.iter().filter_map(|s| placed(s).map(|(start, _)| (*s, start))).collect();
        assert!(landed.len() <= sequence.len(), "only sessions of the sequence land");
        for pair in landed.windows(2) {
            if pair[1].1 <= pair[0].1 {
                return Err(format!(
                    "Keep the sequence: `{}` must come before `{}`. Place the missed sessions first, in order, and shift every later session forward.",
                    pair[0].0.title, pair[1].0.title
                ));
            }
        }
    }
    if let Some(cap) = bounds.cap {
        let mut touched: BTreeSet<NaiveDate> = created.iter().map(|(_, start, _)| start.date()).collect();
        touched.extend(after.values().flatten().map(|(start, _)| start.date()));
        let mut per_day: BTreeMap<NaiveDate, i64> = BTreeMap::new();
        for s in &bounds.sessions {
            if let Some((start, minutes)) = placed(s) {
                *per_day.entry(start.date()).or_default() += minutes;
            }
        }
        for (_, start, minutes) in &created {
            *per_day.entry(start.date()).or_default() += minutes;
        }
        for day in touched {
            let total = per_day.get(&day).copied().unwrap_or(0);
            if total > cap {
                return Err(format!("{day} holds {total} minutes of this goal, over the daily cap of {cap}; move a session to another day"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn at(text: &str) -> NaiveDateTime {
        parse_local(text).unwrap()
    }

    fn session(id: &str, title: &str, start: &str, status: &str) -> Session {
        Session { id: id.into(), title: title.into(), start: at(start), minutes: 30, status: status.into(), locked: false }
    }

    fn reading(ordered: bool) -> Bounds {
        Bounds {
            sessions: vec![
                session("a2", "Chapters 1-2", "2026-11-22T20:00:00", "done"),
                session("a3", "Chapters 3-6", "2026-11-23T08:00:00", "missed"),
                session("a4", "Chapters 7-9", "2026-11-23T12:00:00", "missed"),
                session("a5", "Chapters 10-12", "2026-11-23T16:00:00", "missed"),
                session("a6", "Chapters 12-14", "2026-11-24T20:00:00", "planned"),
                session("a7", "Chapters 15-17", "2026-11-25T20:00:00", "planned"),
            ],
            ordered,
            cap: Some(30),
            now: at("2026-11-23T21:00:00"),
        }
    }

    fn mv(id: &str, title: &str, start: &str) -> Value {
        json!({"op": "move", "action_id": id, "title": title, "start": start, "minutes": 30})
    }

    fn owner_b63_proposal() -> Vec<Value> {
        vec![
            json!({"op": "create", "action_id": null, "title": "Catch up: chapters 3-6", "start": "2026-11-24T07:00:00", "minutes": 30}),
            mv("a4", "Chapters 7-9", "2026-11-26T20:00:00"),
            mv("a5", "Chapters 10-12", "2026-11-28T20:00:00"),
        ]
    }

    fn shifted() -> Vec<Value> {
        vec![
            mv("a3", "Chapters 3-6", "2026-11-24T20:00:00"),
            mv("a4", "Chapters 7-9", "2026-11-25T20:00:00"),
            mv("a5", "Chapters 10-12", "2026-11-26T20:00:00"),
            mv("a6", "Chapters 12-14", "2026-11-27T20:00:00"),
            mv("a7", "Chapters 15-17", "2026-11-28T20:00:00"),
        ]
    }

    #[test]
    fn owner_reading_replan_of_b63_is_rejected() {
        let err = check(&reading(true), &owner_b63_proposal()).unwrap_err();
        assert!(err.contains("repeats the missed session `Chapters 3-6`"), "{err}");
        let without_catch_up = &owner_b63_proposal()[1..];
        let err = check(&reading(true), without_catch_up).unwrap_err();
        assert!(err.contains("`Chapters 3-6` was missed"), "{err}");
        let mut late = shifted();
        late[0] = mv("a3", "Chapters 3-6", "2026-11-29T20:00:00");
        let err = check(&reading(true), &late).unwrap_err();
        assert!(err.contains("Keep the sequence: `Chapters 3-6` must come before `Chapters 7-9`"), "{err}");
    }

    #[test]
    fn shifting_the_sequence_forward_passes() {
        assert_eq!(check(&reading(true), &shifted()), Ok(()));
    }

    #[test]
    fn moves_into_the_past_and_over_the_cap_fail() {
        let mut past = shifted();
        past[0] = mv("a3", "Chapters 3-6", "2026-11-23T20:30:00");
        assert!(check(&reading(true), &past).unwrap_err().contains("into the past"));
        let mut doubled = shifted();
        doubled[1] = mv("a4", "Chapters 7-9", "2026-11-24T21:00:00");
        doubled.truncate(2);
        let err = check(&reading(false), &doubled).unwrap_err();
        assert!(err.contains("2026-11-24 holds 90 minutes"), "{err}");
    }

    #[test]
    fn unordered_goal_is_not_forced_into_order_but_gets_no_duplicate() {
        let workout = Bounds {
            sessions: vec![
                session("w1", "Leg day", "2026-11-23T07:00:00", "missed"),
                session("w2", "Upper body", "2026-11-24T07:00:00", "planned"),
                session("w3", "Core", "2026-11-25T07:00:00", "planned"),
            ],
            ordered: false,
            cap: Some(45),
            now: at("2026-11-23T21:00:00"),
        };
        assert_eq!(check(&workout, &[mv("w1", "Leg day", "2026-11-26T07:00:00")]), Ok(()), "a missed workout may land after later ones");
        assert_eq!(check(&workout, &[]), Ok(()), "an unordered goal may leave a miss alone");
        assert!(check(&workout, &[mv("w1", "Leg day", "2026-11-23T20:00:00")]).unwrap_err().contains("into the past"));
        let dup = json!({"op": "create", "title": "Leg day (catch-up)", "start": "2026-11-26T07:00:00", "minutes": 30});
        assert!(check(&workout, &[dup]).unwrap_err().contains("repeats the missed session `Leg day`"));
    }

    #[test]
    fn locked_sessions_stay_out_of_the_sequence_check() {
        let mut b = reading(true);
        b.sessions[5].locked = true;
        let moves = [
            mv("a3", "Chapters 3-6", "2026-11-24T20:00:00"),
            mv("a4", "Chapters 7-9", "2026-11-26T20:00:00"),
            mv("a5", "Chapters 10-12", "2026-11-27T20:00:00"),
            mv("a6", "Chapters 12-14", "2026-11-28T20:00:00"),
        ];
        assert_eq!(check(&b, &moves), Ok(()), "the locked 15-17 keeps its slot though 12-14 now follows it");
    }

    #[test]
    fn calendar_state_parses_and_skips_entries_without_id() {
        let calendar = json!([
            {"id": "a3", "title": "Chapters 3-6", "start": "2026-11-23T08:00:00", "minutes": 30, "status": "missed", "locked": false},
            {"title": "Other goal", "start": "2026-11-23T09:00:00", "minutes": 30}
        ]);
        let b = Bounds::from_calendar(&calendar, true, Some(0), at("2026-11-23T21:00:00")).unwrap();
        assert_eq!(b.sessions, vec![session("a3", "Chapters 3-6", "2026-11-23T08:00:00", "missed")]);
        assert_eq!(b.cap, None, "a zero cap is no cap");
    }
}
