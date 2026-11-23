use crate::checkpoints::{events_after, EventBuffer, PgCheckpoints};
use crate::error::ServerError;
use crate::io::{connect, Deps, RunIo};
use crate::ledger::{goal_key, user_key, UsageWriter};
use crate::queue::PgQueue;
use crate::rows::{get_row, goal_deleted, insert_run, list_rows, prepare_reads, put_row, run_row, run_row_for, utc, RunRow};
use crate::tools::local_text;
use crate::vault::{check_vendor, KeyInfo};
use crate::teams;
use crate::triage::{triage_input, StartTriage};
use crate::workflows::{self, CALENDAR_TRIAGE, LOG_INTAKE, NEW_GOAL, REPLAN};
use crate::db;
use aroh_domain::calendar::local_days;
use aroh_domain::recur::DEFAULT_WINDOW_DAYS;
use aroh_domain::rows::{
    Action, ActionSeries, ActionStatus, Autonomy, Budget, BudgetScope, BusyBlock, DayKind, Goal, GoalStatus, Place, PlaceLink,
    ProviderSetting, SearchSetting, Tracker,
};
use aroh_domain::time::MISSED_GRACE_MIN;
use crate::triggers;
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::sync::Arc;
use tokio::sync::watch;
use troupekit_agent::{Limits, PeriodLimit};
use troupekit_core::ids::RunId;
use troupekit_core::secret::Secret;
use troupekit_core::store::CheckpointStore;
use troupekit_graph::{Ask, GraphRun, Stop, Workflow};
use troupekit_host::{JobQueue, Lease, Processed, Runner, TickOptions, TickReport, WorkOptions};
use troupekit_providers::CompletionRequest;
use utoipa::ToSchema;
use uuid::Uuid;

const STORE_RETRY_MS: i64 = 1_000;
const DEFAULT_LEASE_MS: i64 = 300_000;
const PAST_DAYS: i64 = 7;
const MAX_TEXT: usize = 4_000;
const RUN_LIST_LIMIT: i64 = 50;

/// Body of `POST /v1/goals`: the goal to plan. Without `goal_id`, or when
/// the server has no such goal yet, a draft goal row is created with it.
/// `timezone` is the owner's IANA zone, used for every local time. The
/// agent controls (`reminders`, `use_calendars`, `run_limit_minutes`) are
/// optional: left out, the goal keeps the Settings defaults.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StartGoal {
    pub goal_id: Option<Uuid>,
    pub title: String,
    pub context: Option<String>,
    pub deadline: Option<String>,
    pub category_id: Option<Uuid>,
    #[schema(value_type = Option<String>, example = "balanced")]
    pub autonomy: Option<Autonomy>,
    pub timezone: String,
    #[serde(default)]
    pub reminders: Option<bool>,
    #[serde(default)]
    pub use_calendars: Option<bool>,
    #[serde(default)]
    pub run_limit_minutes: Option<i64>,
}

/// Body of `POST /v1/goals/{goal_id}/replan`: why to adapt, in the owner's
/// words or the app's (missed actions, drift), and the owner's timezone.
/// The goal's autonomy decides whether the change needs approval.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StartReplan {
    pub reason: String,
    pub timezone: String,
}

/// Body of `POST /v1/logs`: one free-text log, when it happened (RFC 3339,
/// default now) and the owner's timezone. The run maps it onto the
/// owner's trackers and stores the entries.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitLog {
    pub text: String,
    pub at: Option<DateTime<Utc>>,
    pub timezone: String,
}

/// Body of `PUT /v1/keys/{setting_id}`: the vendor and the key. When the
/// server has no setting with that id and `model` is given, a provider
/// setting row is created with it, so the key can be used at once.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SetKey {
    pub vendor: String,
    #[schema(value_type = String, format = Password)]
    pub key: Secret,
    pub model: Option<String>,
    pub label: Option<String>,
    pub is_default: Option<bool>,
}

/// Body of `POST /v1/runs/{run_id}/answers`: the human node and its answer.
/// Approval nodes take a bool, `questions` takes `[{id, answer}]` with
/// `answer` a string, or null for a skipped question, and `change` a string.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    pub node: String,
    #[schema(value_type = Object)]
    pub answer: Value,
}

/// The owner's run time limit: minutes of active work a run may take
/// before it stops, 1 to 120; time waiting for the owner never counts.
/// Five minutes until the owner sets one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RunLimit {
    pub minutes: u32,
}

/// Run time limit for owners who never set one, in minutes.
pub const DEFAULT_RUN_MINUTES: u32 = 5;

/// Most minutes of active work an owner may give one run.
pub const MAX_RUN_MINUTES: u32 = 120;

/// Tries a stop makes when another worker saves the run at the same time.
const STOP_TRIES: usize = 3;

impl RunLimit {
    /// The limit in milliseconds of active work, as a run carries it.
    fn ms(self) -> u64 {
        u64::from(self.minutes) * 60_000
    }
}

/// What a start returns: the goal (none for a log) and the new run.
/// Poll `GET /v1/runs/{run_id}` or listen to `run_events` for progress.
/// Answered with 202.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Started {
    pub goal_id: Option<Uuid>,
    pub run_id: Uuid,
}

/// One human node a run waits at: node id, `approval` or `question`, the
/// prompt, and the state keys the node reads (the plan, the questions or
/// the proposal), as the app shows them.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Waiting {
    pub node: String,
    pub ask: String,
    pub prompt: Option<String>,
    #[schema(value_type = Object)]
    pub data: Map<String, Value>,
}

