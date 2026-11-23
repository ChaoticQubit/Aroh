use crate::evals::{parse_local, slots};
use crate::rows::{get_row, goal_deleted, materialize, put_row, utc, ServerClock};
use aroh_domain::recur::DEFAULT_WINDOW_DAYS;
use aroh_domain::rows::{
    self, Action, ActionSeries, ActionStatus, FieldKind, GoalStatus, Proposal, ProposalSize, ProposalStatus, Tracker,
    TrackerEntry, TrackerField,
};
use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use crate::db::PgPool;
use std::sync::Arc;
use troupekit_core::clock::Clock;
use troupekit_tools::{typed_tool, ToolContext, ToolError, ToolOutput, Toolkit};
use uuid::Uuid;

/// What every Aroh tool of one run writes with: the pool, the server HLC,
/// the clock and the run's user and goal. Tools take the user and goal
/// from here, never from model output.
#[derive(Clone)]
pub struct ToolEnv {
    pub pool: PgPool,
    pub hlc: Arc<ServerClock>,
    pub clock: Arc<dyn Clock>,
    pub user: Uuid,
    pub goal: Option<Uuid>,
}

impl ToolEnv {
    fn goal(&self) -> Result<Uuid, ToolError> {
        self.goal.ok_or_else(|| ToolError::Failed("this run has no goal".into()))
    }

    /// The run's goal while it is live; fails once the owner deleted it (Q68), so a run
    /// caught mid-step writes no plan, proposal or plugin for a deleted goal.
    async fn live_goal(&self) -> Result<Uuid, ToolError> {
        let goal = self.goal()?;
        if goal_deleted(&self.pool, self.user, goal).await.map_err(failed)? {
            return Err(ToolError::Failed("this goal was deleted".into()));
        }
        Ok(goal)
    }

    fn now(&self) -> DateTime<Utc> {
        utc(self.clock.now_ms())
    }
}

fn failed(e: impl std::fmt::Display) -> ToolError {
    ToolError::Failed(e.to_string())
}

/// Returns a row id that is the same every time the same tool call replays
/// and differs across calls: a v7-shaped id hashed from the call's
/// idempotency key and `part`, timed by the run id.
pub fn row_id(ctx: &ToolContext, part: &str) -> Result<String, ToolError> {
    let key = ctx.idempotency_key.as_deref().ok_or_else(|| ToolError::InvalidArgs("tool call has no idempotency key".into()))?;
    let millis = ctx
        .run_id
        .and_then(|r| r.0.get_timestamp())
        .map(|t| {
            let (secs, nanos) = t.to_unix();
            secs * 1000 + u64::from(nanos / 1_000_000)
        })
        .unwrap_or(0);
    Ok(aroh_domain::derived_id(key, part, millis))
}

/// Converts local wall time `text` in `tz` to UTC. A time skipped by a
/// clock change is an error; a repeated one takes its first instant.
/// Returns the instant or why it fails.
pub fn local_to_utc(text: &str, tz: Tz) -> Result<DateTime<Utc>, String> {
    let naive = parse_local(text)?;
    tz.from_local_datetime(&naive)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .ok_or_else(|| format!("`{text}` does not exist in {tz}"))
}

fn timezone(name: &str) -> Result<Tz, ToolError> {
    name.parse().map_err(|_| ToolError::InvalidArgs(format!("unknown timezone `{name}`")))
}

/// One planned item in local wall time: a one-off action, or a series when
/// `rrule` is set. `start` is `YYYY-MM-DDTHH:MM:SS` in the plan's timezone.
/// `minutes` is the length of each occurrence.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanItem {
    pub title: String,
    pub start: String,
    pub minutes: i64,
    pub rrule: Option<String>,
    pub notes: Option<String>,
}

/// One tracker field the planner asks for: key, label, kind
/// (`number`, `text`, `bool`, `choice`) and an optional unit.
/// Choice fields are not planned in this slice.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanField {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub unit: Option<String>,
}

