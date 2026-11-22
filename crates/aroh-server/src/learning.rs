use crate::db::PgPool;
use crate::error::ServerError;
use crate::rows::{insert_derived, list_rows};
use crate::tools::{local_text, ToolEnv};
use aroh_domain::derived_id;
use aroh_domain::rows::Preference;
use chrono::{DateTime, Timelike};
use chrono_tz::Tz;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use troupekit_tools::ToolError;
use uuid::Uuid;

/// Similar edits needed before the analyst proposes a preference (Q37).
/// One edit only locks its action.
/// Counted per owner across goals.
pub const LEARN_MIN_EDITS: usize = 3;

const MAX_FACT_CHARS: usize = 280;
const SOURCE: &str = "Preference analyst";

/// One owner edit of an action as the learner sees it: event id, kind
/// (`move`, `resize`, `delete`), title, local start and end before and
/// after (none after a delete), and its similarity key.
#[derive(Debug, Clone, PartialEq)]
pub struct EditEvent {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub before: (DateTime<Tz>, DateTime<Tz>),
    pub after: Option<(DateTime<Tz>, DateTime<Tz>)>,
    pub key: String,
}

fn span(v: &Value, tz: Tz) -> Option<(DateTime<Tz>, DateTime<Tz>)> {
    let t = |k: &str| {
        v.get(k).and_then(Value::as_str).and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.with_timezone(&tz))
    };
    Some((t("start_at")?, t("end_at")?))
}

fn part(t: DateTime<Tz>) -> &'static str {
    match t.hour() {
        5..=11 => "morning",
        12..=16 => "afternoon",
        17..=21 => "evening",
        _ => "night",
    }
}

/// The similarity key of one edit in local time: `move:<part>-><part>`
/// by part of day (morning 05-12, afternoon 12-17, evening 17-22,
/// night), `resize:shorter` or `resize:longer`, or `delete:<weekday>`.
pub fn edit_key(kind: &str, before: (DateTime<Tz>, DateTime<Tz>), after: Option<(DateTime<Tz>, DateTime<Tz>)>) -> Option<String> {
    match (kind, after) {
        ("move", Some(a)) => Some(format!("move:{}->{}", part(before.0), part(a.0))),
        ("resize", Some(a)) => {
            let (old, new) = (before.1 - before.0, a.1 - a.0);
            (old != new).then(|| if new < old { "resize:shorter" } else { "resize:longer" }.to_string())
        }
        ("delete", _) => Some(format!("delete:{}", before.0.format("%a"))),
        _ => None,
    }
}

impl EditEvent {
    /// Builds the edit of `action_events` row `id` of `kind` from its
    /// `before` and `after` objects in zone `tz`.
    /// Returns `None` for a row that is not a usable edit.
    pub fn from_row(id: i64, kind: &str, before: Option<&Value>, after: &Value, tz: Tz) -> Option<EditEvent> {
        let before_span = span(before?, tz)?;
        let after_span = if kind == "delete" { None } else { Some(span(after, tz)?) };
        let key = edit_key(kind, before_span, after_span)?;
        let title = after.get("title").and_then(Value::as_str).unwrap_or("Action").to_string();
        Some(EditEvent { id, kind: kind.to_string(), title, before: before_span, after: after_span, key })
    }
}

/// A group of similar edits of one owner: the shared key and the edits,
/// oldest first. Only groups of `LEARN_MIN_EDITS` or more are formed.
/// Input of one learning run.
#[derive(Debug, Clone, PartialEq)]
pub struct EditGroup {
    pub key: String,
    pub edits: Vec<EditEvent>,
}

/// Groups `edits` by key and keeps groups of at least `LEARN_MIN_EDITS`,
/// in key order with each group's edits in input order.
/// Returns the groups.
pub fn similar_groups(edits: &[EditEvent]) -> Vec<EditGroup> {
    let mut by_key: BTreeMap<&str, Vec<EditEvent>> = BTreeMap::new();
    for e in edits {
        by_key.entry(e.key.as_str()).or_default().push(e.clone());
    }
    by_key
        .into_iter()
        .filter(|(_, v)| v.len() >= LEARN_MIN_EDITS)
        .map(|(k, edits)| EditGroup { key: k.to_string(), edits })
        .collect()
}