/// A run as the app reads it: ids, workflow, status (tagged by `status`),
/// what it waits for, usage so far, and the events after `after_seq`
/// (each a core `Event` with `run_id`, `seq`, `at_ms` and `body`).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct RunView {
    pub run_id: Uuid,
    pub goal_id: Option<Uuid>,
    pub workflow: String,
    #[schema(value_type = Object)]
    pub status: Value,
    pub waiting: Vec<Waiting>,
    #[schema(value_type = Object)]
    pub usage: Value,
    #[schema(value_type = Vec<Object>)]
    pub events: Vec<Value>,
    pub summary: Option<String>,
    /// What research found (`summary`, `sources`), once written.
    #[schema(value_type = Option<Object>)]
    pub research: Option<Value>,
    /// The submitted answers with their question text.
    pub answers: Vec<crate::insights::RunAnswer>,
}

/// One row of the run list behind the Inbox: ids, workflow, lifecycle
/// state, times, and the sorted slugs of the personas its snapshot holds
/// (the coaches that work on the run). `waiting_human` runs need the
/// owner. Newest first.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct RunSummary {
    pub run_id: Uuid,
    pub goal_id: Option<Uuid>,
    pub workflow: String,
    pub state: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub personas: Vec<String>,
}

/// Result of a key test: whether one 1-token call with the stored key
/// worked, and the provider's error when it did not.
/// The key itself is never part of it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct KeyTest {
    pub ok: bool,
    pub error: Option<String>,
}

/// What a tick did, as `POST /v1/tick` answers it: jobs processed, how
/// many were lost to another worker, and each failure's text.
/// Mirrors Troupekit's `TickReport`.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct TickSummary {
    pub processed: usize,
    pub lost: usize,
    pub errors: Vec<String>,
}

impl From<TickReport> for TickSummary {
    fn from(r: TickReport) -> Self {
        TickSummary { processed: r.processed, lost: r.lost, errors: r.errors }
    }
}

/// A `questions` answer with its skips (`answer: null`) written as the text
/// `skipped`, for runs whose workflow snapshot predates skippable questions.
/// Returns None when nothing was skipped, so the original refusal stands.
fn legacy_skips(answer: &Answer) -> Option<Value> {
    let items = answer.answer.as_array().filter(|_| answer.node == "questions")?;
    if !items.iter().any(|a| a.get("answer").is_some_and(Value::is_null)) {
        return None;
    }
    let mapped = items
        .iter()
        .map(|a| {
            let mut a = a.clone();
            if let Some(slot) = a.get_mut("answer").filter(|v| v.is_null()) {
                *slot = Value::String("skipped".into());
            }
            a
        })
        .collect();
    Some(Value::Array(mapped))
}

/// The plan input over `[from, until)` in `tz`: `busy` holds timed blocks, one full local day
/// per Busy all-day day and every live planned locked action of `fixed` (Q53), in start order;
/// the second list holds the local `YYYY-MM-DD` dates of Away days. Reminders, unchosen
/// all-day events and deleted rows change nothing.
pub fn day_inputs(blocks: &[BusyBlock], fixed: &[Action], tz: Tz, from: DateTime<Utc>, until: DateTime<Utc>) -> (Vec<Value>, Vec<String>) {
    let mut busy: Vec<(String, Value)> = Vec::new();
    let mut away = std::collections::BTreeSet::new();
    let today = from.with_timezone(&tz).date_naive();
    let last = until.with_timezone(&tz).date_naive();
    for b in blocks.iter().filter(|b| !b.deleted && b.end_at > from && b.start_at < until) {
        if !b.all_day {
            let start = local_text(b.start_at.with_timezone(&tz).naive_local());
            busy.push((start.clone(), json!({"title": b.title, "start": start, "minutes": (b.end_at - b.start_at).num_minutes().clamp(1, 24 * 60)})));
            continue;
        }
        let days = local_days(b.start_at, b.end_at, tz).into_iter().filter(|d| *d >= today && *d <= last);
        match b.day_kind {
            Some(DayKind::Busy) => {
                for d in days {
                    let start = local_text(d.and_hms_opt(0, 0, 0).expect("midnight exists"));
                    busy.push((start.clone(), json!({"title": b.title, "start": start, "minutes": 24 * 60})));
                }
            }
            Some(DayKind::Away) => away.extend(days.map(|d| d.to_string())),
            Some(DayKind::Reminder) | None => {}
        }
    }
    for a in fixed.iter().filter(|a| a.locked && !a.deleted && a.status == ActionStatus::Planned && a.end_at > from && a.start_at < until) {
        let start = local_text(a.start_at.with_timezone(&tz).naive_local());
        busy.push((start.clone(), json!({"title": a.title, "start": start, "minutes": (a.end_at - a.start_at).num_minutes().clamp(1, 24 * 60)})));
    }
    busy.sort_by(|a, b| a.0.cmp(&b.0));
    (busy.into_iter().map(|(_, v)| v).collect(), away.into_iter().collect())
}

pub(crate) fn parse_tz(name: &str) -> Result<Tz, ServerError> {
    name.parse().map_err(|_| ServerError::BadRequest(format!("unknown timezone `{name}`")))
}

/// Joins a goal's `title` and optional `context` into one text.
/// The team classifier in `teams` reads this text for keywords.
/// Returns the joined text; an absent context adds nothing.
pub(crate) fn about(title: &str, context: Option<&str>) -> String {
    format!("{title} {}", context.unwrap_or(""))
}

fn check_text(field: &str, text: &str) -> Result<(), ServerError> {
    let n = text.chars().count();
    if n == 0 || n > MAX_TEXT || text.trim().is_empty() {
        return Err(ServerError::BadRequest(format!("{field} must have 1 to {MAX_TEXT} characters")));
    }
    Ok(())
}

/// The server engine: starts runs, answers them, reads them, and steps
/// queued runs in step mode (`tick`) or worker mode (`work`). Every
/// lease gets its own run io, built for the run's user.
pub struct Engine {
    deps: Arc<Deps>,
    queue: Arc<PgQueue>,
    lease_ms: i64,
    push: Option<Arc<crate::push::Pusher>>,
    last_scan: std::sync::Mutex<Option<i64>>,
}