/// When a planned tracker asks for a check-in: `freq` is `daily` or
/// `weekly`, `weekdays` are `Mon` to `Sun` (weekly only), `at` is local
/// `HH:MM`. Stored as the tracker row's `cadence`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanCadence {
    pub freq: String,
    #[serde(default)]
    pub weekdays: Vec<String>,
    pub at: String,
}

const CADENCE_FORMS: &str =
    "use freq daily or weekly, weekdays from Mon Tue Wed Thu Fri Sat Sun (weekly only, empty for daily), at as local HH:MM such as 19:30";

/// Reads planned cadence `c` of tracker `name` into a tracker cadence.
/// Accepts any case, RRULE day codes (MO) and full day names, HH:MM:SS
/// times; daily with some weekdays becomes weekly, all seven days daily,
/// weekly without days a Sunday check-in. Returns `InvalidArgs` naming the
/// allowed forms for an unknown freq, weekday or time.
fn plan_cadence(name: &str, c: &PlanCadence) -> Result<rows::Cadence, ToolError> {
    let bad = |why: String| ToolError::InvalidArgs(format!("bad cadence for `{name}`: {why}; {CADENCE_FORMS}"));
    let freq = match c.freq.trim().to_ascii_lowercase().as_str() {
        "daily" => rows::CadenceFreq::Daily,
        "weekly" => rows::CadenceFreq::Weekly,
        other => return Err(bad(format!("freq `{other}`"))),
    };
    let mut days = Vec::new();
    for d in &c.weekdays {
        let text = d.trim().to_ascii_lowercase();
        let day = match text.as_str() {
            "mo" => Some(chrono::Weekday::Mon),
            "tu" => Some(chrono::Weekday::Tue),
            "we" => Some(chrono::Weekday::Wed),
            "th" => Some(chrono::Weekday::Thu),
            "fr" => Some(chrono::Weekday::Fri),
            "sa" => Some(chrono::Weekday::Sat),
            "su" => Some(chrono::Weekday::Sun),
            _ => text.parse::<chrono::Weekday>().ok(),
        }
        .ok_or_else(|| bad(format!("weekday `{d}`")))?;
        if !days.contains(&day) {
            days.push(day);
        }
    }
    days.sort_by_key(|d| d.num_days_from_monday());
    let at = ["%H:%M", "%H:%M:%S"]
        .iter()
        .find_map(|f| chrono::NaiveTime::parse_from_str(c.at.trim(), f).ok())
        .ok_or_else(|| bad(format!("at `{}`", c.at)))?
        .format("%H:%M")
        .to_string();
    let (freq, weekdays) = match (freq, days.len()) {
        (_, 7) => (rows::CadenceFreq::Daily, Vec::new()),
        (rows::CadenceFreq::Daily, 0) => (rows::CadenceFreq::Daily, Vec::new()),
        (rows::CadenceFreq::Weekly, 0) => (rows::CadenceFreq::Weekly, vec![chrono::Weekday::Sun]),
        (_, _) => (rows::CadenceFreq::Weekly, days),
    };
    assert!((freq == rows::CadenceFreq::Daily) == weekdays.is_empty(), "daily has no weekdays, weekly has some");
    Ok(rows::Cadence { freq, weekdays, at })
}

/// One adaptation rule a planned tracker declares (Q20): `op` is
/// `above`, `below`, `flat` or `missed` over the last `window_days`.
/// Stored as a `trackers.rules` entry when it validates.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanRule {
    pub field: String,
    pub op: String,
    pub value: f64,
    pub window_days: u32,
}

/// One tracker the plan creates, with its fields, an optional check-in
/// cadence and optional rules. Becomes a `trackers` row of the run's
/// goal. Names are free text.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanTracker {
    pub name: String,
    pub fields: Vec<PlanField>,
    #[serde(default)]
    pub cadence: Option<PlanCadence>,
    #[serde(default)]
    pub rules: Option<Vec<PlanRule>>,
}