/// Plain words for key `key`, such as `moved actions from the morning to
/// the evening`, `made actions shorter` or `deleted actions on Sat`.
/// Unknown keys are returned as they are.
pub fn describe(key: &str) -> String {
    if let Some(rest) = key.strip_prefix("move:") {
        if let Some((from, to)) = rest.split_once("->") {
            return if from == to { format!("moved actions within the {from}") } else { format!("moved actions from the {from} to the {to}") };
        }
    }
    match key {
        "resize:shorter" => "made actions shorter".into(),
        "resize:longer" => "made actions longer".into(),
        _ => key.strip_prefix("delete:").map(|d| format!("deleted actions on {d}")).unwrap_or_else(|| key.to_string()),
    }
}

/// The adaptation reason of `group`, such as `3 similar edits: moved
/// actions from the morning to the evening.` Shown as the insight.
/// Returns the text.
pub fn reason(group: &EditGroup) -> String {
    format!("{} similar edits: {}.", group.edits.len(), describe(&group.key))
}

/// The learning run input of `group`: each edit with local times and the
/// pattern key and words, as the `learn-preference` workflow reads them.
/// Returns `(edits, pattern)`.
pub fn run_input(group: &EditGroup) -> (Value, Value) {
    let edits: Vec<Value> = group
        .edits
        .iter()
        .map(|e| {
            json!({
                "title": e.title,
                "op": e.kind,
                "before": local_text(e.before.0.naive_local()),
                "after": e.after.map(|a| local_text(a.0.naive_local())),
            })
        })
        .collect();
    (json!(edits), json!({"key": group.key, "text": describe(&group.key)}))
}

/// The preference row id of `user` for pattern `key`: derived, so the
/// same pattern always maps to one row and a rejected one (a tombstone)
/// is never proposed again. Returns the id.
pub fn preference_id(user: Uuid, key: &str) -> String {
    derived_id(&user.to_string(), &format!("preference:{key}"), 0)
}

/// Whether `user` already has the preference of pattern `key`, proposed,
/// confirmed or rejected. Such edits are consumed without a run.
/// Returns true when the row exists, tombstones included.
pub async fn preference_known(pool: &PgPool, user: Uuid, key: &str) -> Result<bool, ServerError> {
    let id = preference_id(user, key);
    Ok(!list_rows::<Preference>(pool, user, &[("id", id)], true).await?.is_empty())
}

/// The pattern state key of a learning run: the edit key and its words.
/// The tool derives the preference id from `key`.
/// Read from the workflow state.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PatternInput {
    pub key: String,
    pub text: String,
}

/// The analyst's output: one preference fact in the owner's terms.
/// Matches the `preference` state key of `learn-preference`.
/// At most 280 characters.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PreferenceInput {
    pub fact: String,
}

/// Arguments of `propose_preference`: the pattern and the analyst's fact.
/// Both come from the run state, never from free model text.
/// Deserialized by the typed tool.
#[derive(Deserialize, JsonSchema)]
pub struct ProposeArgs {
    pub pattern: PatternInput,
    pub preference: PreferenceInput,
}

