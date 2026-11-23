use crate::checkpoints::{EventBuffer, PgCheckpoints};
use crate::db::{self, PgPool};
use crate::error::ServerError;
use crate::workflows::{LEARN_PREFERENCE, REPLAN};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Map, Value};
use std::sync::Arc;
use troupekit_core::ids::RunId;
use troupekit_core::store::CheckpointStore;
use troupekit_graph::GraphRun;
use utoipa::ToSchema;
use uuid::Uuid;

const INSIGHT_LIMIT: i64 = 30;

/// One Inbox insight: what the engine noticed and did without being
/// asked. `source` is `actions`, `rule`, `review`, `edits` or `calendar`; `body` is
/// the reason plus the run summary; `state` the run's state, if any.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Insight {
    pub id: Uuid,
    pub goal_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub source: String,
    pub title: String,
    pub body: String,
    pub state: Option<String>,
    pub created_at: DateTime<Utc>,
}

fn title(source: &str) -> &'static str {
    match source {
        "actions" => "Missed or late actions",
        "rule" => "Tracker rule",
        "review" => "Weekly review",
        "calendar" => "Away days",
        _ => "Repeated edits",
    }
}

/// One question of a run with the owner's answer, `None` when skipped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RunAnswer {
    pub id: String,
    pub text: String,
    pub answer: Option<String>,
}

/// What a new-goal run found and was told, from its `state`: the `research` object
/// (`summary` and `sources`) when written, and each submitted answer with its
/// question text, in question order. Empty for runs without them.
pub fn run_found(state: &Map<String, Value>) -> (Option<Value>, Vec<RunAnswer>) {
    let research = state.get("research").filter(|r| r.is_object()).cloned();
    let given: Map<String, Value> = state
        .get("answers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|a| Some((a.get("id")?.as_str()?.to_string(), a.get("answer").cloned().unwrap_or(Value::Null))))
        .collect();
    let questions = state.get("intake").and_then(|i| i.get("questions")).and_then(Value::as_array).into_iter().flatten();
    let answers = questions
        .filter_map(|q| {
            let id = q.get("id")?.as_str()?;
            let answer = given.get(id)?;
            Some(RunAnswer { id: id.into(), text: q.get("text").and_then(Value::as_str).unwrap_or(id).into(), answer: answer.as_str().map(str::to_string) })
        })
        .collect();
    (research, answers)
}

/// A one-line summary of a run from its `workflow` and state: the
/// proposal and its outcome (applied, rejected, waiting, no change) for
/// replans, the proposed fact for learning runs. `None` before either.
pub fn run_summary(workflow: &str, state: &Map<String, Value>) -> Option<String> {
    let text = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_string);
    if workflow == LEARN_PREFERENCE {
        return text(state.get("preference").and_then(|p| p.get("fact"))).map(|f| format!("Proposed preference: {f}"));
    }
    if workflow != REPLAN {
        return None;
    }
    let record = state.get("proposal_record")?;
    let summary = text(record.get("summary")).unwrap_or_default();
    let changes = record.get("changes").and_then(Value::as_array).map_or(0, Vec::len);
    if changes == 0 {
        return Some(format!("No change needed. {summary}").trim_end().to_string());
    }
    let outcome = state.get("outcome");
    let line = match outcome.and_then(|o| o.get("status")).and_then(Value::as_str) {
        Some("applied") => {
            let n = |k: &str| outcome.and_then(|o| o.get(k)).and_then(Value::as_i64).unwrap_or(0);
            let skipped = n("skipped");
            let kept = if skipped > 0 { format!(", {skipped} locked kept") } else { String::new() };
            format!("{summary} Applied {} of {changes} changes{kept}.", n("applied"))
        }
        Some("rejected") => format!("{summary} Rejected."),
        _ => format!("{summary} Waiting for your approval."),
    };
    Some(line)
}

/// Reads the latest checkpoint of `run` and returns its summary.
/// `None` when the run has no checkpoint or no summary yet.
/// Errors on a store failure or a corrupt checkpoint.
pub async fn summary_of(pool: &PgPool, run: RunId, workflow: &str) -> Result<Option<String>, ServerError> {
    let store = PgCheckpoints::new(pool.clone(), Arc::new(EventBuffer::default()));
    let Some(cp) = store.latest(run).await? else { return Ok(None) };
    Ok(run_summary(workflow, GraphRun::from_checkpoint(&cp)?.state()))
}