fn valid_rules(tracker: &Tracker, planned: &[PlanRule]) -> Vec<rows::TrackerRule> {
    planned
        .iter()
        .filter_map(|r| serde_json::from_value::<rows::TrackerRule>(json!(r)).ok())
        .filter(|rule| {
            let mut probe = tracker.clone();
            probe.rules = vec![rule.clone()];
            probe.updated_at = aroh_domain::seed::SEED_STAMP.into();
            rows::Record::validate(&probe).is_ok()
        })
        .collect()
}

/// The planner's output: summary, timezone, items and trackers.
/// The owner approves it before `write_plan` stores it.
/// Matches the `plan` state key of the new-goal workflow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Plan {
    pub summary: String,
    pub timezone: String,
    pub items: Vec<PlanItem>,
    pub trackers: Vec<PlanTracker>,
}

#[derive(Deserialize, JsonSchema)]
struct PlanArgs {
    plan: Plan,
}

async fn write_plan(env: &ToolEnv, plan: &Plan, ctx: &ToolContext) -> Result<Value, ToolError> {
    let goal = env.live_goal().await?;
    let tz = timezone(&plan.timezone)?;
    let (user, goal_text) = (env.user.to_string(), goal.to_string());
    let (mut actions, mut series_count, mut occurrences) = (0, 0, 0);
    for (i, item) in plan.items.iter().enumerate() {
        slots(&serde_json::to_value(item).expect("items serialize")).map_err(ToolError::InvalidArgs)?;
        match item.rrule.as_deref().filter(|r| !r.trim().is_empty()) {
            Some(rule) => {
                let mut series = ActionSeries {
                    id: row_id(ctx, &format!("series:{i}"))?,
                    user_id: user.clone(),
                    goal_id: goal_text.clone(),
                    title: item.title.clone(),
                    notes: item.notes.clone(),
                    rrule: rule.to_string(),
                    dtstart_local: parse_local(&item.start).map_err(ToolError::InvalidArgs)?.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    timezone: plan.timezone.clone(),
                    duration_minutes: item.minutes,
                    updated_at: String::new(),
                    deleted: false,
                };
                put_row(&env.pool, &env.hlc, env.user, &mut series).await.map_err(failed)?;
                occurrences += materialize(&env.pool, env.user, &series, env.now(), DEFAULT_WINDOW_DAYS).await.map_err(failed)?;
                series_count += 1;
            }
            None => {
                let start = local_to_utc(&item.start, tz).map_err(ToolError::InvalidArgs)?;
                let mut action = Action {
                    id: row_id(ctx, &format!("action:{i}"))?,
                    user_id: user.clone(),
                    goal_id: goal_text.clone(),
                    series_id: None,
                    occurrence: None,
                    title: item.title.clone(),
                    notes: item.notes.clone(),
                    start_at: start,
                    end_at: start + Duration::minutes(item.minutes),
                    status: ActionStatus::Planned,
                    locked: false,
                    done_at: None,
                    updated_at: String::new(),
                    deleted: false,
                };
                put_row(&env.pool, &env.hlc, env.user, &mut action).await.map_err(failed)?;
                actions += 1;
            }
        }
    }
    for (i, t) in plan.trackers.iter().enumerate() {
        let fields = t
            .fields
            .iter()
            .map(|f| {
                let kind: FieldKind = serde_json::from_value(json!(f.kind))
                    .map_err(|_| ToolError::InvalidArgs(format!("unknown field kind `{}`", f.kind)))?;
                Ok(TrackerField { key: f.key.clone(), label: f.label.clone(), kind, unit: f.unit.clone(), choices: vec![], health: None })
            })
            .collect::<Result<Vec<_>, ToolError>>()?;
        let cadence = t.cadence.as_ref().map(|c| plan_cadence(&t.name, c)).transpose()?;
        let mut tracker = Tracker {
            id: row_id(ctx, &format!("tracker:{i}"))?,
            user_id: user.clone(),
            goal_id: goal_text.clone(),
            plugin_id: None,
            name: t.name.clone(),
            fields,
            cadence,
            rules: vec![],
            updated_at: String::new(),
            deleted: false,
        };
        tracker.rules = valid_rules(&tracker, t.rules.as_deref().unwrap_or_default());
        put_row(&env.pool, &env.hlc, env.user, &mut tracker).await.map_err(failed)?;
    }
    if let Some(mut g) = get_row::<rows::Goal>(&env.pool, env.user, &goal_text).await.map_err(failed)? {
        if g.status == GoalStatus::Draft {
            g.status = GoalStatus::Active;
            put_row(&env.pool, &env.hlc, env.user, &mut g).await.map_err(failed)?;
        }
    }
    Ok(json!({"actions": actions, "series": series_count, "occurrences": occurrences, "trackers": plan.trackers.len()}))
}

