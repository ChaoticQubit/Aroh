use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, SecondsFormat, TimeDelta, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::fetch::CourseText;
use crate::model::{Action, ActionStatus, Goal, Plan, PlanStatus};
use crate::template::{builtin, render};
use crate::{AgentError, Claude};

const MIN_QUESTIONS: usize = 5;
const MAX_QUESTIONS: usize = 10;

/// One module or lesson of a course with its estimated study time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CourseModule {
    pub title: String,
    pub minutes: i64,
}

/// What the course analyzer found: the modules and their total minutes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CourseAnalysis {
    pub title: String,
    pub modules: Vec<CourseModule>,
    pub total_minutes: i64,
}

/// One intake question. Empty `options` means a free-text answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub text: String,
    pub options: Vec<String>,
}

/// One action as the planner proposes it, before it becomes an `Action` row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedAction {
    pub title: String,
    pub notes: Option<String>,
    pub start_at: String,
    pub end_at: String,
    pub est_minutes: i64,
}

#[derive(Deserialize)]
struct QuestionList {
    questions: Vec<Question>,
}

#[derive(Deserialize)]
struct PlanDraft {
    summary: String,
    actions: Vec<PlannedAction>,
}

/// Step 1: analyze course text into modules with estimated minutes.
/// `total_minutes` is recomputed from the modules.
pub async fn analyze_course(
    claude: &Claude,
    goal: &Goal,
    course: &CourseText,
) -> Result<CourseAnalysis, AgentError> {
    let source = course
        .source_url
        .as_deref()
        .unwrap_or("text pasted by the owner");
    let values = HashMap::from([
        ("goal_title", goal.title.as_str()),
        ("source", source),
        ("course_text", course.text.as_str()),
    ]);
    let mut analysis: CourseAnalysis = run(claude, "course-analyzer", &values).await?;
    if analysis.modules.is_empty() {
        return Err(AgentError::Validation(
            "course analysis has no modules".into(),
        ));
    }
    if let Some((i, m)) = analysis
        .modules
        .iter()
        .enumerate()
        .find(|(_, m)| m.title.trim().is_empty() || m.minutes <= 0)
    {
        return Err(AgentError::Validation(format!(
            "module #{} `{}` needs a title and positive minutes",
            i + 1,
            m.title
        )));
    }
    analysis.total_minutes = analysis.modules.iter().map(|m| m.minutes).sum();
    Ok(analysis)
}

/// Step 2: write 5 to 10 intake questions for the goal. Ids are unique.
pub async fn write_questions(
    claude: &Claude,
    goal: &Goal,
    analysis: &CourseAnalysis,
) -> Result<Vec<Question>, AgentError> {
    let analysis_json = to_json(analysis)?;
    let values = HashMap::from([
        ("goal_title", goal.title.as_str()),
        ("target_date", goal.target_date.as_str()),
        ("course_analysis", analysis_json.as_str()),
    ]);
    let list: QuestionList = run(claude, "question-writer", &values).await?;
    if !(MIN_QUESTIONS..=MAX_QUESTIONS).contains(&list.questions.len()) {
        return Err(AgentError::Validation(format!(
            "got {} questions, want {MIN_QUESTIONS} to {MAX_QUESTIONS}",
            list.questions.len()
        )));
    }
    for (i, q) in list.questions.iter().enumerate() {
        if q.id.trim().is_empty()
            || q.text.trim().is_empty()
            || list.questions[..i].iter().any(|p| p.id == q.id)
        {
            return Err(AgentError::Validation(format!(
                "question #{} `{}` needs a unique id and text",
                i + 1,
                q.id
            )));
        }
    }
    Ok(list.questions)
}

/// Step 3: plan the goal's actions from the analysis and `goal.answers_json`,
/// around the `busy` calendar, then validate. Returns a proposed plan at
/// version 1 (the caller bumps it for a regenerated plan) and its actions
/// in order, with fresh uuid v7 ids.
pub async fn generate_plan(
    claude: &Claude,
    goal: &Goal,
    analysis: &CourseAnalysis,
    busy: &[Action],
    now: DateTime<Utc>,
) -> Result<(Plan, Vec<Action>), AgentError> {
    let analysis_json = to_json(analysis)?;
    let busy_json = to_json(
        &busy
            .iter()
            .map(|a| serde_json::json!({"title": a.title, "start_at": a.start_at, "end_at": a.end_at}))
            .collect::<Vec<_>>(),
    )?;
    let now_text = now.to_rfc3339_opts(SecondsFormat::Secs, true);
    let values = HashMap::from([
        ("goal_title", goal.title.as_str()),
        ("target_date", goal.target_date.as_str()),
        ("now", now_text.as_str()),
        ("course_analysis", analysis_json.as_str()),
        ("answers", goal.answers_json.as_deref().unwrap_or("{}")),
        ("busy", busy_json.as_str()),
    ]);
    let draft: PlanDraft = run(claude, "planner", &values).await?;
    let actions: Vec<Action> = draft
        .actions
        .into_iter()
        .enumerate()
        .map(|(seq, p)| Action {
            id: Uuid::now_v7().to_string(),
            goal_id: goal.id.clone(),
            title: p.title.trim().to_owned(),
            notes: p.notes.filter(|n| !n.trim().is_empty()),
            start_at: p.start_at,
            end_at: p.end_at,
            est_minutes: p.est_minutes,
            status: ActionStatus::Planned,
            seq: seq as i64,
            done_at: None,
        })
        .collect();
    validate_plan(goal, &actions, busy, now)?;
    let plan = Plan {
        id: Uuid::now_v7().to_string(),
        goal_id: goal.id.clone(),
        version: 1,
        status: PlanStatus::Proposed,
        summary: draft.summary.trim().to_owned(),
    };
    Ok((plan, actions))
}