impl Engine {
    /// Builds the engine over `deps` with a 300 s lease.
    /// One per process; requests and workers share it.
    /// Returns the engine.
    pub fn new(deps: Arc<Deps>) -> Engine {
        let queue = Arc::new(PgQueue::new(deps.pool.clone(), deps.clock.clone()));
        Engine { deps, queue, lease_ms: DEFAULT_LEASE_MS, push: None, last_scan: std::sync::Mutex::new(None) }
    }

    /// Sets the lease length; it must be positive and outlast one step.
    /// Tests shorten it to exercise lease expiry.
    /// Returns the engine.
    pub fn with_lease_ms(mut self, lease_ms: i64) -> Engine {
        assert!(lease_ms > 0, "a lease lasts some time");
        self.lease_ms = lease_ms;
        self
    }

    /// Attaches the APNs pusher; without one, push dispatch reports itself off.
    /// Set once at start when the APNs variables are configured.
    /// Returns the engine.
    pub fn with_push(mut self, pusher: Arc<crate::push::Pusher>) -> Engine {
        self.push = Some(pusher);
        self
    }

    /// The APNs pusher, when push is configured.
    /// Worker mode loops over it; step mode calls it from `/v1/push/tick`.
    /// Returns it borrowed.
    pub fn push(&self) -> Option<&Arc<crate::push::Pusher>> {
        self.push.as_ref()
    }

    /// The shared dependencies, for the API's key routes.
    /// Same object every call.
    /// Returns the deps.
    pub fn deps(&self) -> &Deps {
        &self.deps
    }

    /// The Postgres job queue the engine claims from.
    /// Tests inspect job status through it.
    /// Returns the queue.
    pub fn queue(&self) -> &PgQueue {
        &self.queue
    }

    fn now(&self) -> DateTime<Utc> {
        utc(self.deps.clock.now_ms())
    }

    /// Takes the scan slot of this process when `every_ms` passed since
    /// the last one taken at `now_ms` (or none was taken yet).
    /// Returns whether the caller scans now.
    pub(crate) fn claim_scan(&self, now_ms: i64, every_ms: i64) -> bool {
        let mut last = self.last_scan.lock().unwrap_or_else(|p| p.into_inner());
        if last.is_some_and(|t| now_ms - t < every_ms) {
            return false;
        }
        *last = Some(now_ms);
        true
    }

    /// The owner's confirmed facts, newest first, blocked topics left out:
    /// the profile planning and the intake read as state `profile` (Q12, Q37,
    /// Q62). Returns each fact with id, topic, source and date.
    pub(crate) async fn profile(&self, user: Uuid) -> Result<Vec<crate::profile::ProfileFact>, ServerError> {
        crate::profile::profile(&self.deps.pool, user).await
    }

    /// The owner's named places (Q56) as planning reads them in state `places`: each name
    /// with the titles of its linked goals and routines, never coordinates or addresses.
    /// Raw visits never reach the server. Returns one entry per live place, in id order.
    pub(crate) async fn places(&self, user: Uuid) -> Result<Vec<Value>, ServerError> {
        let places: Vec<Place> = list_rows::<Place>(&self.deps.pool, user, &[], false).await?;
        if places.is_empty() {
            return Ok(Vec::new());
        }
        let links: Vec<PlaceLink> = list_rows::<PlaceLink>(&self.deps.pool, user, &[], false).await?;
        let goals: Vec<Goal> = list_rows::<Goal>(&self.deps.pool, user, &[], false).await?;
        let series: Vec<ActionSeries> = list_rows::<ActionSeries>(&self.deps.pool, user, &[], false).await?;
        Ok(places
            .iter()
            .map(|p| {
                let mine: Vec<&PlaceLink> = links.iter().filter(|l| l.place_id == p.id).collect();
                let mut goal_titles: Vec<&str> = mine.iter().filter_map(|l| goals.iter().find(|g| g.id == l.goal_id)).map(|g| g.title.as_str()).collect();
                goal_titles.sort_unstable();
                goal_titles.dedup();
                let routines: Vec<&str> = mine.iter().filter_map(|l| l.series_id.as_deref()).filter_map(|s| series.iter().find(|x| x.id == s)).map(|s| s.title.as_str()).collect();
                json!({"name": p.name, "goals": goal_titles, "routines": routines})
            })
            .collect())
    }

    async fn limits(&self, user: Uuid, goal: Option<Uuid>) -> Result<Limits, ServerError> {
        let budgets = list_rows::<Budget>(&self.deps.pool, user, &[], false).await?;
        let goal_text = goal.map(|g| g.to_string());
        let pick = |scope: BudgetScope, for_goal: bool| {
            budgets
                .iter()
                .filter(|b| b.scope == scope && (!for_goal || b.goal_id == goal_text))
                .map(|b| u64::try_from(b.limit_usd_micros).unwrap_or(0))
                .min()
        };
        Ok(Limits {
            call: pick(BudgetScope::Call, false),
            run: pick(BudgetScope::Run, false),
            scope: goal.map(|g| PeriodLimit {
                key: goal_key(g),
                max_usd_micros: pick(BudgetScope::GoalMonth, true).unwrap_or(u64::MAX),
            }),
            user: pick(BudgetScope::UserMonth, false).map(|m| PeriodLimit { key: user_key(user), max_usd_micros: m }),
        })
    }

    pub(crate) async fn start(&self, user: Uuid, goal: Option<Uuid>, name: &str, input: Map<String, Value>) -> Result<RunId, ServerError> {
        self.launch(user, goal, workflows::workflow(name)?, input).await
    }