/// One change an adaptation proposes, in local wall time of the goal:
/// `move` or `resize` an action, `create` a new one, `delete` one.
/// `action_id` names the action for every op but `create`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChangeInput {
    pub op: String,
    pub action_id: Option<String>,
    pub title: String,
    pub start: Option<String>,
    pub minutes: Option<i64>,
}

/// The adaptation analyst's output: summary, size and changes.
/// Matches the `proposal` state key of the replan workflow.
/// `record_proposal` turns it into a `proposals` row.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProposalInput {
    pub summary: String,
    pub size: String,
    pub changes: Vec<ChangeInput>,
}

/// The goal state key as tools read it: only the timezone matters here.
/// Other keys of the state object are ignored.
/// Proposals use it to turn local times into UTC.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GoalInput {
    pub timezone: String,
}

/// A time span in UTC, the `before` and `after` of a change.
/// Both ends are RFC 3339 instants.
/// `end_at` is after `start_at`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Span {
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
}

/// One change as stored in `proposals.diff` and shown to the owner:
/// op, action, title, and the span before and after (null for a create's
/// before and a delete's after).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Change {
    pub op: String,
    pub action_id: Option<String>,
    pub title: String,
    pub before: Option<Span>,
    pub after: Option<Span>,
}

/// A recorded proposal: its row id, summary, size and its changes, none on a
/// locked action. The `proposal_record` state key of the replan workflow,
/// read by the autonomy gate and the approval card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProposalRecord {
    pub proposal_id: String,
    pub summary: String,
    pub size: String,
    pub changes: Vec<Change>,
}

#[derive(Deserialize, JsonSchema)]
struct RecordArgs {
    goal: GoalInput,
    proposal: ProposalInput,
}

#[derive(Deserialize, JsonSchema)]
struct DecideArgs {
    proposal_record: ProposalRecord,
}

async fn goal_action(env: &ToolEnv, id: Option<&str>) -> Result<Action, ToolError> {
    let id = id.ok_or_else(|| ToolError::InvalidArgs("this change needs an action_id".into()))?;
    let action = get_row::<Action>(&env.pool, env.user, id).await.map_err(failed)?;
    match action {
        Some(a) if a.goal_id == env.goal()?.to_string() => Ok(a),
        _ => Err(ToolError::InvalidArgs(format!("action `{id}` is not an action of this goal"))),
    }
}

/// The goal action a move, resize or delete names, refused with `InvalidArgs`
/// when it is locked: a fixed action is never changed by an agent (Q53).
async fn flexible_action(env: &ToolEnv, id: Option<&str>) -> Result<Action, ToolError> {
    let a = goal_action(env, id).await?;
    if a.locked {
        return Err(ToolError::InvalidArgs(format!("`{}` is a fixed action: never move, resize or delete it", a.title)));
    }
    Ok(a)
}

fn span_of(a: &Action) -> Span {
    Span { start_at: a.start_at, end_at: a.end_at }
}

