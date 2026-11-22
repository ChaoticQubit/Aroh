use crate::checkpoints::{events_after, EventBuffer, PgCheckpoints};
use crate::error::ServerError;
use crate::io::{connect, Deps, RunIo};
use crate::ledger::{goal_key, user_key, UsageWriter};
use crate::queue::PgQueue;
use crate::rows::{get_row, insert_run, list_rows, prepare_reads, put_row, run_row, run_row_for, utc, RunRow};
use crate::tools::local_text;
use crate::vault::{check_vendor, KeyInfo};
use crate::triage::{triage_input, StartTriage};
use crate::workflows::{self, CALENDAR_TRIAGE, LOG_INTAKE, NEW_GOAL, REPLAN};
use crate::db;
use aroh_domain::recur::DEFAULT_WINDOW_DAYS;
use aroh_domain::rows::{
    Action, Autonomy, Budget, BudgetScope, BusyBlock, Goal, GoalStatus, ProviderSetting, SearchSetting, Tracker,
};
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
use troupekit_graph::{Ask, GraphRun};
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
/// `timezone` is the owner's IANA zone, used for every local time.
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
/// Approval nodes take a bool, `questions` takes `[{id, answer}]`, and
/// `change` takes a string.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    pub node: String,
    #[schema(value_type = Object)]
    pub answer: Value,
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
}

/// One row of the run list behind the Inbox: ids, workflow, lifecycle
/// state and times. `waiting_human` runs need the owner.
/// Newest first.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct RunSummary {
    pub run_id: Uuid,
    pub goal_id: Option<Uuid>,
    pub workflow: String,
    pub state: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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

fn parse_tz(name: &str) -> Result<Tz, ServerError> {
    name.parse().map_err(|_| ServerError::BadRequest(format!("unknown timezone `{name}`")))
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
}

impl Engine {
    /// Builds the engine over `deps` with a 300 s lease.
    /// One per process; requests and workers share it.
    /// Returns the engine.
    pub fn new(deps: Arc<Deps>) -> Engine {
        let queue = Arc::new(PgQueue::new(deps.pool.clone(), deps.clock.clone()));
        Engine { deps, queue, lease_ms: DEFAULT_LEASE_MS, push: None }
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

    async fn start(&self, user: Uuid, goal: Option<Uuid>, name: &str, input: Map<String, Value>) -> Result<RunId, ServerError> {
        let wf = workflows::workflow(name)?;
        let personas = workflows::snapshot(&self.deps.pool, user, goal, &wf).await?;
        let events = Arc::new(EventBuffer::default());
        let io = RunIo::build(self.deps.clone(), user, goal, None, &personas, events.clone()).await?;
        let limits = self.limits(user, goal).await?;
        let run = GraphRun::new(wf, input, limits, &io).await?;
        let row = RunRow { id: run.run_id(), user_id: user, goal_id: goal, workflow: name.to_string(), personas };
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

    async fn busy(&self, user: Uuid, tz: Tz, from: DateTime<Utc>) -> Result<Vec<Value>, ServerError> {
        let until = from + Duration::days(i64::from(DEFAULT_WINDOW_DAYS));
        let mut blocks: Vec<BusyBlock> = list_rows::<BusyBlock>(&self.deps.pool, user, &[], false)
            .await?
            .into_iter()
            .filter(|b| !b.all_day && b.end_at > from && b.start_at < until)
            .collect();
        blocks.sort_by_key(|b| b.start_at);
        Ok(blocks
            .iter()
            .map(|b| {
                json!({
                    "title": b.title,
                    "start": local_text(b.start_at.with_timezone(&tz).naive_local()),
                    "minutes": (b.end_at - b.start_at).num_minutes().clamp(1, 24 * 60),
                })
            })
            .collect())
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
        let goal = match get_row::<Goal>(&self.deps.pool, user, &id.to_string()).await? {
            Some(g) if !g.deleted => g,
            Some(_) => return Err(ServerError::NotFound(format!("goal {id}"))),
            None => {
                let mut g = Goal {
                    id: id.to_string(),
                    user_id: user.to_string(),
                    category_id: req.category_id.map(|c| c.to_string()),
                    title: req.title.clone(),
                    context: req.context.clone(),
                    deadline: req.deadline.clone(),
                    status: GoalStatus::Draft,
                    autonomy: req.autonomy.unwrap_or(Autonomy::Balanced),
                    created_at: now,
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
        input.insert("busy".into(), json!(self.busy(user, tz, now).await?));
        let template = aroh_plugins::template("fitness").expect("the fitness template ships");
        input.insert("plugin_guide".into(), json!({"template": template, "reference": aroh_plugins::reference()}));
        let run = self.start(user, Some(id), NEW_GOAL, input).await?;
        Ok(Started { goal_id: Some(id), run_id: run.0 })
    }

    /// Starts the replan workflow for `user`'s goal `goal_id` with `req`'s
    /// reason; the goal's autonomy and locked actions decide approval.
    /// Returns the run, or `NotFound` for a goal the user does not have.
    pub async fn start_replan(&self, user: Uuid, goal_id: Uuid, req: StartReplan) -> Result<Started, ServerError> {
        check_text("reason", &req.reason)?;
        let tz = parse_tz(&req.timezone)?;
        let now = self.now();
        prepare_reads(&self.deps.pool, user, now, DEFAULT_WINDOW_DAYS).await?;
        let goal = get_row::<Goal>(&self.deps.pool, user, &goal_id.to_string())
            .await?
            .filter(|g| !g.deleted)
            .ok_or_else(|| ServerError::NotFound(format!("goal {goal_id}")))?;
        let mut input = Map::new();
        input.insert("goal".into(), Self::goal_input(&goal, tz, now));
        let from = now - Duration::days(PAST_DAYS);
        input.insert("calendar".into(), json!(self.calendar(user, tz, Some(&goal.id), from).await?));
        input.insert("busy".into(), json!(self.busy(user, tz, now).await?));
        input.insert("reason".into(), json!(req.reason));
        input.insert("autonomy".into(), serde_json::to_value(goal.autonomy).expect("autonomy serializes"));
        let run = self.start(user, Some(goal_id), REPLAN, input).await?;
        Ok(Started { goal_id: Some(goal_id), run_id: run.0 })
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
        run.resume(&answer.node, answer.answer)?;
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
        let events = events_after(&self.deps.pool, row.id, after_seq)
            .await?
            .into_iter()
            .map(|e| serde_json::to_value(e).expect("events serialize"))
            .collect();
        Ok(RunView {
            run_id,
            goal_id: row.goal_id,
            workflow: row.workflow,
            status: serde_json::to_value(run.status()).expect("statuses serialize"),
            waiting,
            usage: serde_json::to_value(run.usage()).expect("usage serializes"),
            events,
        })
    }

    /// Lists `user`'s 50 newest runs for the Inbox and chat, optionally only
    /// those in `state` (such as `waiting_human`) and of `goal`. Never
    /// another user's runs. Returns the summaries, newest first.
    pub async fn list_runs(&self, user: Uuid, state: Option<&str>, goal: Option<Uuid>) -> Result<Vec<RunSummary>, ServerError> {
        let rows: Vec<(Uuid, Option<Uuid>, String, String, DateTime<Utc>, DateTime<Utc>)> = db::query_as(
            "select id, goal_id, workflow, state, created_at, updated_at from public.runs
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
            .map(|(run_id, goal_id, workflow, state, created_at, updated_at)| RunSummary {
                run_id,
                goal_id,
                workflow,
                state,
                created_at,
                updated_at,
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