    /// Runs the parsed workflow `wf` for `user` (and `goal`) on `input`.
    /// Takes a parsed workflow so a caller can staff its agent nodes first.
    /// Returns the new run id, or the setup error.
    async fn launch(&self, user: Uuid, goal: Option<Uuid>, wf: Workflow, input: Map<String, Value>) -> Result<RunId, ServerError> {
        let name = wf.name.clone();
        let personas = workflows::snapshot(&self.deps.pool, user, goal, &wf).await?;
        let events = Arc::new(EventBuffer::default());
        let io = RunIo::build(self.deps.clone(), user, goal, None, &personas, events.clone()).await?;
        let limits = self.limits(user, goal).await?;
        let run = GraphRun::new(wf, input, limits, &io).await?.with_time_limit(self.limit_for(user, goal).await?.ms());
        let row = RunRow { id: run.run_id(), user_id: user, goal_id: goal, workflow: name, personas };
        insert_run(&self.deps.pool, &row).await?;
        let runner = self.runner(events, io);
        runner.enqueue(&run).await?;
        Ok(run.run_id())
    }

    fn runner(&self, events: Arc<EventBuffer>, io: RunIo) -> Runner {
        let queue: Arc<dyn JobQueue> = self.queue.clone();
        let checkpoints: Arc<dyn CheckpointStore> = Arc::new(PgCheckpoints::new(self.deps.pool.clone(), events));
        Runner::new(queue, checkpoints, Arc::new(io)).with_lease_ms(self.lease_ms)
    }

    async fn calendar(&self, user: Uuid, tz: Tz, goal: Option<&str>, from: DateTime<Utc>) -> Result<Vec<Value>, ServerError> {
        let filters: Vec<(&str, String)> = goal.map(|g| vec![("goal_id", g.to_string())]).unwrap_or_default();
        let until = self.now() + Duration::days(i64::from(DEFAULT_WINDOW_DAYS));
        let mut actions: Vec<Action> = list_rows::<Action>(&self.deps.pool, user, &filters, false)
            .await?
            .into_iter()
            .filter(|a| a.end_at > from && a.start_at < until)
            .collect();
        actions.sort_by_key(|a| a.start_at);
        let missed_before = self.now() - Duration::minutes(MISSED_GRACE_MIN);
        for a in actions.iter_mut().filter(|a| a.status == ActionStatus::Planned && a.end_at < missed_before) {
            a.status = ActionStatus::Missed;
        }
        Ok(actions
            .iter()
            .map(|a| {
                let mut v = json!({
                    "title": a.title,
                    "start": local_text(a.start_at.with_timezone(&tz).naive_local()),
                    "minutes": (a.end_at - a.start_at).num_minutes(),
                });
                if goal.is_some() {
                    v["id"] = json!(a.id);
                    v["status"] = serde_json::to_value(a.status).expect("statuses serialize");
                    v["locked"] = json!(a.locked);
                }
                v
            })
            .collect())
    }

    async fn busy(&self, user: Uuid, goal: &Goal, tz: Tz, from: DateTime<Utc>) -> Result<(Vec<Value>, Vec<String>), ServerError> {
        if !goal.use_calendars {
            return Ok((Vec::new(), Vec::new()));
        }
        let until = from + Duration::days(i64::from(DEFAULT_WINDOW_DAYS));
        let blocks: Vec<BusyBlock> = list_rows::<BusyBlock>(&self.deps.pool, user, &[], false).await?;
        let actions: Vec<Action> = list_rows::<Action>(&self.deps.pool, user, &[], false).await?;
        Ok(day_inputs(&blocks, &actions, tz, from, until))
    }

    fn goal_input(goal: &Goal, tz: Tz, now: DateTime<Utc>) -> Value {
        json!({
            "id": goal.id,
            "title": goal.title,
            "context": goal.context,
            "deadline": goal.deadline,
            "timezone": tz.name(),
            "today": now.with_timezone(&tz).date_naive().to_string(),
        })
    }

    /// Starts the new-goal workflow for `user`: creates the draft goal row
    /// when the server lacks it, readies reads, and queues the run.
    /// Returns the goal and run, or `Setup` when no provider key is usable.
    pub async fn start_goal(&self, user: Uuid, req: StartGoal) -> Result<Started, ServerError> {
        let tz = parse_tz(&req.timezone)?;
        let now = self.now();
        prepare_reads(&self.deps.pool, user, now, DEFAULT_WINDOW_DAYS).await?;
        let id = req.goal_id.unwrap_or_else(|| Uuid::now_v7());
        let goal = match list_rows::<Goal>(&self.deps.pool, user, &[("id", id.to_string())], true).await?.pop() {
            Some(g) if !g.deleted => g,
            Some(_) => return Err(ServerError::NotFound(format!("goal {id}"))),
            None => {
                let inferred = || aroh_domain::category::guess(&req.title, req.context.as_deref()).and_then(|slug| teams::seeded_id(user, slug));
                let category = req.category_id.map(|c| c.to_string()).or_else(inferred);
                let mut g = Goal {
                    id: id.to_string(),
                    user_id: user.to_string(),
                    category_id: category,
                    title: req.title.clone(),
                    context: req.context.clone(),
                    deadline: req.deadline.clone(),
                    status: GoalStatus::Draft,
                    autonomy: req.autonomy.unwrap_or(Autonomy::Balanced),
                    created_at: now,
                    reminders: req.reminders.unwrap_or(true),
                    use_calendars: req.use_calendars.unwrap_or(true),
                    run_limit_minutes: req.run_limit_minutes,
                    updated_at: String::new(),
                    deleted: false,
                };
                put_row(&self.deps.pool, &self.deps.hlc, user, &mut g).await?;
                g
            }
        };
        let mut input = Map::new();
        input.insert("goal".into(), Self::goal_input(&goal, tz, now));
        input.insert("calendar".into(), json!(self.calendar(user, tz, None, now).await?));
        input.insert("profile".into(), json!(self.profile(user).await?));
        input.insert("blocked_topics".into(), json!(crate::profile::blocked_topics(&self.deps.pool, user).await?));
        input.insert("files".into(), json!(crate::files::goal_files(&self.deps.pool, user, &goal.id).await?));
        input.insert("places".into(), json!(self.places(user).await?));
        let (busy, away_days) = self.busy(user, &goal, tz, now).await?;
        input.insert("busy".into(), json!(busy));
        input.insert("away_days".into(), json!(away_days));
        input.insert("limits".into(), crate::limits::initial());
        input.insert("limit_answers".into(), json!([]));
        let team = teams::for_goal(user, goal.category_id.as_deref(), &about(&goal.title, goal.context.as_deref()));
        let template = aroh_plugins::template(team.template).expect("every team template ships");
        input.insert("plugin_guide".into(), json!({"template": template, "reference": aroh_plugins::reference()}));
        let mut wf = workflows::workflow(NEW_GOAL)?;
        teams::staff(&mut wf, team);
        let run = self.launch(user, Some(id), wf, input).await?;
        self.ensure_review(user, id, tz).await?;
        Ok(Started { goal_id: Some(id), run_id: run.0 })
    }