async fn record_proposal(env: &ToolEnv, args: RecordArgs, ctx: &ToolContext) -> Result<Value, ToolError> {
    let goal = env.live_goal().await?;
    let tz = timezone(&args.goal.timezone)?;
    let size: ProposalSize = serde_json::from_value(json!(args.proposal.size))
        .map_err(|_| ToolError::InvalidArgs("size must be small or big".into()))?;
    let mut changes = Vec::with_capacity(args.proposal.changes.len());
    for c in &args.proposal.changes {
        let after = || -> Result<Span, ToolError> {
            let (Some(start), Some(minutes)) = (c.start.as_deref(), c.minutes) else {
                return Err(ToolError::InvalidArgs(format!("`{}` needs start and minutes", c.title)));
            };
            if !(1..=1440).contains(&minutes) {
                return Err(ToolError::InvalidArgs(format!("`{}` lasts {minutes} minutes", c.title)));
            }
            let start_at = local_to_utc(start, tz).map_err(ToolError::InvalidArgs)?;
            Ok(Span { start_at, end_at: start_at + Duration::minutes(minutes) })
        };
        let (action_id, before, after) = match c.op.as_str() {
            "move" | "resize" => {
                let a = flexible_action(env, c.action_id.as_deref()).await?;
                (Some(a.id.clone()), Some(span_of(&a)), Some(after()?))
            }
            "delete" => {
                let a = flexible_action(env, c.action_id.as_deref()).await?;
                (Some(a.id.clone()), Some(span_of(&a)), None)
            }
            "create" => (None, None, Some(after()?)),
            other => return Err(ToolError::InvalidArgs(format!("unknown op `{other}`"))),
        };
        changes.push(Change { op: c.op.clone(), action_id, title: c.title.clone(), before, after });
    }
    if changes.is_empty() {
        let record = ProposalRecord { proposal_id: String::new(), summary: args.proposal.summary, size: args.proposal.size, changes };
        return Ok(serde_json::to_value(record).expect("records serialize"));
    }
    let mut row = Proposal {
        id: row_id(ctx, "proposal")?,
        user_id: env.user.to_string(),
        goal_id: goal.to_string(),
        run_id: ctx.run_id.map(|r| r.to_string()),
        summary: args.proposal.summary.clone(),
        diff: json!({ "changes": changes }),
        size,
        status: ProposalStatus::Proposed,
        undo: None,
        created_at: env.now(),
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&env.pool, &env.hlc, env.user, &mut row).await.map_err(failed)?;
    let record = ProposalRecord {
        proposal_id: row.id,
        summary: args.proposal.summary,
        size: args.proposal.size,
        changes,
    };
    Ok(serde_json::to_value(record).expect("records serialize"))
}

fn outcome(status: &str, applied: usize, skipped: usize) -> Value {
    json!({"status": status, "applied": applied, "skipped": skipped})
}

async fn load_proposal(env: &ToolEnv, id: &str) -> Result<Proposal, ToolError> {
    get_row::<Proposal>(&env.pool, env.user, id)
        .await
        .map_err(failed)?
        .filter(|p| Some(p.goal_id.as_str()) == env.goal.map(|g| g.to_string()).as_deref())
        .ok_or_else(|| ToolError::InvalidArgs(format!("proposal `{id}` is not a proposal of this goal")))
}

