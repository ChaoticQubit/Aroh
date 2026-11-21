//! Goals pod commands: draft a course goal, analyze it, ask intake questions,
//! propose a plan and approve it into calendar actions.

use super::with_core;
use crate::AppState;
use aroh_agents::model as agent;
use aroh_agents::{CourseAnalysis, Claude, Question};
use aroh_core::models::{Action, Goal, GoalKind, GoalStatus, Plan, PlanStatus};
use aroh_core::repo::{self, GoalDraft};
use aroh_core::{keychain, Error};
use chrono::{Local, NaiveDate, SecondsFormat, TimeZone, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::State;

const MISSING_KEY: &str = "missing-key: No Claude API key saved. Add one in Settings.";
const MAX_TITLE_CHARS: usize = 200;
const MAX_URL_CHARS: usize = 2048;
const MAX_ANSWERS: usize = 20;
const MAX_ANSWER_CHARS: usize = 500;
const MAX_QUESTION_CHARS: usize = 300;

/// What the analyze step returns: the course analysis and the intake questions.
#[derive(Serialize)]
pub struct Analysis {
    analysis: CourseAnalysis,
    questions: Vec<Question>,
}

/// A plan with its actions: proposed ones before approval, calendar ones after.
#[derive(Serialize)]
pub struct PlanView {
    plan: Plan,
    actions: Vec<Action>,
}

/// Everything the goal detail screen shows.
#[derive(Serialize)]
pub struct GoalDetail {
    goal: Goal,
    plan: Option<Plan>,
    actions: Vec<Action>,
    proposed: Vec<Action>,
    analysis: Option<CourseAnalysis>,
    questions: Vec<Question>,
}

fn claude() -> Result<Claude, String> {
    let key = keychain::claude_api_key().map_err(|e| e.to_string())?.ok_or(MISSING_KEY)?;
    Claude::new(&key).map_err(|e| e.to_string())
}

fn parse_date(target_date: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(target_date, "%Y-%m-%d").map_err(|_| "Target date must be YYYY-MM-DD.".to_string())
}

/// The first instant after `target_date` ends in the device's local time, as UTC RFC 3339.
fn local_deadline(target_date: &str) -> Result<String, String> {
    let next = parse_date(target_date)?.succ_opt().ok_or("Target date is out of range.")?;
    let midnight = next.and_hms_opt(0, 0, 0).ok_or("Target date is out of range.")?;
    let local = Local.from_local_datetime(&midnight).earliest().ok_or("Target date has no local midnight.")?;
    Ok(local.with_timezone(&Utc).to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// The goal as aroh-agents sees it, with `target_date` turned into the local deadline instant.
fn agent_goal(goal: &Goal) -> Result<agent::Goal, String> {
    Ok(agent::Goal {
        id: goal.id.clone(),
        title: goal.title.clone(),
        kind: agent::GoalKind::Course,
        source_url: goal.source_url.clone(),
        target_date: local_deadline(&goal.target_date)?,
        status: agent::GoalStatus::Draft,
        answers_json: goal.answers_json.clone(),
        created_at: goal.created_at.clone(),
    })
}

fn agent_action(a: &Action) -> agent::Action {
    agent::Action {
        id: a.id.clone(),
        goal_id: a.goal_id.clone(),
        title: a.title.clone(),
        notes: a.notes.clone(),
        start_at: a.start_at.clone(),
        end_at: a.end_at.clone(),
        est_minutes: a.est_minutes,
        status: agent::ActionStatus::Planned,
        seq: a.seq,
        done_at: None,
    }
}

fn core_action(a: agent::Action) -> Action {
    Action {
        id: a.id,
        goal_id: a.goal_id,
        title: a.title,
        notes: a.notes,
        start_at: a.start_at,
        end_at: a.end_at,
        est_minutes: a.est_minutes,
        status: aroh_core::models::ActionStatus::Planned,
        seq: a.seq,
        done_at: None,
        updated_at: String::new(),
        deleted: false,
    }
}

fn decode<T: serde::de::DeserializeOwned>(json: Option<&str>) -> Option<T> {
    json.and_then(|j| serde_json::from_str(j).ok())
}

/// Creates a draft course goal. Exactly one of `source_url` (http or https) or
/// pasted text is the course source; pasted text goes to `goals_analyze` instead
/// of being stored. `target_date` is `YYYY-MM-DD`, today or later.
#[tauri::command]
pub fn goals_create(
    state: State<'_, AppState>,
    title: String,
    source_url: Option<String>,
    target_date: String,
) -> Result<Goal, String> {
    let title = title.trim().to_string();
    if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
        return Err(format!("Title must be 1 to {MAX_TITLE_CHARS} characters."));
    }
    let source_url = source_url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
    if let Some(url) = &source_url {
        let lower = url.to_ascii_lowercase();
        if url.len() > MAX_URL_CHARS || !(lower.starts_with("https://") || lower.starts_with("http://")) {
            return Err("Link must start with http:// or https://.".into());
        }
    }
    if parse_date(&target_date)? < Local::now().date_naive() {
        return Err("Target date is in the past.".into());
    }
    let mut goal = Goal {
        id: aroh_core::new_id(),
        title,
        kind: GoalKind::Course,
        source_url,
        target_date,
        status: GoalStatus::Draft,
        answers_json: None,
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        updated_at: String::new(),
        deleted: false,
    };
    with_core(&state, |c| repo::insert_goal(&c.conn, &mut c.clock, &mut goal))?;
    Ok(goal)
}

/// Reads the course (the goal's link, or `pasted_text` when given), analyzes it,
/// writes 5 to 10 intake questions and keeps both in the goal's local draft.
#[tauri::command]
pub async fn goals_analyze(
    state: State<'_, AppState>,
    goal_id: String,
    pasted_text: Option<String>,
) -> Result<Analysis, String> {
    let claude = claude()?;
    let goal = with_core(&state, |c| repo::get_goal(&c.conn, &goal_id))?;
    let course = match (pasted_text.as_deref().map(str::trim).filter(|t| !t.is_empty()), goal.source_url.as_deref()) {
        (Some(text), _) => aroh_agents::pasted_text(text),
        (None, Some(url)) => aroh_agents::fetch_course(url).await,
        (None, None) => return Err("Add a course link or paste the course text.".into()),
    }
    .map_err(|e| e.to_string())?;
    let agent_goal = agent_goal(&goal)?;
    let analysis = aroh_agents::analyze_course(&claude, &agent_goal, &course).await.map_err(|e| e.to_string())?;
    let questions = aroh_agents::write_questions(&claude, &agent_goal, &analysis).await.map_err(|e| e.to_string())?;
    let mut draft = GoalDraft {
        goal_id: goal.id.clone(),
        analysis_json: Some(serde_json::to_string(&analysis).map_err(|e| e.to_string())?),
        questions_json: Some(serde_json::to_string(&questions).map_err(|e| e.to_string())?),
        proposed_actions: Vec::new(),
        updated_at: String::new(),
    };
    with_core(&state, |c| repo::put_draft(&c.conn, &mut c.clock, &mut draft))?;
    Ok(Analysis { analysis, questions })
}

/// Saves `answers` (question text to answer text) on the goal, generates and
/// validates a plan around every live calendar action, and stores it as the
/// goal's new proposed plan. A previous proposed plan becomes rejected.
#[tauri::command]
pub async fn goals_plan(
    state: State<'_, AppState>,
    goal_id: String,
    answers: BTreeMap<String, String>,
) -> Result<PlanView, String> {
    if answers.len() > MAX_ANSWERS || answers.iter().any(|(k, v)| k.chars().count() > MAX_QUESTION_CHARS || v.chars().count() > MAX_ANSWER_CHARS) {
        return Err(format!("At most {MAX_ANSWERS} answers of up to {MAX_ANSWER_CHARS} characters."));
    }
    let claude = claude()?;
    let answers_json = serde_json::to_string(&answers).map_err(|e| e.to_string())?;
    let (goal, draft, busy) = with_core(&state, |c| {
        let mut goal = repo::get_goal(&c.conn, &goal_id)?;
        if goal.status != GoalStatus::Draft {
            return Err(Error::InvalidState("goal already has an approved plan".into()));
        }
        let draft = repo::get_draft(&c.conn, &goal.id)?
            .filter(|d| d.analysis_json.is_some())
            .ok_or_else(|| Error::InvalidState("analyze the course first".into()))?;
        goal.answers_json = Some(answers_json);
        repo::update_goal(&c.conn, &mut c.clock, &mut goal)?;
        let busy: Vec<Action> = repo::list_actions(&c.conn, None)?
            .into_iter()
            .filter(|a| a.status == aroh_core::models::ActionStatus::Planned)
            .collect();
        Ok((goal, draft, busy))
    })?;
    let analysis: CourseAnalysis = decode(draft.analysis_json.as_deref()).ok_or("Stored course analysis is unreadable.")?;
    let busy: Vec<agent::Action> = busy.iter().map(agent_action).collect();
    let (plan, actions) = aroh_agents::generate_plan(&claude, &agent_goal(&goal)?, &analysis, &busy, Utc::now(), *Local::now().offset())
        .await
        .map_err(|e| e.to_string())?;
    let actions: Vec<Action> = actions.into_iter().map(core_action).collect();
    let view = with_core(&state, |c| {
        let previous = repo::latest_plan(&c.conn, &goal.id)?;
        let version = previous.as_ref().map_or(1, |p| p.version + 1);
        if let Some(mut old) = previous.filter(|p| p.status == PlanStatus::Proposed) {
            old.status = PlanStatus::Rejected;
            repo::update_plan(&c.conn, &mut c.clock, &mut old)?;
        }
        let mut stored = Plan {
            id: plan.id,
            goal_id: goal.id.clone(),
            version,
            status: PlanStatus::Proposed,
            summary: Some(plan.summary).filter(|s| !s.is_empty()),
            updated_at: String::new(),
            deleted: false,
        };
        repo::insert_plan(&c.conn, &mut c.clock, &mut stored)?;
        let mut draft = draft;
        draft.proposed_actions = actions.clone();
        repo::put_draft(&c.conn, &mut c.clock, &mut draft)?;
        Ok(PlanView { plan: stored, actions })
    })?;
    Ok(view)
}

/// Approves proposed plan `plan_id`: its actions join the calendar and the goal
/// becomes active, in one transaction. Returns the approved plan and its actions.
#[tauri::command]
pub fn goals_approve(app: tauri::AppHandle, state: State<'_, AppState>, plan_id: String) -> Result<PlanView, String> {
    let view = with_core(&state, |c| {
        let actions = repo::approve_plan(&c.conn, &mut c.clock, &plan_id)?;
        Ok(PlanView { plan: repo::get_plan(&c.conn, &plan_id)?, actions })
    })?;
    crate::commands::calendar::sync_reminders(&app)?;
    Ok(view)
}

/// Returns a goal with its latest plan, calendar actions, proposed actions,
/// analysis and questions.
#[tauri::command]
pub fn goals_detail(state: State<'_, AppState>, goal_id: String) -> Result<GoalDetail, String> {
    with_core(&state, |c| {
        let goal = repo::get_goal(&c.conn, &goal_id)?;
        let plan = repo::latest_plan(&c.conn, &goal.id)?;
        let actions = repo::list_actions(&c.conn, Some(&goal.id))?;
        let draft = repo::get_draft(&c.conn, &goal.id)?;
        Ok(GoalDetail {
            analysis: draft.as_ref().and_then(|d| decode(d.analysis_json.as_deref())),
            questions: draft.as_ref().and_then(|d| decode(d.questions_json.as_deref())).unwrap_or_default(),
            proposed: draft.map(|d| d.proposed_actions).unwrap_or_default(),
            goal,
            plan,
            actions,
        })
    })
}