/// The 30 newest insights of `user`, of goal `goal` when given, newest
/// first. A start that failed shows its error instead of a summary.
/// Never another user's rows.
pub async fn list(pool: &PgPool, user: Uuid, goal: Option<Uuid>) -> Result<Vec<Insight>, ServerError> {
    let rows: Vec<(Uuid, Option<Uuid>, Option<Uuid>, String, String, Option<String>, DateTime<Utc>, Option<String>, Option<String>)> =
        db::query_as(
            "select a.id, a.goal_id, a.run_id, a.source, a.reason, a.error, a.created_at, r.state, r.workflow
             from public.adaptations a left join public.runs r on r.id = a.run_id and r.user_id = a.user_id
             where a.user_id = $1 and ($2::uuid is null or a.goal_id = $2)
             order by a.created_at desc, a.id desc limit $3",
        )
        .bind(user)
        .bind(goal)
        .bind(INSIGHT_LIMIT)
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, goal_id, run_id, source, reason, error, created_at, state, workflow) in rows {
        let summary = match (run_id, workflow.as_deref()) {
            (Some(r), Some(w)) => summary_of(pool, RunId(r), w).await?,
            _ => None,
        };
        let tail = match (&error, summary) {
            (Some(e), _) => Some(format!("Could not start: {e}")),
            (None, s) => s,
        };
        let body = tail.map_or(reason.clone(), |t| format!("{reason} {t}"));
        out.push(Insight { id, goal_id, run_id, title: title(&source).into(), source, body, state, created_at });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn state(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn found_and_answers_come_from_the_run_state() {
        assert_eq!(run_found(&state(json!({}))), (None, vec![]));
        let s = state(json!({
            "research": {"summary": "24 chapters.", "sources": [{"title": "Publisher", "url": "https://example.com"}]},
            "intake": {"questions": [{"id": "pace", "text": "Minutes a day?"}, {"id": "when", "text": "When?"}, {"id": "late", "text": "Asked later"}]},
            "answers": [{"id": "pace", "answer": "30"}, {"id": "when", "answer": null}]
        }));
        let (research, answers) = run_found(&s);
        assert_eq!(research.unwrap()["summary"], "24 chapters.");
        assert_eq!(
            answers,
            vec![
                RunAnswer { id: "pace".into(), text: "Minutes a day?".into(), answer: Some("30".into()) },
                RunAnswer { id: "when".into(), text: "When?".into(), answer: None },
            ],
            "only answered or skipped questions, in question order"
        );
    }

    #[test]
    fn run_summary_per_outcome() {
        let record = json!({"summary": "Move the run to Monday.", "changes": [{"op": "move"}, {"op": "move"}]});
        assert_eq!(run_summary(REPLAN, &state(json!({}))), None);
        assert_eq!(
            run_summary(REPLAN, &state(json!({"proposal_record": record}))).unwrap(),
            "Move the run to Monday. Waiting for your approval."
        );
        let applied = json!({"proposal_record": record, "outcome": {"status": "applied", "applied": 1, "skipped": 1}});
        assert_eq!(run_summary(REPLAN, &state(applied)).unwrap(), "Move the run to Monday. Applied 1 of 2 changes, 1 locked kept.");
        let rejected = json!({"proposal_record": record, "outcome": {"status": "rejected", "applied": 0, "skipped": 0}});
        assert_eq!(run_summary(REPLAN, &state(rejected)).unwrap(), "Move the run to Monday. Rejected.");
        let none = json!({"proposal_record": {"summary": "On track.", "changes": []}});
        assert_eq!(run_summary(REPLAN, &state(none)).unwrap(), "No change needed. On track.");
        let learned = json!({"preference": {"fact": "Plan workouts in the evening."}});
        assert_eq!(run_summary(LEARN_PREFERENCE, &state(learned)).unwrap(), "Proposed preference: Plan workouts in the evening.");
        assert_eq!(run_summary("new-goal", &state(json!({"proposal_record": record}))), None);
    }
}