async fn apply_proposal(env: &ToolEnv, record: &ProposalRecord, ctx: &ToolContext) -> Result<Value, ToolError> {
    let mut proposal = load_proposal(env, &record.proposal_id).await?;
    let stored: Vec<Change> = serde_json::from_value(proposal.diff["changes"].clone()).map_err(failed)?;
    match proposal.status {
        ProposalStatus::Proposed => {}
        ProposalStatus::Applied => return Ok(outcome("applied", stored.len(), 0)),
        other => return Err(ToolError::Failed(format!("proposal is {other:?}, not proposed"))),
    }
    let (mut applied, mut skipped, mut undo) = (0, 0, Vec::new());
    for (i, c) in stored.iter().enumerate() {
        if c.op == "create" {
            let span = c.after.clone().ok_or_else(|| failed("create without a span"))?;
            let mut a = Action {
                id: row_id(ctx, &format!("create:{i}"))?,
                user_id: env.user.to_string(),
                goal_id: proposal.goal_id.clone(),
                series_id: None,
                occurrence: None,
                title: c.title.clone(),
                notes: None,
                start_at: span.start_at,
                end_at: span.end_at,
                status: ActionStatus::Planned,
                locked: false,
                done_at: None,
                updated_at: String::new(),
                deleted: false,
            };
            put_row(&env.pool, &env.hlc, env.user, &mut a).await.map_err(failed)?;
            undo.push(json!({"op": "delete", "action_id": a.id}));
            applied += 1;
            continue;
        }
        let mut a = goal_action(env, c.action_id.as_deref()).await?;
        if a.locked {
            skipped += 1;
            continue;
        }
        undo.push(json!({"op": "restore", "action_id": a.id, "before": span_of(&a), "deleted": a.deleted}));
        match &c.after {
            Some(span) => {
                a.start_at = span.start_at;
                a.end_at = span.end_at;
            }
            None => a.deleted = true,
        }
        put_row(&env.pool, &env.hlc, env.user, &mut a).await.map_err(failed)?;
        applied += 1;
    }
    proposal.status = ProposalStatus::Applied;
    proposal.undo = Some(json!({ "changes": undo }));
    put_row(&env.pool, &env.hlc, env.user, &mut proposal).await.map_err(failed)?;
    Ok(outcome("applied", applied, skipped))
}

async fn reject_proposal(env: &ToolEnv, record: &ProposalRecord) -> Result<Value, ToolError> {
    let mut proposal = load_proposal(env, &record.proposal_id).await?;
    if proposal.status == ProposalStatus::Proposed {
        proposal.status = ProposalStatus::Rejected;
        put_row(&env.pool, &env.hlc, env.user, &mut proposal).await.map_err(failed)?;
    }
    Ok(outcome("rejected", 0, 0))
}

/// One value the log intake mapped: a tracker field key and the value as
/// text, converted by the field's kind when stored.
/// Booleans are `true` or `false`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EntryValue {
    pub key: String,
    pub value: String,
}

/// One tracker entry the log intake mapped from free text.
/// `tracker_id` must name a tracker of the run's user.
/// Values name that tracker's fields.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EntryInput {
    pub tracker_id: String,
    pub values: Vec<EntryValue>,
}

/// The logged text as the run received it: text, when it happened (RFC
/// 3339) and the owner's timezone.
/// The `log` state key of the log-intake workflow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LogInput {
    pub text: String,
    pub at: String,
    pub timezone: String,
}

/// The log intake persona's output: the entries it mapped.
/// Empty when the text matched no tracker.
/// The `entries` state key of the log-intake workflow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Entries {
    pub entries: Vec<EntryInput>,
}

#[derive(Deserialize, JsonSchema)]
struct EntriesArgs {
    log: LogInput,
    entries: Entries,
}

fn field_value(field: &TrackerField, text: &str) -> Result<Value, ToolError> {
    let bad = || ToolError::InvalidArgs(format!("`{text}` is not a value for {}", field.key));
    Ok(match field.kind {
        FieldKind::Number => json!(text.trim().parse::<f64>().map_err(|_| bad())?),
        FieldKind::Bool => json!(text.trim().parse::<bool>().map_err(|_| bad())?),
        FieldKind::Text | FieldKind::Choice => json!(text),
    })
}

