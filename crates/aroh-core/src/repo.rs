//! Repository functions for goals, actions, plans and local goal drafts. Every write stamps `updated_at` with the HLC.

use crate::hlc::Hlc;
use crate::models::{Action, Goal, GoalStatus, Plan, PlanStatus};
use crate::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};

const GOAL_COLUMNS: &str =
    "id, title, kind, source_url, target_date, status, answers_json, created_at, updated_at, deleted";
const ACTION_COLUMNS: &str =
    "id, goal_id, title, notes, start_at, end_at, est_minutes, status, seq, done_at, updated_at, deleted";

fn goal_from_row(row: &Row<'_>) -> rusqlite::Result<Goal> {
    Ok(Goal {
        id: row.get(0)?,
        title: row.get(1)?,
        kind: row.get(2)?,
        source_url: row.get(3)?,
        target_date: row.get(4)?,
        status: row.get(5)?,
        answers_json: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        deleted: row.get(9)?,
    })
}

fn action_from_row(row: &Row<'_>) -> rusqlite::Result<Action> {
    Ok(Action {
        id: row.get(0)?,
        goal_id: row.get(1)?,
        title: row.get(2)?,
        notes: row.get(3)?,
        start_at: row.get(4)?,
        end_at: row.get(5)?,
        est_minutes: row.get(6)?,
        status: row.get(7)?,
        seq: row.get(8)?,
        done_at: row.get(9)?,
        updated_at: row.get(10)?,
        deleted: row.get(11)?,
    })
}

fn expect_one(changed: usize, id: &str) -> Result<()> {
    assert!(changed <= 1, "primary key matched more than one row");
    if changed == 0 {
        return Err(Error::NotFound(id.chars().take(64).collect()));
    }
    Ok(())
}

fn soft_delete(conn: &Connection, clock: &mut Hlc, table: &str, id: &str) -> Result<()> {
    let sql = format!("UPDATE {table} SET deleted = 1, updated_at = ?2 WHERE id = ?1 AND deleted = 0");
    expect_one(conn.execute(&sql, params![id, clock.tick()])?, id)
}

/// Inserts `goal`, stamping its `updated_at` and clearing `deleted`.
pub fn insert_goal(conn: &Connection, clock: &mut Hlc, goal: &mut Goal) -> Result<()> {
    goal.updated_at = clock.tick();
    goal.deleted = false;
    conn.execute(
        &format!("INSERT INTO goals ({GOAL_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)"),
        params![goal.id, goal.title, goal.kind, goal.source_url, goal.target_date, goal.status,
            goal.answers_json, goal.created_at, goal.updated_at],
    )?;
    Ok(())
}

/// Updates every field of a live goal and stamps `updated_at`. Errors with `NotFound` if missing or deleted.
pub fn update_goal(conn: &Connection, clock: &mut Hlc, goal: &mut Goal) -> Result<()> {
    goal.updated_at = clock.tick();
    let changed = conn.execute(
        "UPDATE goals SET title = ?2, kind = ?3, source_url = ?4, target_date = ?5, status = ?6,
            answers_json = ?7, updated_at = ?8 WHERE id = ?1 AND deleted = 0",
        params![goal.id, goal.title, goal.kind, goal.source_url, goal.target_date, goal.status,
            goal.answers_json, goal.updated_at],
    )?;
    expect_one(changed, &goal.id)
}

/// Marks a goal deleted (tombstone) and stamps `updated_at`.
pub fn soft_delete_goal(conn: &Connection, clock: &mut Hlc, id: &str) -> Result<()> {
    soft_delete(conn, clock, "goals", id)
}

