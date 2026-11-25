//! Profile facts (Q62, D7): the facts planning and the intake read as state `profile`, and
//! the `save_facts` tool that keeps lasting answers after the questions node without a
//! model call. Skipped and one-off answers are never kept; blocked topics never written.

use crate::db::PgPool;
use crate::error::ServerError;
use crate::rows::{list_rows, put_row};
use crate::tools::ToolEnv;
use aroh_domain::derived_id;
use aroh_domain::rows::{Goal, Preference, PreferenceOrigin};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use troupekit_tools::ToolError;
use uuid::Uuid;

/// Most facts the profile hands to planning and the intake, newest first.
pub const PROFILE_LIMIT: usize = 20;

const SOURCE: &str = "answer";
const MAX_TOPIC_CHARS: usize = 60;
const MAX_LABEL_CHARS: usize = 60;
const MAX_ANSWER_CHARS: usize = 500;

/// One profile fact as state `profile` holds it: row id, text, topic, where it came from
/// (the goal title for an answer, "your edits" for an edit) and its `YYYY-MM-DD` date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileFact {
    pub id: String,
    pub fact: String,
    pub topic: Option<String>,
    pub source: String,
    pub date: String,
}

/// The owner's confirmed facts, newest first, at most [`PROFILE_LIMIT`], blocked topics
/// left out. Errors only when the database does.
pub async fn profile(pool: &PgPool, user: Uuid) -> Result<Vec<ProfileFact>, ServerError> {
    let mut prefs: Vec<Preference> = list_rows::<Preference>(pool, user, &[], false).await?;
    prefs.retain(|p| p.confirmed && !p.blocked);
    prefs.sort_by(|a, b| b.noted_at.cmp(&a.noted_at));
    let goals: BTreeMap<String, String> = list_rows::<Goal>(pool, user, &[], false).await?.into_iter().map(|g| (g.id, g.title)).collect();
    Ok(prefs
        .into_iter()
        .take(PROFILE_LIMIT)
        .map(|p| {
            let source = match (p.origin, p.goal_id.as_ref().and_then(|g| goals.get(g))) {
                (PreferenceOrigin::Edit, _) => "your edits".to_string(),
                (PreferenceOrigin::Answer, Some(title)) => format!("your answer on {title}"),
                (PreferenceOrigin::Answer, None) => "your answers".to_string(),
            };
            ProfileFact { id: p.id, fact: p.fact, topic: p.topic, source, date: p.noted_at.date_naive().to_string() }
        })
        .collect())
}

/// One topic the owner chose never to have learned, as state `blocked_topics` holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockedTopic {
    pub topic: String,
    pub label: String,
}

/// The owner's blocked topics with their labels, so the intake never marks them lasting.
/// Errors only when the database does.
pub async fn blocked_topics(pool: &PgPool, user: Uuid) -> Result<Vec<BlockedTopic>, ServerError> {
    let prefs: Vec<Preference> = list_rows::<Preference>(pool, user, &[], false).await?;
    Ok(prefs.into_iter().filter(|p| p.blocked).filter_map(|p| Some(BlockedTopic { topic: p.topic?, label: p.fact })).collect())
}

/// What the intake says a question keeps: a snake_case `topic` and a short owner-facing `label`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Lasting {
    pub topic: String,
    pub label: String,
}

/// A question a profile fact already answers: the fact's id, its value and where it came from.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Known {
    pub fact_id: String,
    pub value: String,
    #[serde(default)]
    pub source: String,
}

/// One intake question as `save_facts` reads it; other fields are ignored.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FactQuestion {
    pub id: String,
    #[serde(default)]
    pub lasting: Option<Lasting>,
    #[serde(default)]
    pub known: Option<Known>,
}

/// The intake state: its questions.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FactIntake {
    pub questions: Vec<FactQuestion>,
}

/// One submitted answer; `None` when skipped.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FactAnswer {
    pub id: String,
    #[serde(default)]
    pub answer: Option<String>,
}