    /// Starts the replan workflow for `user`'s goal `goal_id` with `req`'s
    /// reason; the goal's autonomy and locked actions decide approval.
    /// Returns the run, or `NotFound` for a goal the user does not have.
    pub async fn start_replan(&self, user: Uuid, goal_id: Uuid, req: StartReplan) -> Result<Started, ServerError> {
        check_text("reason", &req.reason)?;
        let tz = parse_tz(&req.timezone)?;
        let goal = get_row::<Goal>(&self.deps.pool, user, &goal_id.to_string())
            .await?
            .filter(|g| !g.deleted)
            .ok_or_else(|| ServerError::NotFound(format!("goal {goal_id}")))?;
        self.ensure_review(user, goal_id, tz).await?;
        let run = self.replan(user, &goal, &req.reason, tz).await?;
        Ok(Started { goal_id: Some(goal_id), run_id: run.0 })
    }

    /// Starts a replan of `user`'s `goal` for `reason` in zone `tz`, for the
    /// owner and every trigger alike: calendar, profile and autonomy as
    /// input, then the goal's pending action events are claimed by the run.
    pub(crate) async fn replan(&self, user: Uuid, goal: &Goal, reason: &str, tz: Tz) -> Result<RunId, ServerError> {
        let now = self.now();
        let goal_id = Uuid::parse_str(&goal.id).map_err(|_| ServerError::Db("bad goal id".into()))?;
        prepare_reads(&self.deps.pool, user, now, DEFAULT_WINDOW_DAYS).await?;
        let mut input = Map::new();
        input.insert("goal".into(), Self::goal_input(goal, tz, now));
        let from = now - Duration::days(PAST_DAYS);
        input.insert("calendar".into(), json!(self.calendar(user, tz, Some(&goal.id), from).await?));
        let (busy, away_days) = self.busy(user, &goal, tz, now).await?;
        input.insert("busy".into(), json!(busy));
        input.insert("away_days".into(), json!(away_days));
        input.insert("reason".into(), json!(reason));
        input.insert("autonomy".into(), serde_json::to_value(goal.autonomy).expect("autonomy serializes"));
        input.insert("profile".into(), json!(self.profile(user).await?));
        input.insert("places".into(), json!(self.places(user).await?));
        let run = self.start(user, Some(goal_id), REPLAN, input).await?;
        triggers::claim_status_events(&self.deps.pool, user, goal_id, run, now).await?;
        Ok(run)
    }

    /// Starts the log-intake workflow for one free-text log of `user`,
    /// with the user's live trackers as the map targets.
    /// Returns the run; the stored entries appear in `tracker_entries`.
    pub async fn submit_log(&self, user: Uuid, req: SubmitLog) -> Result<Started, ServerError> {
        check_text("text", &req.text)?;
        let tz = parse_tz(&req.timezone)?;
        let trackers: Vec<Value> = list_rows::<Tracker>(&self.deps.pool, user, &[], false)
            .await?
            .iter()
            .map(|t| json!({"id": t.id, "goal_id": t.goal_id, "name": t.name, "fields": t.fields}))
            .collect();
        let at = req.at.unwrap_or_else(|| self.now());
        let mut input = Map::new();
        input.insert("log".into(), json!({"text": req.text, "at": at.to_rfc3339(), "timezone": tz.name()}));
        input.insert("trackers".into(), json!(trackers));
        let run = self.start(user, None, LOG_INTAKE, input).await?;
        Ok(Started { goal_id: None, run_id: run.0 })
    }

    /// Starts the calendar-triage workflow for `user`'s unmatched device
    /// events. Returns `Conflict` while another triage of the user is not
    /// done, so a device never stacks runs; the rules arrive by sync.
    pub async fn start_triage(&self, user: Uuid, req: StartTriage) -> Result<Started, ServerError> {
        let tz = parse_tz(&req.timezone)?;
        let input = triage_input(&req, tz)?;
        let open: i64 = db::query_scalar(
            "select count(*) from public.runs where user_id = $1 and workflow = $2 and state not in ('done', 'failed')",
        )
        .bind(user)
        .bind(CALENDAR_TRIAGE)
        .fetch_one(&self.deps.pool)
        .await?;
        if open > 0 {
            return Err(ServerError::Conflict("a calendar triage is still open; answer it in the Inbox first".into()));
        }
        let run = self.start(user, None, CALENDAR_TRIAGE, input).await?;
        Ok(Started { goal_id: None, run_id: run.0 })
    }