/// `value` as JSON text.
fn to_json<T: Serialize>(value: &T) -> Result<String, AgentError> {
    serde_json::to_string(value).map_err(|e| AgentError::InvalidJson(e.to_string()))
}

/// Render built-in agent `name` with `values`, call Claude with the agent's
/// output schema and effort, and deserialize the reply.
async fn run<T: DeserializeOwned>(
    claude: &Claude,
    name: &str,
    values: &HashMap<&str, &str>,
) -> Result<T, AgentError> {
    let decl =
        builtin(name).ok_or_else(|| AgentError::Template(format!("no built-in agent `{name}`")))?;
    let rendered = render(&decl, values)?;
    let schema = rendered.output.as_deref().map(output_schema).transpose()?;
    let text = claude
        .complete(
            &rendered.system,
            &rendered.prompt,
            schema.as_ref(),
            rendered.effort,
        )
        .await?;
    serde_json::from_str(&text).map_err(|e| AgentError::InvalidJson(format!("agent `{name}`: {e}")))
}

/// The JSON schema behind an agent's `output` name.
fn output_schema(name: &str) -> Result<Value, AgentError> {
    let string = json!({"type": "string"});
    let object = |properties: Value, required: &[&str]| json!({"type": "object", "properties": properties, "required": required, "additionalProperties": false});
    let schema = match name {
        "course_analysis" => object(
            json!({
                "title": string,
                "modules": {"type": "array", "items": object(json!({"title": string, "minutes": {"type": "integer"}}), &["title", "minutes"])},
                "total_minutes": {"type": "integer"},
            }),
            &["title", "modules", "total_minutes"],
        ),
        "questions" => object(
            json!({
                "questions": {"type": "array", "items": object(
                    json!({"id": string, "text": string, "options": {"type": "array", "items": string}}),
                    &["id", "text", "options"],
                )},
            }),
            &["questions"],
        ),
        "plan" => object(
            json!({
                "summary": string,
                "actions": {"type": "array", "items": object(
                    json!({
                        "title": string,
                        "notes": string,
                        "start_at": {"type": "string", "format": "date-time"},
                        "end_at": {"type": "string", "format": "date-time"},
                        "est_minutes": {"type": "integer"},
                    }),
                    &["title", "notes", "start_at", "end_at", "est_minutes"],
                )},
            }),
            &["summary", "actions"],
        ),
        _ => return Err(AgentError::Template(format!("no output schema `{name}`"))),
    };
    Ok(schema)
}

/// Check planned `actions` of `goal` against the clock and the calendar.
/// Rejects an empty plan, empty titles, unparseable times, `end_at <= start_at`,
/// `start_at` before `now`, `end_at` after the goal target date,
/// `est_minutes` not positive or not equal to the span, and any overlap
/// with another planned action or a `busy` one. Errors name the action.
pub fn validate_plan(
    goal: &Goal,
    actions: &[Action],
    busy: &[Action],
    now: DateTime<Utc>,
) -> Result<(), AgentError> {
    let deadline = deadline(&goal.target_date)?;
    if actions.is_empty() {
        return Err(AgentError::Validation("plan has no actions".into()));
    }
    let mut planned = Vec::with_capacity(actions.len());
    for (i, action) in actions.iter().enumerate() {
        let label = format!("action #{} `{}`", i + 1, action.title.trim());
        let bad = |m: String| Err(AgentError::Validation(format!("{label}: {m}")));
        if action.title.trim().is_empty() {
            return bad("empty title".into());
        }
        let (start, end) = match span(action) {
            Ok(span) => span,
            Err(m) => return bad(m),
        };
        if end <= start {
            return bad("end_at is not after start_at".into());
        }
        if start < now {
            return bad("start_at is in the past".into());
        }
        if end > deadline {
            return bad(format!(
                "end_at is after the goal target date {}",
                goal.target_date
            ));
        }
        let span_minutes = (end - start).num_minutes();
        if action.est_minutes <= 0
            || action.est_minutes != span_minutes
            || end - start != TimeDelta::minutes(span_minutes)
        {
            return bad(format!(
                "est_minutes {} does not match the span",
                action.est_minutes
            ));
        }
        planned.push((start, end, label));
    }
    let mut taken = Vec::with_capacity(busy.len());
    for action in busy {
        let label = format!("busy `{}`", action.title.trim());
        let (start, end) =
            span(action).map_err(|m| AgentError::Validation(format!("{label}: {m}")))?;
        taken.push((start, end, label));
    }
    for (i, (start, end, label)) in planned.iter().enumerate() {
        let others = planned[i + 1..].iter().chain(&taken);
        if let Some((_, _, other)) = others.into_iter().find(|(s, e, _)| s < end && start < e) {
            return Err(AgentError::Validation(format!("{label} overlaps {other}")));
        }
    }
    Ok(())
}