/// The `save_facts` tool input, read from the run state.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SaveFactsArgs {
    pub intake: FactIntake,
    pub answers: Vec<FactAnswer>,
}

/// One fact write an answer asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactWrite {
    /// A lasting answer: keep "<label>: <answer>" under `topic`.
    Keep { topic: String, label: String, answer: String },
    /// A known question answered: refresh `fact_id` (or the fact of `topic` when the model
    /// named a wrong id) when `same`, else set its answer.
    Known { fact_id: String, answer: String, same: bool, label: Option<String>, topic: Option<String> },
}

/// Cuts `text` to at most `max` characters after trimming.
fn cut(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect()
}

/// The fact writes `answers` ask for, in question order: a known question answered gives
/// `Known`, a lasting one gives `Keep` with a lowercased topic; skipped answers, blank
/// topics or labels and questions marked neither give nothing.
pub fn fact_writes(intake: &FactIntake, answers: &[FactAnswer]) -> Vec<FactWrite> {
    let given: BTreeMap<&str, &str> = answers.iter().filter_map(|a| Some((a.id.as_str(), a.answer.as_deref()?.trim()))).filter(|(_, a)| !a.is_empty()).collect();
    intake
        .questions
        .iter()
        .filter_map(|q| {
            let answer = cut(given.get(q.id.as_str())?, MAX_ANSWER_CHARS);
            let lasting = q.lasting.as_ref().map(|l| (cut(&l.topic, MAX_TOPIC_CHARS).to_lowercase(), cut(&l.label, MAX_LABEL_CHARS))).filter(|(t, l)| !t.is_empty() && !l.is_empty());
            if let Some(k) = &q.known {
                let same = k.value.trim().eq_ignore_ascii_case(&answer);
                let (topic, label) = lasting.map_or((None, None), |(t, l)| (Some(t), Some(l)));
                return Some(FactWrite::Known { fact_id: k.fact_id.clone(), answer, same, label, topic });
            }
            let (topic, label) = lasting?;
            Some(FactWrite::Keep { topic, label, answer })
        })
        .collect()
}

/// The row id a kept topic of `user` uses, so a topic holds one fact and replays write once.
pub fn fact_id(user: Uuid, topic: &str) -> String {
    derived_id(&user.to_string(), &format!("fact:{topic}"), 0)
}