    /// Answers human node `answer.node` of `user`'s run `run_id` and queues
    /// it now. Returns `Conflict` when the run does not wait there, the
    /// answer does not fit, or the run moved meanwhile.
    pub async fn answer(&self, user: Uuid, run_id: Uuid, answer: Answer) -> Result<(), ServerError> {
        let row = run_row_for(&self.deps.pool, user, RunId(run_id)).await?;
        let store = PgCheckpoints::new(self.deps.pool.clone(), Arc::new(EventBuffer::default()));
        let cp = store.latest(row.id).await?.ok_or_else(|| ServerError::NotFound(format!("run {run_id}")))?;
        let mut run = GraphRun::from_checkpoint(&cp)?;
        if let Err(e) = run.resume(&answer.node, answer.answer.clone()) {
            let Some(legacy) = legacy_skips(&answer) else { return Err(e.into()) };
            run.resume(&answer.node, legacy)?;
        }
        store.save(run.checkpoint()).await?;
        self.queue.enqueue(row.id, self.deps.clock.now_ms()).await?;
        Ok(())
    }

    /// Reads `user`'s run time limit, the default when none is stored.
    /// Returns the limit.
    /// Errors only when the database does.
    pub async fn run_limit(&self, user: Uuid) -> Result<RunLimit, ServerError> {
        let minutes: Option<i32> = db::query_scalar("select minutes from public.run_limits where user_id = $1").bind(user).fetch_optional(&self.deps.pool).await?;
        Ok(RunLimit { minutes: minutes.and_then(|m| u32::try_from(m).ok()).unwrap_or(DEFAULT_RUN_MINUTES) })
    }

    /// The time limit of a run of `user` on `goal`: the goal's own limit when
    /// it sets one, else the owner's Settings limit. Every run (new goal,
    /// replan, triggers, retry) takes its limit from here.
    async fn limit_for(&self, user: Uuid, goal: Option<Uuid>) -> Result<RunLimit, ServerError> {
        let own = match goal {
            Some(id) => get_row::<Goal>(&self.deps.pool, user, &id.to_string()).await?.and_then(|g| g.run_limit_minutes),
            None => None,
        };
        match own.and_then(|m| u32::try_from(m).ok()) {
            Some(minutes) => {
                assert!((1..=MAX_RUN_MINUTES).contains(&minutes), "goal rows hold a valid limit");
                Ok(RunLimit { minutes })
            }
            None => self.run_limit(user).await,
        }
    }

    /// Stores `user`'s run time limit for runs started or retried after.
    /// Returns the stored limit, or `BadRequest` outside 1 to 120 minutes.
    pub async fn set_run_limit(&self, user: Uuid, limit: RunLimit) -> Result<RunLimit, ServerError> {
        if !(1..=MAX_RUN_MINUTES).contains(&limit.minutes) {
            return Err(ServerError::BadRequest(format!("minutes: 1 to {MAX_RUN_MINUTES}")));
        }
        db::query(
            "insert into public.run_limits (user_id, minutes, updated_at) values ($1, $2, now())
             on conflict (user_id) do update set minutes = excluded.minutes, updated_at = excluded.updated_at",
        )
        .bind(user)
        .bind(i32::try_from(limit.minutes).expect("at most 120"))
        .execute(&self.deps.pool)
        .await?;
        Ok(limit)
    }

    /// Stops `user`'s run `run_id` now: it fails as cancelled and calls no
    /// model until retried. A finished run is left as it is, so stopping
    /// twice is fine. Returns `NotFound` for another user's run.
    pub async fn cancel(&self, user: Uuid, run_id: Uuid) -> Result<(), ServerError> {
        let row = run_row_for(&self.deps.pool, user, RunId(run_id)).await?;
        let store = PgCheckpoints::new(self.deps.pool.clone(), Arc::new(EventBuffer::default()));
        for _ in 0..STOP_TRIES {
            let cp = store.latest(row.id).await?.ok_or_else(|| ServerError::NotFound(format!("run {run_id}")))?;
            let mut run = GraphRun::from_checkpoint(&cp)?;
            if run.stop(Stop::Cancelled).is_err() {
                return Ok(());
            }
            match store.save(run.checkpoint()).await {
                Err(troupekit_core::error::CoreError::Conflict { .. }) => continue,
                saved => return Ok(saved?),
            }
        }
        Err(ServerError::Conflict("the run kept moving; try again".into()))
    }

    /// Continues `user`'s stopped run `run_id` where it stopped, with the
    /// user's current time limit and no active time used, and queues it.
    /// Returns `Conflict` for a run that is not stopped.
    pub async fn retry(&self, user: Uuid, run_id: Uuid) -> Result<(), ServerError> {
        let row = run_row_for(&self.deps.pool, user, RunId(run_id)).await?;
        if let Some(goal) = row.goal_id {
            if goal_deleted(&self.deps.pool, user, goal).await? {
                return Err(ServerError::NotFound(format!("goal {goal}")));
            }
        }
        let store = PgCheckpoints::new(self.deps.pool.clone(), Arc::new(EventBuffer::default()));
        let cp = store.latest(row.id).await?.ok_or_else(|| ServerError::NotFound(format!("run {run_id}")))?;
        let mut run = GraphRun::from_checkpoint(&cp)?;
        run.retry(Some(self.limit_for(user, row.goal_id).await?.ms()))?;
        store.save(run.checkpoint()).await?;
        self.queue.enqueue(row.id, self.deps.clock.now_ms()).await?;
        Ok(())
    }