async fn write_entries(env: &ToolEnv, args: EntriesArgs, ctx: &ToolContext) -> Result<Value, ToolError> {
    let logged_at = DateTime::parse_from_rfc3339(&args.log.at).map(|t| t.with_timezone(&Utc)).unwrap_or_else(|_| env.now());
    let mut written = 0;
    for (i, e) in args.entries.entries.iter().enumerate() {
        let tracker = get_row::<Tracker>(&env.pool, env.user, &e.tracker_id)
            .await
            .map_err(failed)?
            .ok_or_else(|| ToolError::InvalidArgs(format!("tracker `{}` is not one of yours", e.tracker_id)))?;
        let mut data = Map::new();
        for v in &e.values {
            let field = tracker
                .fields
                .iter()
                .find(|f| f.key == v.key)
                .ok_or_else(|| ToolError::InvalidArgs(format!("{} is not a field of {}", v.key, tracker.name)))?;
            data.insert(v.key.clone(), field_value(field, &v.value)?);
        }
        let mut entry = TrackerEntry {
            id: row_id(ctx, &format!("entry:{i}"))?,
            user_id: env.user.to_string(),
            tracker_id: tracker.id.clone(),
            data,
            source: rows::EntrySource::Log,
            raw_text: Some(args.log.text.chars().take(4000).collect()),
            logged_at,
            updated_at: String::new(),
            deleted: false,
        };
        rows::validate_entry(&tracker, &entry).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        put_row(&env.pool, &env.hlc, env.user, &mut entry).await.map_err(failed)?;
        written += 1;
    }
    Ok(json!({ "entries": written }))
}

/// The plugin builder's output: one bundle as TOML text.
/// Matches the `plugin_draft` state key of the new-goal workflow.
/// Validated again by `store_plugin` before it is stored.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginDraft {
    pub bundle_toml: String,
}

#[derive(Deserialize, JsonSchema)]
struct StorePluginArgs {
    plugin_draft: PluginDraft,
}

async fn store_plugin(env: &ToolEnv, draft: &PluginDraft, ctx: &ToolContext) -> Result<Value, ToolError> {
    let goal = env.live_goal().await?;
    let writer = crate::plugins::Writer { pool: &env.pool, hlc: &env.hlc, user: env.user };
    let row = writer.store(goal, &draft.bundle_toml, row_id(ctx, "plugin")?, true).await.map_err(|e| match e {
        crate::error::ServerError::Setup(m) => ToolError::InvalidArgs(m),
        other => failed(other),
    })?;
    Ok(json!({"id": row.id, "name": row.name, "version": row.version}))
}

fn json_output(v: Value) -> ToolOutput {
    ToolOutput::text(v.to_string())
}

/// The Aroh toolkit of one run: `write_plan`, `record_proposal`,
/// `apply_proposal`, `reject_proposal`, `write_entries`, `store_plugin` and
/// `propose_preference`, all writing through the domain rules as `env.user`. Workflow tool nodes call them.
pub fn aroh_toolkit(env: ToolEnv) -> Toolkit {
    let e1 = env.clone();
    let write = typed_tool("write_plan", "Store an approved plan as actions, series and trackers.", move |a: PlanArgs, ctx: ToolContext| {
        let env = e1.clone();
        async move { write_plan(&env, &a.plan, &ctx).await.map(json_output) }
    });
    let e2 = env.clone();
    let record = typed_tool("record_proposal", "Store a plan change as a proposal for the Inbox.", move |a: RecordArgs, ctx: ToolContext| {
        let env = e2.clone();
        async move { record_proposal(&env, a, &ctx).await.map(json_output) }
    });
    let e3 = env.clone();
    let apply = typed_tool("apply_proposal", "Apply a recorded proposal, never moving locked actions.", move |a: DecideArgs, ctx: ToolContext| {
        let env = e3.clone();
        async move { apply_proposal(&env, &a.proposal_record, &ctx).await.map(json_output) }
    });
    let e4 = env.clone();
    let reject = typed_tool("reject_proposal", "Mark a recorded proposal as rejected.", move |a: DecideArgs, _ctx: ToolContext| {
        let env = e4.clone();
        async move { reject_proposal(&env, &a.proposal_record).await.map(json_output) }
    });
    let e5 = env.clone();
    let entries = typed_tool("write_entries", "Store tracker entries mapped from a free-text log.", move |a: EntriesArgs, ctx: ToolContext| {
        let env = e5.clone();
        async move { write_entries(&env, a, &ctx).await.map(json_output) }
    });
    let e6 = env.clone();
    let plugin = typed_tool("store_plugin", "Validate and store a goal's plugin bundle as its next enabled version.", move |a: StorePluginArgs, ctx: ToolContext| {
        let env = e6.clone();
        async move { store_plugin(&env, &a.plugin_draft, &ctx).await.map(json_output) }
    });
    let e7 = env.clone();
    let propose = typed_tool(
        "propose_preference",
        "Store one unconfirmed owner preference learned from similar edits.",
        move |a: crate::learning::ProposeArgs, _ctx: ToolContext| {
            let env = e7.clone();
            async move { crate::learning::propose_preference(&env, a).await.map(json_output) }
        },
    );
    let e8 = env;
    let facts = typed_tool(
        "save_facts",
        "Keep lasting answers as confirmed profile facts and refresh the facts known questions came from.",
        move |a: crate::profile::SaveFactsArgs, _ctx: ToolContext| {
            let env = e8.clone();
            async move { crate::profile::save_facts(&env, a).await.map(json_output) }
        },
    );
    Toolkit::new("aroh")
        .with(Arc::new(write))
        .with(Arc::new(record))
        .with(Arc::new(apply))
        .with(Arc::new(reject))
        .with(Arc::new(entries))
        .with(Arc::new(plugin))
        .with(Arc::new(propose))
        .with(Arc::new(facts))
}