/// Lists live goals ordered by creation time.
pub fn list_goals(conn: &Connection) -> Result<Vec<Goal>> {
    let mut stmt = conn.prepare(&format!("SELECT {GOAL_COLUMNS} FROM goals WHERE deleted = 0 ORDER BY created_at, id"))?;
    let rows = stmt.query_map([], goal_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Inserts `action`, stamping its `updated_at` and clearing `deleted`.
pub fn insert_action(conn: &Connection, clock: &mut Hlc, action: &mut Action) -> Result<()> {
    action.updated_at = clock.tick();
    action.deleted = false;
    conn.execute(
        &format!("INSERT INTO actions ({ACTION_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0)"),
        params![action.id, action.goal_id, action.title, action.notes, action.start_at, action.end_at,
            action.est_minutes, action.status, action.seq, action.done_at, action.updated_at],
    )?;
    Ok(())
}

/// Updates every field of a live action and stamps `updated_at`. Errors with `NotFound` if missing or deleted.
pub fn update_action(conn: &Connection, clock: &mut Hlc, action: &mut Action) -> Result<()> {
    action.updated_at = clock.tick();
    let changed = conn.execute(
        "UPDATE actions SET goal_id = ?2, title = ?3, notes = ?4, start_at = ?5, end_at = ?6, est_minutes = ?7,
            status = ?8, seq = ?9, done_at = ?10, updated_at = ?11 WHERE id = ?1 AND deleted = 0",
        params![action.id, action.goal_id, action.title, action.notes, action.start_at, action.end_at,
            action.est_minutes, action.status, action.seq, action.done_at, action.updated_at],
    )?;
    expect_one(changed, &action.id)
}

/// Marks an action deleted (tombstone) and stamps `updated_at`.
pub fn soft_delete_action(conn: &Connection, clock: &mut Hlc, id: &str) -> Result<()> {
    soft_delete(conn, clock, "actions", id)
}

/// Lists live actions by start time, optionally only those of `goal_id`.
pub fn list_actions(conn: &Connection, goal_id: Option<&str>) -> Result<Vec<Action>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ACTION_COLUMNS} FROM actions WHERE deleted = 0 AND (?1 IS NULL OR goal_id = ?1) ORDER BY start_at, seq, id"
    ))?;
    let rows = stmt.query_map(params![goal_id], action_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Returns the live goal `id`. Errors with `NotFound` if missing or deleted.
pub fn get_goal(conn: &Connection, id: &str) -> Result<Goal> {
    conn.query_row(&format!("SELECT {GOAL_COLUMNS} FROM goals WHERE id = ?1 AND deleted = 0"), params![id], goal_from_row)
        .optional()?
        .ok_or_else(|| Error::NotFound(id.chars().take(64).collect()))
}

const PLAN_COLUMNS: &str = "id, goal_id, version, status, summary, updated_at, deleted";

fn plan_from_row(row: &Row<'_>) -> rusqlite::Result<Plan> {
    Ok(Plan {
        id: row.get(0)?,
        goal_id: row.get(1)?,
        version: row.get(2)?,
        status: row.get(3)?,
        summary: row.get(4)?,
        updated_at: row.get(5)?,
        deleted: row.get(6)?,
    })
}

/// Inserts `plan`, stamping its `updated_at` and clearing `deleted`.
pub fn insert_plan(conn: &Connection, clock: &mut Hlc, plan: &mut Plan) -> Result<()> {
    assert!(plan.version >= 1, "plan versions start at 1");
    plan.updated_at = clock.tick();
    plan.deleted = false;
    conn.execute(
        &format!("INSERT INTO plans ({PLAN_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)"),
        params![plan.id, plan.goal_id, plan.version, plan.status, plan.summary, plan.updated_at],
    )?;
    Ok(())
}

/// Updates every field of a live plan and stamps `updated_at`. Errors with `NotFound` if missing or deleted.
pub fn update_plan(conn: &Connection, clock: &mut Hlc, plan: &mut Plan) -> Result<()> {
    plan.updated_at = clock.tick();
    let changed = conn.execute(
        "UPDATE plans SET goal_id = ?2, version = ?3, status = ?4, summary = ?5, updated_at = ?6
            WHERE id = ?1 AND deleted = 0",
        params![plan.id, plan.goal_id, plan.version, plan.status, plan.summary, plan.updated_at],
    )?;
    expect_one(changed, &plan.id)
}

/// Returns the live plan `id`. Errors with `NotFound` if missing or deleted.
pub fn get_plan(conn: &Connection, id: &str) -> Result<Plan> {
    conn.query_row(&format!("SELECT {PLAN_COLUMNS} FROM plans WHERE id = ?1 AND deleted = 0"), params![id], plan_from_row)
        .optional()?
        .ok_or_else(|| Error::NotFound(id.chars().take(64).collect()))
}

/// Returns the live plan of `goal_id` with the highest version, if any.
pub fn latest_plan(conn: &Connection, goal_id: &str) -> Result<Option<Plan>> {
    Ok(conn
        .query_row(
            &format!("SELECT {PLAN_COLUMNS} FROM plans WHERE goal_id = ?1 AND deleted = 0 ORDER BY version DESC, id DESC LIMIT 1"),
            params![goal_id],
            plan_from_row,
        )
        .optional()?)
}

/// Device-local planning state of one goal, never synced: the course analysis,
/// the intake questions (both JSON text) and the actions of the proposed plan.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoalDraft {
    pub goal_id: String,
    pub analysis_json: Option<String>,
    pub questions_json: Option<String>,
    pub proposed_actions: Vec<Action>,
    pub updated_at: String,
}