    /// Reads `user`'s run `run_id`: status, waiting nodes, usage, and the
    /// events after `after_seq` (all when none).
    /// Returns `NotFound` for a missing run or another user's run.
    pub async fn run_view(&self, user: Uuid, run_id: Uuid, after_seq: Option<u64>) -> Result<RunView, ServerError> {
        let row = run_row_for(&self.deps.pool, user, RunId(run_id)).await?;
        let store = PgCheckpoints::new(self.deps.pool.clone(), Arc::new(EventBuffer::default()));
        let cp = store.latest(row.id).await?.ok_or_else(|| ServerError::NotFound(format!("run {run_id}")))?;
        let run = GraphRun::from_checkpoint(&cp)?;
        let waiting = run
            .pending_human()
            .into_iter()
            .map(|h| Waiting {
                node: h.node,
                ask: match h.ask {
                    Ask::Approval => "approval",
                    Ask::Question => "question",
                }
                .into(),
                prompt: h.prompt,
                data: h.data,
            })
            .collect();
        let summary = crate::insights::run_summary(&row.workflow, run.state());
        let (research, answers) = crate::insights::run_found(run.state());
        let events = events_after(&self.deps.pool, row.id, after_seq)
            .await?
            .into_iter()
            .map(|e| serde_json::to_value(e).expect("events serialize"))
            .collect();
        let mut status = serde_json::to_value(run.status()).expect("statuses serialize");
        if let Some(stop) = run.stopped() {
            status["reason"] = serde_json::to_value(stop).expect("stops serialize");
        }
        Ok(RunView {
            run_id,
            goal_id: row.goal_id,
            workflow: row.workflow,
            status,
            waiting,
            usage: serde_json::to_value(run.usage()).expect("usage serializes"),
            events,
            summary,
            research,
            answers,
        })
    }