/// Writes one unconfirmed preference for the run's user from `args`,
/// insert-or-ignore under the pattern's derived id; the Inbox shows it
/// as a suggestion. Returns `{preference_id, new}`.
pub async fn propose_preference(env: &ToolEnv, args: ProposeArgs) -> Result<Value, ToolError> {
    let fact = args.preference.fact.trim();
    if fact.is_empty() || fact.chars().count() > MAX_FACT_CHARS {
        return Err(ToolError::InvalidArgs(format!("a preference has 1 to {MAX_FACT_CHARS} characters")));
    }
    let row = Preference {
        id: preference_id(env.user, &args.pattern.key),
        user_id: env.user.to_string(),
        fact: fact.to_string(),
        source: SOURCE.into(),
        noted_at: crate::rows::utc(env.clock.now_ms()),
        confirmed: false,
        updated_at: env.hlc.stamp_after("").map_err(|e| ToolError::Failed(e.to_string()))?,
        deleted: false,
    };
    let new = insert_derived(&env.pool, env.user, &row).await.map_err(|e| ToolError::Failed(e.to_string()))?;
    Ok(json!({"preference_id": row.id, "new": new}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const BERLIN: Tz = chrono_tz::Europe::Berlin;

    fn local(h: u32, m: u32, day: u32) -> DateTime<Tz> {
        BERLIN.with_ymd_and_hms(2026, 11, day, h, m, 0).unwrap()
    }

    fn edit(id: i64, kind: &str, before: (u32, u32), after: Option<(u32, u32)>, day: u32) -> EditEvent {
        let b = (local(before.0, 0, day), local(before.0, 0, day) + chrono::Duration::minutes(i64::from(before.1)));
        let a = after.map(|(h, m)| (local(h, 0, day), local(h, 0, day) + chrono::Duration::minutes(i64::from(m))));
        EditEvent { id, kind: kind.into(), title: "Workout".into(), before: b, after: a, key: edit_key(kind, b, a).unwrap() }
    }

    #[test]
    fn edit_keys_group_similar_edits() {
        assert_eq!(edit(1, "move", (7, 30), Some((18, 30)), 23).key, "move:morning->evening");
        assert_eq!(edit(1, "move", (7, 30), Some((9, 30)), 23).key, "move:morning->morning");
        assert_eq!(edit(1, "move", (23, 30), Some((13, 30)), 23).key, "move:night->afternoon");
        assert_eq!(edit(1, "resize", (7, 45), Some((7, 30)), 23).key, "resize:shorter");
        assert_eq!(edit(1, "resize", (7, 30), Some((7, 60)), 23).key, "resize:longer");
        assert_eq!(edit(1, "delete", (7, 30), None, 28).key, "delete:Sat");
        let b = (local(7, 0, 23), local(7, 30, 23));
        assert_eq!(edit_key("resize", b, Some(b)), None);
        assert_eq!(edit_key("move", b, None), None);

        let edits = vec![
            edit(1, "move", (7, 30), Some((18, 30)), 23),
            edit(2, "move", (6, 30), Some((19, 30)), 24),
            edit(3, "delete", (7, 30), None, 28),
            edit(4, "move", (8, 30), Some((20, 30)), 25),
            edit(5, "move", (7, 30), Some((13, 30)), 26),
        ];
        assert!(similar_groups(&edits[..2]).is_empty(), "two edits only lock");
        let groups = similar_groups(&edits);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].key, "move:morning->evening");
        assert_eq!(groups[0].edits.iter().map(|e| e.id).collect::<Vec<_>>(), [1, 2, 4]);
        assert_eq!(reason(&groups[0]), "3 similar edits: moved actions from the morning to the evening.");
        let (input, pattern) = run_input(&groups[0]);
        assert_eq!(input[0]["before"], "2026-11-23T07:00:00");
        assert_eq!(input[0]["after"], "2026-11-23T18:00:00");
        assert_eq!(pattern["key"], "move:morning->evening");
        assert_eq!(describe("delete:Sat"), "deleted actions on Sat");
    }

    #[test]
    fn rows_become_edits_in_local_time() {
        let before = json!({"title": "Core", "start_at": "2026-11-23T06:00:00Z", "end_at": "2026-11-23T06:30:00Z"});
        let after = json!({"title": "Core", "start_at": "2026-11-23T17:00:00Z", "end_at": "2026-11-23T17:30:00Z"});
        let e = EditEvent::from_row(9, "move", Some(&before), &after, BERLIN).unwrap();
        assert_eq!((e.key.as_str(), e.title.as_str()), ("move:morning->evening", "Core"));
        assert!(EditEvent::from_row(9, "move", None, &after, BERLIN).is_none());
        let deleted = EditEvent::from_row(9, "delete", Some(&before), &json!({"title": "Core"}), BERLIN).unwrap();
        assert_eq!(deleted.key, "delete:Mon");
        let user = Uuid::now_v7();
        assert_eq!(preference_id(user, "move:a->b"), preference_id(user, "move:a->b"));
        assert_ne!(preference_id(user, "move:a->b"), preference_id(user, "move:b->a"));
    }
}