/// Keeps the run's lasting answers as confirmed profile facts (D6, D7) with topic, source
/// goal and date; refreshes or updates the facts known questions came from. Blocked topics
/// and facts are never written, nor anything from a run that read the diary. A store error
/// skips the facts instead of failing the run. Returns `{kept, refreshed, updated, skipped}`.
pub async fn save_facts(env: &ToolEnv, args: SaveFactsArgs) -> Result<Value, ToolError> {
    if env.sealed {
        return Ok(json!({"kept": 0, "refreshed": 0, "updated": 0, "skipped": args.answers.len()}));
    }
    let mine: BTreeMap<String, Preference> = match list_rows::<Preference>(&env.pool, env.user, &[], false).await {
        Ok(rows) => rows.into_iter().map(|p| (p.id.clone(), p)).collect(),
        Err(e) => {
            eprintln!("aroh: profile facts not read, none kept: {e}");
            return Ok(json!({"kept": 0, "refreshed": 0, "updated": 0, "skipped": args.answers.len()}));
        }
    };
    let blocked: BTreeSet<&str> = mine.values().filter(|p| p.blocked).filter_map(|p| p.topic.as_deref()).collect();
    let now = crate::rows::utc(env.clock.now_ms());
    let (mut kept, mut refreshed, mut updated, mut skipped) = (0, 0, 0, 0);
    for w in fact_writes(&args.intake, &args.answers) {
        let mut row = match &w {
            FactWrite::Keep { topic, label, answer } => {
                if blocked.contains(topic.as_str()) {
                    continue;
                }
                let id = fact_id(env.user, topic);
                let mut row = mine.get(&id).cloned().unwrap_or_else(|| Preference {
                    id,
                    user_id: env.user.to_string(),
                    fact: String::new(),
                    source: SOURCE.into(),
                    noted_at: now,
                    confirmed: true,
                    topic: Some(topic.clone()),
                    goal_id: None,
                    origin: PreferenceOrigin::Answer,
                    blocked: false,
                    updated_at: String::new(),
                    deleted: false,
                });
                row.fact = format!("{label}: {answer}");
                row.goal_id = env.goal.map(|g| g.to_string());
                row.origin = PreferenceOrigin::Answer;
                kept += 1;
                row
            }
            FactWrite::Known { fact_id, answer, same, label, topic } => {
                let by_topic = topic.as_deref().map(|t| self::fact_id(env.user, t));
                let found = mine.get(fact_id).or_else(|| by_topic.and_then(|id| mine.get(&id)));
                let Some(mut row) = found.filter(|p| !p.blocked && p.confirmed).cloned() else { continue };
                if *same {
                    refreshed += 1;
                } else {
                    let label = label.clone().or_else(|| row.fact.split_once(": ").map(|(l, _)| l.to_string()));
                    row.fact = label.map_or_else(|| answer.clone(), |l| format!("{l}: {answer}"));
                    updated += 1;
                }
                row
            }
        };
        row.noted_at = now;
        row.confirmed = true;
        assert!(!row.blocked, "a blocked topic is never written as a fact");
        if let Err(e) = put_row(&env.pool, &env.hlc, env.user, &mut row).await {
            eprintln!("aroh: profile fact {} not kept: {e}", row.id);
            skipped += 1;
        }
    }
    Ok(json!({"kept": kept, "refreshed": refreshed, "updated": updated, "skipped": skipped}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(id: &str, lasting: Option<(&str, &str)>, known: Option<(&str, &str)>) -> FactQuestion {
        FactQuestion {
            id: id.into(),
            lasting: lasting.map(|(topic, label)| Lasting { topic: topic.into(), label: label.into() }),
            known: known.map(|(fact_id, value)| Known { fact_id: fact_id.into(), value: value.into(), source: String::new() }),
        }
    }

    fn a(id: &str, answer: Option<&str>) -> FactAnswer {
        FactAnswer { id: id.into(), answer: answer.map(str::to_string) }
    }

    #[test]
    fn only_answered_lasting_and_known_questions_write_facts() {
        let intake = FactIntake {
            questions: vec![
                q("minutes", Some(("Daily_Minutes", "Free minutes a day")), None),
                q("chapter", None, None),
                q("when", Some(("time_of_day", "Reading time of day")), None),
                q("pace", None, Some(("f1", "30 min"))),
                q("days", Some(("gym_days", "Gym days")), Some(("f2", "Mon, Wed"))),
                q("blank", Some(("  ", "x")), None),
            ],
        };
        let answers = [a("minutes", Some(" 30 min ")), a("chapter", Some("6")), a("when", None), a("pace", Some("30 MIN")), a("days", Some("Tue")), a("blank", Some("y"))];
        assert_eq!(
            fact_writes(&intake, &answers),
            vec![
                FactWrite::Keep { topic: "daily_minutes".into(), label: "Free minutes a day".into(), answer: "30 min".into() },
                FactWrite::Known { fact_id: "f1".into(), answer: "30 MIN".into(), same: true, label: None, topic: None },
                FactWrite::Known { fact_id: "f2".into(), answer: "Tue".into(), same: false, label: Some("Gym days".into()), topic: Some("gym_days".into()) },
            ],
            "a one-off answer, a skipped answer and a blank topic keep nothing"
        );
    }

    #[test]
    fn a_topic_keeps_one_row_per_owner() {
        let (u, v) = (Uuid::now_v7(), Uuid::now_v7());
        assert_eq!(fact_id(u, "daily_minutes"), fact_id(u, "daily_minutes"));
        assert_ne!(fact_id(u, "daily_minutes"), fact_id(v, "daily_minutes"));
        assert_ne!(fact_id(u, "daily_minutes"), fact_id(u, "gym_days"));
    }
}