/// Returns a naive local time as the `YYYY-MM-DDTHH:MM:SS` text plans use.
/// The inverse of `parse_local` for whole seconds.
/// Used when building run inputs from stored actions.
pub fn local_text(t: NaiveDateTime) -> String {
    t.format("%Y-%m-%dT%H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Weekday;
    use rows::{Cadence, CadenceFreq};

    fn planned(freq: &str, weekdays: &[&str], at: &str) -> PlanCadence {
        PlanCadence { freq: freq.into(), weekdays: weekdays.iter().map(|d| d.to_string()).collect(), at: at.into() }
    }

    #[test]
    fn cadence_accepts_common_spellings() {
        let weekly =
            Cadence { freq: CadenceFreq::Weekly, weekdays: vec![Weekday::Mon, Weekday::Wed, Weekday::Fri], at: "19:30".into() };
        for c in [
            planned("weekly", &["Mon", "Wed", "Fri"], "19:30"),
            planned("Weekly", &["MO", "WE", "FR"], "19:30:00"),
            planned("WEEKLY", &["friday", "monday", "Wednesday", "mon"], "19:30"),
            planned("daily", &["MO", "WE", "FR"], "19:30"),
        ] {
            assert_eq!(plan_cadence("quizzes", &c).unwrap(), weekly, "{c:?}");
        }
        let daily = Cadence { freq: CadenceFreq::Daily, weekdays: vec![], at: "07:05".into() };
        assert_eq!(plan_cadence("q", &planned("daily", &[], "7:05")).unwrap(), daily);
        let all = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];
        assert_eq!(plan_cadence("q", &planned("daily", &all, "07:05")).unwrap(), daily);
        assert_eq!(plan_cadence("q", &planned("weekly", &all, "07:05")).unwrap(), daily);
        let sunday = Cadence { freq: CadenceFreq::Weekly, weekdays: vec![Weekday::Sun], at: "18:00".into() };
        assert_eq!(plan_cadence("q", &planned("weekly", &[], "18:00")).unwrap(), sunday);
    }

    #[test]
    fn bad_cadence_names_the_allowed_forms() {
        for c in [
            planned("monthly", &[], "19:30"),
            planned("weekly", &["Mx"], "19:30"),
            planned("daily", &[], "7pm"),
        ] {
            let err = plan_cadence("quizzes", &c).unwrap_err().to_string();
            assert!(
                err.contains("quizzes") && err.contains("daily or weekly") && err.contains("Mon") && err.contains("HH:MM"),
                "{err}"
            );
        }
    }
}