    /// Lists `user`'s 50 newest runs for the Inbox and chat, optionally only
    /// those in `state` (such as `waiting_human`) and of `goal`. Never
    /// another user's runs. Returns the summaries, newest first.
    pub async fn list_runs(&self, user: Uuid, state: Option<&str>, goal: Option<Uuid>) -> Result<Vec<RunSummary>, ServerError> {
        let rows: Vec<(Uuid, Option<Uuid>, String, String, DateTime<Utc>, DateTime<Utc>, Vec<String>)> = db::query_as(
            "select id, goal_id, workflow, state, created_at, updated_at,
                    array(select k from jsonb_object_keys(personas) k order by k) from public.runs
             where user_id = $1 and ($2::text is null or state = $2) and ($3::uuid is null or goal_id = $3)
             order by created_at desc, id desc limit $4",
        )
        .bind(user)
        .bind(state)
        .bind(goal)
        .bind(RUN_LIST_LIMIT)
        .fetch_all(&self.deps.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(run_id, goal_id, workflow, state, created_at, updated_at, personas)| RunSummary {
                run_id,
                goal_id,
                workflow,
                state,
                created_at,
                updated_at,
                personas,
            })
            .collect())
    }

    /// Stores `req.key` for `user`'s setting `setting_id`, creating the
    /// provider setting row when the server has none and `req.model` is
    /// given. Returns the key info, or `Setup` for an unknown setting.
    pub async fn set_key(&self, user: Uuid, setting_id: Uuid, req: SetKey) -> Result<KeyInfo, ServerError> {
        check_vendor(&req.vendor)?;
        let id = setting_id.to_string();
        let pool = &self.deps.pool;
        let known = get_row::<ProviderSetting>(pool, user, &id).await?.is_some_and(|s| !s.deleted)
            || get_row::<SearchSetting>(pool, user, &id).await?.is_some_and(|s| !s.deleted);
        if !known {
            let Some(model) = req.model.clone() else {
                return Err(ServerError::Setup(format!("unknown_setting: no provider setting {id}; send model to create it")));
            };
            let mut row = ProviderSetting {
                id: id.clone(),
                user_id: user.to_string(),
                label: req.label.clone().unwrap_or_else(|| req.vendor.clone()),
                vendor: req.vendor.clone(),
                model,
                base_url: None,
                is_default: req.is_default.unwrap_or(false),
                updated_at: String::new(),
                deleted: false,
            };
            put_row(pool, &self.deps.hlc, user, &mut row).await?;
        }
        self.deps.vault.set(user, setting_id, &req.vendor, &req.key).await
    }

    /// Tests `user`'s stored key of provider setting `setting_id` with one
    /// 1-token call, charged to the user like any call.
    /// Returns `{ok, error}`, or `Setup` when no key or setting exists.
    pub async fn test_key(&self, user: Uuid, setting_id: Uuid) -> Result<KeyTest, ServerError> {
        let setting = get_row::<ProviderSetting>(&self.deps.pool, user, &setting_id.to_string())
            .await?
            .filter(|s| !s.deleted)
            .ok_or_else(|| ServerError::NotFound(format!("provider setting {setting_id}")))?;
        let writer = Arc::new(UsageWriter {
            pool: self.deps.pool.clone(),
            hlc: self.deps.hlc.clone(),
            clock: self.deps.clock.clone(),
            user,
            goal: None,
            run: None,
        });
        let selector = format!("{}/{}", setting.vendor, setting.model);
        let handle = connect(&self.deps, writer, &setting, &selector).await?;
        Ok(match handle.model.complete(&CompletionRequest::user("Reply with OK.", 1)).await {
            Ok(_) => KeyTest { ok: true, error: None },
            Err(e) => KeyTest { ok: false, error: Some(e.to_string()) },
        })
    }

    /// Processes one claimed `lease` with a run io built for the run's
    /// user: load, step, save with the step's events, then queue.
    /// A missing run fails the job; an io failure retries in one second.
    pub async fn process(&self, lease: &Lease) -> Result<Processed, ServerError> {
        let Some(row) = run_row(&self.deps.pool, lease.run_id).await? else {
            self.queue.fail(lease, "run row is gone").await?;
            return Err(ServerError::NotFound(format!("run {}", lease.run_id)));
        };
        if let Some(goal) = row.goal_id {
            if goal_deleted(&self.deps.pool, row.user_id, goal).await? {
                self.cancel(row.user_id, row.id.0).await?;
                self.queue.fail(lease, "the goal was deleted").await?;
                return Ok(Processed::Lost);
            }
        }
        let events = Arc::new(EventBuffer::default());
        let io = match RunIo::build(self.deps.clone(), row.user_id, row.goal_id, Some(row.id), &row.personas, events.clone()).await {
            Ok(io) => io,
            Err(e) => {
                let _ = self.queue.reschedule(lease, self.deps.clock.now_ms() + STORE_RETRY_MS).await;
                return Err(e);
            }
        };
        Ok(self.runner(events, io).process(lease).await?)
    }

    /// Step mode: claims due jobs and processes each batch concurrently
    /// until nothing is due, `max_jobs` ran, or `budget_ms` passed (checked
    /// before each claim), as Troupekit's `Runner::tick` does.
    pub async fn tick(&self, opts: TickOptions) -> Result<TickSummary, ServerError> {
        let start = self.deps.clock.now_ms();
        let mut report = TickReport::default();
        if let Err(e) = self.scan_if_due().await {
            report.errors.push(format!("trigger scan: {e}"));
        }
        while report.processed < opts.max_jobs && self.deps.clock.now_ms() - start < opts.budget_ms {
            let leases = self.queue.claim_due(opts.max_jobs - report.processed, self.lease_ms).await?;
            if leases.is_empty() {
                break;
            }
            assert!(leases.len() <= opts.max_jobs - report.processed);
            for result in join_all(leases.iter().map(|l| self.process(l))).await {
                report.processed += 1;
                match result {
                    Ok(Processed::Lost) => report.lost += 1,
                    Ok(_) => {}
                    Err(e) => report.errors.push(e.to_string()),
                }
            }
        }
        Ok(report.into())
    }

    /// Worker mode: ticks in a loop, sleeping `idle_ms` after a tick that
    /// processed nothing or failed. Returns once `shutdown` turns true or
    /// its sender is gone, after the tick in progress.
    pub async fn work(&self, opts: WorkOptions, mut shutdown: watch::Receiver<bool>) {
        while !*shutdown.borrow() {
            let idle = self.tick(opts.tick).await.map_or(true, |r| r.processed == 0);
            if !idle || *shutdown.borrow() {
                continue;
            }
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(opts.idle_ms)) => {}
                changed = shutdown.changed() => {
                    if changed.is_err() {
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(title: &str, start: &str, end: &str, all_day: bool, kind: Option<DayKind>) -> BusyBlock {
        BusyBlock {
            id: Uuid::now_v7().to_string(),
            user_id: Uuid::now_v7().to_string(),
            calendar_id: "home".into(),
            event_id: format!("evt-{title}"),
            title: title.into(),
            start_at: start.parse().unwrap(),
            end_at: end.parse().unwrap(),
            all_day,
            group_name: None,
            day_kind: kind,
            updated_at: String::new(),
            deleted: false,
        }
    }

    #[test]
    fn busy_days_are_full_days_away_days_are_dates_and_reminders_change_nothing() {
        let berlin = chrono_tz::Europe::Berlin;
        let from: DateTime<Utc> = "2026-11-26T08:00:00Z".parse().unwrap();
        let until = from + Duration::days(14);
        let blocks = vec![
            block("Conference", "2026-11-26T23:00:00Z", "2026-11-28T23:00:00Z", true, Some(DayKind::Busy)),
            block("Vacation", "2026-11-29T23:00:00Z", "2026-12-01T23:00:00Z", true, Some(DayKind::Away)),
            block("Birthday", "2026-11-27T23:00:00Z", "2026-11-28T23:00:00Z", true, Some(DayKind::Reminder)),
            block("Unsure", "2026-11-27T23:00:00Z", "2026-11-28T23:00:00Z", true, None),
            block("Standup", "2026-11-27T08:00:00Z", "2026-11-27T08:30:00Z", false, None),
            block("Old trip", "2026-11-01T23:00:00Z", "2026-11-25T23:00:00Z", true, Some(DayKind::Away)),
        ];
        let fixed = |title: &str, start: &str, status: ActionStatus, locked: bool| Action {
            id: Uuid::now_v7().to_string(),
            user_id: Uuid::now_v7().to_string(),
            goal_id: Uuid::now_v7().to_string(),
            series_id: None,
            occurrence: None,
            title: title.into(),
            notes: None,
            start_at: start.parse().unwrap(),
            end_at: start.parse::<DateTime<Utc>>().unwrap() + Duration::minutes(60),
            status,
            locked,
            done_at: None,
            updated_at: String::new(),
            deleted: false,
        };
        let actions = vec![
            fixed("Dentist", "2026-11-27T14:00:00Z", ActionStatus::Planned, true),
            fixed("Run", "2026-11-27T16:00:00Z", ActionStatus::Planned, false),
            fixed("Call", "2026-11-27T17:00:00Z", ActionStatus::Done, true),
            fixed("Later", "2026-12-20T17:00:00Z", ActionStatus::Planned, true),
        ];
        let (busy, away) = day_inputs(&blocks, &actions, berlin, from, until);
        assert_eq!(
            busy,
            vec![
                json!({"title": "Conference", "start": "2026-11-27T00:00:00", "minutes": 1440}),
                json!({"title": "Standup", "start": "2026-11-27T09:00:00", "minutes": 30}),
                json!({"title": "Dentist", "start": "2026-11-27T15:00:00", "minutes": 60}),
                json!({"title": "Conference", "start": "2026-11-28T00:00:00", "minutes": 1440}),
            ]
        );
        assert_eq!(away, ["2026-11-30", "2026-12-01"]);
        let mut gone = blocks.clone();
        gone.iter_mut().for_each(|b| b.deleted = true);
        assert_eq!(day_inputs(&gone, &[], berlin, from, until), (vec![], vec![]));
    }
}