fn json_error(err: serde_json::Error) -> Error {
    Error::InvalidState(format!("goal draft json: {err}"))
}

/// Inserts or replaces the draft of `draft.goal_id`, stamping `updated_at`.
pub fn put_draft(conn: &Connection, clock: &mut Hlc, draft: &mut GoalDraft) -> Result<()> {
    draft.updated_at = clock.tick();
    let actions = serde_json::to_string(&draft.proposed_actions).map_err(json_error)?;
    conn.execute(
        "INSERT INTO goal_drafts (goal_id, analysis_json, questions_json, proposed_actions_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(goal_id) DO UPDATE SET analysis_json = ?2, questions_json = ?3,
                proposed_actions_json = ?4, updated_at = ?5",
        params![draft.goal_id, draft.analysis_json, draft.questions_json, actions, draft.updated_at],
    )?;
    Ok(())
}

/// Returns the draft of `goal_id`, if one was stored.
pub fn get_draft(conn: &Connection, goal_id: &str) -> Result<Option<GoalDraft>> {
    let row = conn
        .query_row(
            "SELECT goal_id, analysis_json, questions_json, proposed_actions_json, updated_at
                FROM goal_drafts WHERE goal_id = ?1",
            params![goal_id],
            |r| Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get::<_, String>(3)?, r.get(4)?)),
        )
        .optional()?;
    row.map(|(goal_id, analysis_json, questions_json, actions, updated_at)| {
        let proposed_actions = serde_json::from_str(&actions).map_err(json_error)?;
        Ok(GoalDraft { goal_id, analysis_json, questions_json, proposed_actions, updated_at })
    })
    .transpose()
}

/// Approves proposed plan `plan_id` in one transaction: inserts the draft's proposed
/// actions, marks the plan approved and its goal active, and clears the proposed actions.
/// Returns the inserted actions. Errors with `InvalidState` unless the plan is proposed
/// and has proposed actions; on any error nothing is written.
pub fn approve_plan(conn: &Connection, clock: &mut Hlc, plan_id: &str) -> Result<Vec<Action>> {
    let tx = conn.unchecked_transaction()?;
    let mut plan = get_plan(&tx, plan_id)?;
    if plan.status != PlanStatus::Proposed {
        return Err(Error::InvalidState(format!("plan is {}, not proposed", plan.status.as_str())));
    }
    let mut goal = get_goal(&tx, &plan.goal_id)?;
    let mut draft = get_draft(&tx, &goal.id)?
        .filter(|d| !d.proposed_actions.is_empty())
        .ok_or_else(|| Error::InvalidState("plan has no proposed actions".into()))?;
    let mut actions = std::mem::take(&mut draft.proposed_actions);
    for action in &mut actions {
        assert_eq!(action.goal_id, goal.id, "proposed action belongs to another goal");
        insert_action(&tx, clock, action)?;
    }
    plan.status = PlanStatus::Approved;
    update_plan(&tx, clock, &mut plan)?;
    goal.status = GoalStatus::Active;
    update_goal(&tx, clock, &mut goal)?;
    put_draft(&tx, clock, &mut draft)?;
    tx.commit()?;
    Ok(actions)
}