fn span(action: &Action) -> Result<(DateTime<Utc>, DateTime<Utc>), String> {
    let parse = |field: &str, text: &str| {
        DateTime::parse_from_rfc3339(text)
            .map(|t| t.with_timezone(&Utc))
            .map_err(|_| format!("{field} `{text}` is not an RFC 3339 time"))
    };
    Ok((
        parse("start_at", &action.start_at)?,
        parse("end_at", &action.end_at)?,
    ))
}

/// Latest instant an action may end: a date-time target as given, a date
/// target at the end of that UTC day.
fn deadline(target_date: &str) -> Result<DateTime<Utc>, AgentError> {
    if let Ok(t) = DateTime::parse_from_rfc3339(target_date) {
        return Ok(t.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(target_date, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.succ_opt())
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|t| t.and_utc())
        .ok_or_else(|| {
            AgentError::Validation(format!("goal target_date `{target_date}` is not a date"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ActionStatus, GoalKind, GoalStatus};

    fn goal() -> Goal {
        Goal {
            id: "g1".into(),
            title: "Finish Rust course".into(),
            kind: GoalKind::Course,
            source_url: None,
            target_date: "2026-12-10".into(),
            status: GoalStatus::Draft,
            answers_json: None,
            created_at: "2026-11-20T09:00:00Z".into(),
        }
    }

    fn action(title: &str, start: &str, end: &str, minutes: i64) -> Action {
        Action {
            id: format!("a-{title}"),
            goal_id: "g1".into(),
            title: title.into(),
            notes: None,
            start_at: start.into(),
            end_at: end.into(),
            est_minutes: minutes,
            status: ActionStatus::Planned,
            seq: 0,
            done_at: None,
        }
    }

    fn now() -> DateTime<Utc> {
        "2026-11-20T12:00:00Z".parse().unwrap()
    }

    fn valid() -> Vec<Action> {
        vec![
            action(
                "Module 1",
                "2026-11-21T18:00:00Z",
                "2026-11-21T19:00:00Z",
                60,
            ),
            action(
                "Module 2",
                "2026-11-21T19:00:00Z",
                "2026-11-21T19:30:00Z",
                30,
            ),
            action(
                "Module 3",
                "2026-12-10T22:00:00Z",
                "2026-12-10T23:00:00Z",
                60,
            ),
        ]
    }

    fn rejects(actions: Vec<Action>, busy: &[Action], needle: &str) {
        let err = validate_plan(&goal(), &actions, busy, now()).unwrap_err();
        assert!(
            matches!(err, AgentError::Validation(ref m) if m.contains(needle)),
            "want `{needle}` in {err}"
        );
    }

    #[test]
    fn valid_plan_passes() {
        validate_plan(&goal(), &valid(), &[], now()).unwrap();
    }

    #[test]
    fn overlap_rejected() {
        let mut plan = valid();
        plan[1] = action(
            "Module 2",
            "2026-11-21T18:30:00Z",
            "2026-11-21T19:30:00Z",
            60,
        );
        rejects(plan, &[], "Module 2");
    }

    #[test]
    fn overlap_with_busy_calendar_rejected() {
        let busy = [action(
            "Gym",
            "2026-11-21T18:45:00Z",
            "2026-11-21T19:15:00Z",
            30,
        )];
        rejects(valid(), &busy, "Gym");
    }

    #[test]
    fn past_start_rejected() {
        let mut plan = valid();
        plan[0] = action(
            "Module 1",
            "2026-11-20T11:00:00Z",
            "2026-11-20T12:30:00Z",
            90,
        );
        rejects(plan, &[], "Module 1");
    }

    #[test]
    fn past_deadline_rejected() {
        let mut plan = valid();
        plan[2] = action(
            "Module 3",
            "2026-12-10T23:30:00Z",
            "2026-12-11T00:30:00Z",
            60,
        );
        rejects(plan, &[], "Module 3");
    }

    #[test]
    fn bad_span_minutes_and_title_rejected() {
        let mut plan = valid();
        plan[0] = action(
            "Module 1",
            "2026-11-21T19:00:00Z",
            "2026-11-21T18:00:00Z",
            60,
        );
        rejects(plan, &[], "Module 1");
        let mut plan = valid();
        plan[0].est_minutes = 45;
        rejects(plan, &[], "Module 1");
        let mut plan = valid();
        plan[0].est_minutes = 0;
        rejects(plan, &[], "Module 1");
        let mut plan = valid();
        plan[1].title = "  ".into();
        rejects(plan, &[], "#2");
        let mut plan = valid();
        plan[0].start_at = "tomorrow".into();
        rejects(plan, &[], "Module 1");
    }
}
