//! Action cards on the server (Q71): which occurrences still need a full card,
//! the recent session history the coach progresses from, and the tools that write
//! cards (`card_slots`, `write_cards`) and apply an owner's chat edit (`edit_card`).

use crate::db::{self, PgPool};
use crate::engine::{Engine, Started};
use crate::error::ServerError;
use crate::rows::{get_row, put_row};
use crate::tools::{local_text, row_id, ToolEnv};
use crate::triggers::{active_goal, claim_window, settle, Source};
use crate::workflows::{CARD_CHAT, FILL_CARDS};
use aroh_domain::rows::Goal;
use serde_json::Map;
use utoipa::ToSchema;
use aroh_domain::card::{Block, Card, Depth, Exercise, Item, SetLog, SetPlan, Step, Unit};
use aroh_domain::recur::DEFAULT_WINDOW_DAYS;
use aroh_domain::rows::{Action, ActionLog, ActionSeries, ActionStatus, Proposal, ProposalSize, ProposalStatus};
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use troupekit_tools::{typed_tool, Tool, ToolContext, ToolError, ToolOutput};
use uuid::Uuid;

/// Most occurrences one card run writes, so one step stays inside its token budget.
pub const MAX_SLOTS: i64 = 21;
/// Past sessions the coach reads to progress the next cards.
pub const HISTORY: i64 = 4;
/// Window key of the one-time fill of every existing goal's next two weeks.
pub const BACKFILL_WINDOW: &str = "fill:backfill";

/// One occurrence that still needs its full card, in the goal's local time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Slot {
    pub action_id: String,
    pub series_id: Option<String>,
    pub title: String,
    pub start: String,
    pub minutes: i64,
    pub notes: Option<String>,
    pub outline: Option<String>,
    pub planned: Vec<String>,
}

/// One past session of the goal as text lines: what was planned and what the owner
/// logged, one exercise per line, so the coach can progress from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Session {
    pub title: String,
    pub start: String,
    pub status: String,
    pub planned: Vec<String>,
    pub logged: Vec<String>,
    pub effort: Option<String>,
    pub note: Option<String>,
}

fn set_text(reps: u32, weight: Option<f64>, unit: Option<Unit>) -> String {
    let unit = match unit {
        Some(Unit::Kg) => " kg",
        Some(Unit::Lb) => " lb",
        None => "",
    };
    weight.map_or(format!("{reps}"), |w| format!("{reps} x {w}{unit}"))
}

/// Renders `card` as one line per exercise or list block, and `sets` (the log of
/// that card) as one line per exercise with each logged set. Used for history.
pub fn card_lines(card: Option<&Card>, sets: &[SetLog]) -> (Vec<String>, Vec<String>) {
    let (mut planned, mut logged) = (Vec::new(), Vec::new());
    for (bi, b) in card.map(|c| c.blocks.as_slice()).unwrap_or_default().iter().enumerate() {
        match b {
            Block::Workout { exercises } => {
                for (ri, e) in exercises.iter().enumerate() {
                    let plan: Vec<String> = e.sets.iter().map(|s| set_text(s.reps, s.weight, e.unit)).collect();
                    planned.push(format!("{}: {}, rest {} s", e.name, plan.join(", "), e.rest_s));
                    let done: Vec<String> = sets
                        .iter()
                        .filter(|s| usize::from(s.block) == bi && usize::from(s.row) == ri && s.done)
                        .map(|s| set_text(s.reps, s.weight, e.unit))
                        .collect();
                    if !done.is_empty() {
                        logged.push(format!("{}: {}", e.name, done.join(", ")));
                    }
                }
            }
            Block::Steps { title, items } => planned.push(format!("{title}: {} steps", items.len())),
            Block::Checklist { title, items } => planned.push(format!("{title}: {} items", items.len())),
        }
    }
    (planned, logged)
}

/// The model's view of `card`: every block with all of `kind`, `title`, `exercises`
/// and `items` (null when unused), the shape `card_from_wire` reads back.
pub fn to_wire(card: &Card) -> Value {
    let blocks: Vec<Value> = card
        .blocks
        .iter()
        .map(|b| match b {
            Block::Workout { exercises } => json!({"kind": "workout", "title": null, "exercises": exercises, "items": null}),
            Block::Steps { title, items } => json!({"kind": "steps", "title": title, "exercises": null, "items": items}),
            Block::Checklist { title, items } => {
                let items: Vec<Value> = items.iter().map(|i| json!({"text": i.text, "minutes": null})).collect();
                json!({"kind": "checklist", "title": title, "exercises": null, "items": items})
            }
        })
        .collect();
    json!({"why": card.why, "blocks": blocks})
}

/// Condition `cards_gate()`: `some` when the `card_work` state key lists slots
/// to write, else `none`, so a goal with nothing to fill skips the model call.
pub fn cards_gate(state: &serde_json::Map<String, Value>, _args: &[Value]) -> Result<String, String> {
    let slots = state.get("card_work").and_then(|w| w.get("slots")).and_then(Value::as_array).ok_or("card_work has no slots")?;
    Ok(if slots.is_empty() { "none" } else { "some" }.into())
}

fn failed(e: impl std::fmt::Display) -> ToolError {
    ToolError::Failed(e.to_string())
}

/// Planned occurrences of `user`'s `goal` starting in the next two weeks whose card is
/// missing or only an outline, in start order, at most [`MAX_SLOTS`].
pub async fn slots(pool: &PgPool, user: Uuid, goal: Uuid, tz: Tz, now: DateTime<Utc>) -> Result<Vec<Slot>, ServerError> {
    let rows: Vec<Value> = db::query_scalar(
        "select to_jsonb(a) - 'server_seq' from public.actions a
         where a.user_id = $1 and a.goal_id = $2 and not a.deleted and a.status = 'planned'
           and a.start_at >= $3 and a.start_at < $4 and (a.card is null or a.card->>'depth' = 'outline')
         order by a.start_at, a.id limit $5",
    )
    .bind(user)
    .bind(goal)
    .bind(now)
    .bind(now + Duration::days(i64::from(DEFAULT_WINDOW_DAYS)))
    .bind(MAX_SLOTS)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|v| {
            let a: Action = crate::rows::parse_row(v)?;
            Ok(Slot {
                start: local_text(a.start_at.with_timezone(&tz).naive_local()),
                minutes: (a.end_at - a.start_at).num_minutes(),
                outline: a.card.as_ref().and_then(|c| c.outline.clone()),
                planned: card_lines(a.card.as_ref(), &[]).0,
                action_id: a.id,
                series_id: a.series_id,
                title: a.title,
                notes: a.notes,
            })
        })
        .collect()
}

/// The last [`HISTORY`] sessions of `user`'s `goal` before `now` that were done or
/// logged, newest first, each with its card and log, for progression.
pub async fn history(pool: &PgPool, user: Uuid, goal: Uuid, tz: Tz, now: DateTime<Utc>) -> Result<Vec<Session>, ServerError> {
    let rows: Vec<(Value, Option<Value>)> = db::query_as(
        "select to_jsonb(a) - 'server_seq', to_jsonb(l) - 'server_seq' from public.actions a
         left join public.action_logs l on l.user_id = a.user_id and l.action_id = a.id and not l.deleted
         where a.user_id = $1 and a.goal_id = $2 and not a.deleted and a.start_at < $3
           and (a.status = 'done' or l.id is not null)
         order by a.start_at desc limit $4",
    )
    .bind(user)
    .bind(goal)
    .bind(now)
    .bind(HISTORY)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(a, l)| {
            let a: Action = crate::rows::parse_row(a)?;
            let log: Option<ActionLog> = l.map(crate::rows::parse_row).transpose()?;
            let (planned, logged) = card_lines(a.card.as_ref(), log.as_ref().map(|l| l.sets.as_slice()).unwrap_or_default());
            let word = |v: Value| v.as_str().map(str::to_string);
            Ok(Session {
                title: a.title,
                start: local_text(a.start_at.with_timezone(&tz).naive_local()),
                status: serde_json::to_value(a.status).ok().and_then(word).unwrap_or_default(),
                planned,
                logged,
                effort: log.as_ref().and_then(|l| l.effort).and_then(|e| serde_json::to_value(e).ok()).and_then(word),
                note: log.and_then(|l| l.note),
            })
        })
        .collect()
}

/// Whether a card run (new goal or fill) of `user`'s `goal` is still unfinished,
/// so a fill or backfill never races the run that writes the same cards.
pub async fn card_run_busy(pool: &PgPool, user: Uuid, goal: Uuid) -> Result<bool, ServerError> {
    Ok(db::query_scalar(
        "select exists(select 1 from public.runs where user_id = $1 and goal_id = $2 and workflow in ('new-goal', 'fill-cards')
           and state in ('queued', 'running', 'checkpointed', 'waiting_human', 'waiting_budget'))",
    )
    .bind(user)
    .bind(goal)
    .fetch_one(pool)
    .await?)
}

/// Converts the model's card (blocks with a `kind` and optional `title`,
/// `exercises` and `items`) into a checked domain card of depth `depth`.
/// Returns `InvalidArgs` naming the problem, so the repair round can fix it.
pub fn card_from_wire(v: &Value, depth: Depth, outline: Option<String>) -> Result<Card, ToolError> {
    let bad = |m: String| ToolError::InvalidArgs(m);
    let why = v.get("why").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let mut blocks = Vec::new();
    for b in v.get("blocks").and_then(Value::as_array).cloned().unwrap_or_default() {
        let kind = b.get("kind").and_then(Value::as_str).unwrap_or("");
        let title = b.get("title").and_then(Value::as_str).unwrap_or("").to_string();
        let items = b.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let text = |i: &Value| i.get("text").and_then(Value::as_str).unwrap_or("").to_string();
        blocks.push(match kind {
            "workout" => {
                let mut exercises = Vec::new();
                for e in b.get("exercises").and_then(Value::as_array).cloned().unwrap_or_default() {
                    let unit: Option<Unit> = match e.get("unit") {
                        None | Some(Value::Null) => None,
                        Some(u) => Some(serde_json::from_value(u.clone()).map_err(|_| bad(format!("unit `{u}` is kg, lb or null")))?),
                    };
                    let sets = e
                        .get("sets")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .map(|s| {
                            let reps = s.get("reps").and_then(Value::as_u64).ok_or_else(|| bad("each set has whole reps".into()))?;
                            Ok(SetPlan { reps: u32::try_from(reps).unwrap_or(u32::MAX), weight: s.get("weight").and_then(Value::as_f64) })
                        })
                        .collect::<Result<Vec<_>, ToolError>>()?;
                    exercises.push(Exercise {
                        name: e.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                        unit,
                        rest_s: e.get("rest_s").and_then(Value::as_u64).and_then(|r| u32::try_from(r).ok()).unwrap_or(0),
                        cue: e.get("cue").and_then(Value::as_str).map(str::to_string).filter(|c| !c.trim().is_empty()),
                        sets,
                    });
                }
                Block::Workout { exercises }
            }
            "steps" => Block::Steps {
                title,
                items: items.iter().map(|i| Step { text: text(i), minutes: i.get("minutes").and_then(Value::as_u64).and_then(|m| u32::try_from(m).ok()) }).collect(),
            },
            "checklist" => Block::Checklist { title, items: items.iter().map(|i| Item { text: text(i) }).collect() },
            other => return Err(bad(format!("block kind `{other}` is workout, steps or checklist"))),
        });
    }
    let card = Card { v: 1, depth, why, blocks, outline };
    card.check().map_err(|e| bad(e.to_string()))?;
    Ok(card)
}

#[derive(Deserialize, JsonSchema)]
struct SlotsArgs {
    goal: crate::tools::GoalInput,
}

#[derive(Deserialize, JsonSchema)]
struct WorkArgs {
    slots: Vec<Slot>,
}

#[derive(Deserialize, JsonSchema)]
struct CardsOut {
    cards: Vec<CardOut>,
    outlines: Vec<OutlineOut>,
}

#[derive(Deserialize, JsonSchema)]
struct CardOut {
    action_id: String,
    card: Value,
}

#[derive(Deserialize, JsonSchema)]
struct OutlineOut {
    series_id: String,
    outline: String,
}

#[derive(Deserialize, JsonSchema)]
struct WriteArgs {
    card_work: WorkArgs,
    cards: CardsOut,
}

/// The owner's card chat request: the action, the scope picked before sending and the text.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EditRequest {
    pub action_id: String,
    pub scope: String,
    pub text: String,
}

#[derive(Deserialize, JsonSchema)]
struct ChangeOut {
    reply: String,
    card: Value,
    minutes: i64,
}

#[derive(Deserialize, JsonSchema)]
struct EditArgs {
    request: EditRequest,
    change: ChangeOut,
}

async fn goal_action(env: &ToolEnv, id: &str) -> Result<Action, ToolError> {
    let goal = env.goal.ok_or_else(|| ToolError::Failed("this run has no goal".into()))?;
    match get_row::<Action>(&env.pool, env.user, id).await.map_err(failed)? {
        Some(a) if a.goal_id == goal.to_string() => Ok(a),
        _ => Err(ToolError::InvalidArgs(format!("action `{id}` is not an action of this goal"))),
    }
}

async fn card_slots(env: &ToolEnv, args: SlotsArgs) -> Result<Value, ToolError> {
    let goal = env.goal.ok_or_else(|| ToolError::Failed("this run has no goal".into()))?;
    let tz: Tz = args.goal.timezone.parse().map_err(|_| ToolError::InvalidArgs(format!("unknown timezone `{}`", args.goal.timezone)))?;
    let now = crate::rows::utc(env.clock.now_ms());
    let slots = slots(&env.pool, env.user, goal, tz, now).await.map_err(failed)?;
    let history = history(&env.pool, env.user, goal, tz, now).await.map_err(failed)?;
    Ok(json!({"slots": slots, "history": history}))
}

/// Writes the full cards the coach wrote for this run's slots and the outline line of
/// each named series. Cards for actions outside the slots are refused; a slot whose card
/// is already full (the owner edited it meanwhile) is left alone. Returns written, kept and missing counts.
async fn write_cards(env: &ToolEnv, args: WriteArgs) -> Result<Value, ToolError> {
    let (mut written, mut kept) = (0, 0);
    for c in &args.cards.cards {
        let Some(slot) = args.card_work.slots.iter().find(|s| s.action_id == c.action_id) else {
            return Err(ToolError::InvalidArgs(format!("action `{}` is not one of the cards to write", c.action_id)));
        };
        let card = card_from_wire(&c.card, Depth::Full, slot.outline.clone())?;
        let mut a = goal_action(env, &c.action_id).await?;
        if a.card.as_ref().is_some_and(|k| k.depth == Depth::Full) || a.status != ActionStatus::Planned {
            kept += 1;
            continue;
        }
        a.card = Some(card);
        put_row(&env.pool, &env.hlc, env.user, &mut a).await.map_err(failed)?;
        written += 1;
    }
    for o in &args.cards.outlines {
        let goal = env.goal.ok_or_else(|| ToolError::Failed("this run has no goal".into()))?;
        let Some(mut s) = get_row::<ActionSeries>(&env.pool, env.user, &o.series_id).await.map_err(failed)?.filter(|s| s.goal_id == goal.to_string()) else {
            return Err(ToolError::InvalidArgs(format!("series `{}` is not a series of this goal", o.series_id)));
        };
        let base = s.card.clone().unwrap_or(Card { v: 1, depth: Depth::Outline, why: String::new(), blocks: vec![], outline: None });
        s.card = Some(Card { depth: Depth::Outline, outline: Some(o.outline.trim().to_string()), ..base });
        s.card.as_ref().expect("just set").check().map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        put_row(&env.pool, &env.hlc, env.user, &mut s).await.map_err(failed)?;
    }
    let missing = args.card_work.slots.len().saturating_sub(written + kept);
    assert!(written + kept <= args.cards.cards.len());
    Ok(json!({"written": written, "kept": kept, "missing": missing}))
}

fn undo_entry(a: &Action) -> Value {
    json!({"table": "actions", "id": a.id, "card": a.card, "end_at": a.end_at})
}

/// Applies the owner's card chat edit at once (Q71): this occurrence, or with scope
/// `future` also the series template and every later planned occurrence without a log.
/// A new length moves the end. Records an applied proposal with the diff and the undo;
/// a replayed call returns that record and changes nothing.
async fn edit_card(env: &ToolEnv, args: EditArgs, ctx: &ToolContext) -> Result<Value, ToolError> {
    let goal = env.goal.ok_or_else(|| ToolError::Failed("this run has no goal".into()))?;
    if !(1..=1440).contains(&args.change.minutes) {
        return Err(ToolError::InvalidArgs("minutes between 1 and 1440".into()));
    }
    let future = match args.request.scope.as_str() {
        "this" => false,
        "future" => true,
        other => return Err(ToolError::InvalidArgs(format!("scope `{other}` is this or future"))),
    };
    let proposal_id = row_id(ctx, "card-edit")?;
    if let Some(done) = get_row::<Proposal>(&env.pool, env.user, &proposal_id).await.map_err(failed)? {
        let changed = done.diff.get("count").and_then(Value::as_u64).unwrap_or(0);
        return Ok(json!({"reply": done.summary, "proposal_id": done.id, "changed": changed}));
    }
    let mut this = goal_action(env, &args.request.action_id).await?;
    let outline = this.card.as_ref().and_then(|c| c.outline.clone());
    let card = card_from_wire(&args.change.card, Depth::Full, outline)?;
    let before = json!({"card": this.card, "minutes": (this.end_at - this.start_at).num_minutes()});
    let mut targets = vec![this.clone()];
    let mut undo = vec![undo_entry(&this)];
    if future {
        if let Some(series_id) = this.series_id.clone() {
            if let Some(mut s) = get_row::<ActionSeries>(&env.pool, env.user, &series_id).await.map_err(failed)? {
                undo.push(json!({"table": "action_series", "id": s.id, "card": s.card, "duration_minutes": s.duration_minutes}));
                s.card = Some(card.as_outline());
                s.duration_minutes = args.change.minutes;
                put_row(&env.pool, &env.hlc, env.user, &mut s).await.map_err(failed)?;
            }
            let later: Vec<Value> = db::query_scalar(
                "select to_jsonb(a) - 'server_seq' from public.actions a
                 where a.user_id = $1 and a.goal_id = $2 and a.series_id::text = $3 and not a.deleted and a.status = 'planned'
                   and a.start_at > $4 and not exists (select 1 from public.action_logs l where l.user_id = a.user_id and l.action_id = a.id and not l.deleted)
                 order by a.start_at",
            )
            .bind(env.user)
            .bind(goal)
            .bind(&series_id)
            .bind(this.start_at)
            .fetch_all(&env.pool)
            .await
            .map_err(failed)?;
            for v in later {
                let a: Action = crate::rows::parse_row(v).map_err(failed)?;
                undo.push(undo_entry(&a));
                targets.push(a);
            }
        }
    }
    for a in targets.iter_mut() {
        a.card = Some(card.clone());
        a.end_at = a.start_at + Duration::minutes(args.change.minutes);
        put_row(&env.pool, &env.hlc, env.user, a).await.map_err(failed)?;
    }
    this.card = Some(card.clone());
    let mut proposal = Proposal {
        id: proposal_id,
        user_id: env.user.to_string(),
        goal_id: goal.to_string(),
        run_id: ctx.run_id.map(|r| r.0.to_string()),
        summary: args.change.reply.chars().take(2000).collect::<String>().trim().to_string(),
        diff: json!({"op": "card", "scope": args.request.scope, "request": args.request.text, "action_id": this.id,
            "before": before, "after": {"card": card, "minutes": args.change.minutes}, "count": targets.len()}),
        size: ProposalSize::Small,
        status: ProposalStatus::Applied,
        undo: Some(json!({"rows": undo})),
        created_at: crate::rows::utc(env.clock.now_ms()),
        updated_at: String::new(),
        deleted: false,
    };
    if proposal.summary.is_empty() {
        proposal.summary = "Changed the card.".into();
    }
    put_row(&env.pool, &env.hlc, env.user, &mut proposal).await.map_err(failed)?;
    Ok(json!({"reply": proposal.summary, "proposal_id": proposal.id, "changed": targets.len()}))
}

fn json_output(v: Value) -> ToolOutput {
    ToolOutput::text(v.to_string())
}

/// The card tools of one run: `card_slots`, `write_cards` and `edit_card`,
/// writing as `env.user` inside the run's goal. Workflow tool nodes call them.
pub fn card_tools(env: ToolEnv) -> Vec<Arc<dyn Tool>> {
    let e1 = env.clone();
    let slots = typed_tool("card_slots", "List the occurrences of the next two weeks that need a full card, and recent sessions.", move |a: SlotsArgs, _ctx: ToolContext| {
        let env = e1.clone();
        async move { card_slots(&env, a).await.map(json_output) }
    });
    let e2 = env.clone();
    let write = typed_tool("write_cards", "Store the coach's full cards and series outlines.", move |a: WriteArgs, _ctx: ToolContext| {
        let env = e2.clone();
        async move { write_cards(&env, a).await.map(json_output) }
    });
    let e3 = env;
    let edit = typed_tool("edit_card", "Apply the owner's card edit now and record it with its undo.", move |a: EditArgs, ctx: ToolContext| {
        let env = e3.clone();
        async move { edit_card(&env, a, &ctx).await.map(json_output) }
    });
    vec![Arc::new(slots), Arc::new(write), Arc::new(edit)]
}

/// Eval `cards_filled(path)`: every card at `path` (a list of `{action_id, card}`)
/// is a full card with a why and blocks that pass the card bounds.
/// Errors name each card to fix, so the repair round rewrites only those.
pub fn cards_filled(output: &Value, args: &[Value]) -> Result<(), String> {
    let path = args.first().and_then(Value::as_str).filter(|_| args.len() == 1).ok_or("takes (path)")?;
    let list = output.pointer(&format!("/{}", path.replace('.', "/"))).and_then(Value::as_array).cloned().unwrap_or_default();
    let mut bad = Vec::new();
    for c in &list {
        let id = c.get("action_id").and_then(Value::as_str).unwrap_or("?");
        let card = c.get("card").cloned().unwrap_or(Value::Null);
        let empty = card.get("blocks").and_then(Value::as_array).is_none_or(Vec::is_empty);
        match card_from_wire(&card, Depth::Full, None) {
            Ok(_) if !empty => {}
            Ok(_) => bad.push(format!("{id}: add the session's blocks (exercises with sets, reps, weight and rest, or steps, or a checklist)")),
            Err(e) => bad.push(format!("{id}: {e}")),
        }
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(format!("fix these cards: {}", bad.join("; ")))
    }
}

/// Body of `POST /v1/actions/{action_id}/chat`: the scope the owner picked
/// before sending (`this` or `future`) and the change in plain words.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CardChat {
    pub scope: String,
    pub text: String,
}

/// The window key of the weekly card fill whose review slot fell on local `date`.
pub fn fill_window(date: chrono::NaiveDate) -> String {
    format!("fill:{date}")
}

impl Engine {
    /// Starts the card coach on `user`'s action `action_id` with `req` (Q71): the
    /// edit applies when the run ends, recorded as an applied proposal with undo.
    /// Returns the run; `NotFound` for an action that is not the owner's.
    pub async fn card_chat(&self, user: Uuid, action_id: Uuid, req: CardChat) -> Result<Started, ServerError> {
        crate::engine::check_text("text", &req.text)?;
        if !matches!(req.scope.as_str(), "this" | "future") {
            return Err(ServerError::BadRequest("scope must be this or future".into()));
        }
        let pool = &self.deps().pool;
        let action = get_row::<Action>(pool, user, &action_id.to_string())
            .await?
            .filter(|a| !a.deleted)
            .ok_or_else(|| ServerError::NotFound(format!("action {action_id}")))?;
        let goal = Uuid::parse_str(&action.goal_id).map_err(|_| ServerError::Db("bad goal id".into()))?;
        let tz = crate::triggers::goal_zone(pool, user, goal).await?;
        let now = crate::rows::utc(self.deps().clock.now_ms());
        let mut input = Map::new();
        input.insert("request".into(), json!({"action_id": action.id, "scope": req.scope, "text": req.text}));
        input.insert(
            "current".into(),
            json!({
                "title": action.title,
                "start": local_text(action.start_at.with_timezone(&tz).naive_local()),
                "minutes": (action.end_at - action.start_at).num_minutes(),
                "card": action.card.as_ref().map(to_wire),
                "history": history(pool, user, goal, tz, now).await?,
            }),
        );
        input.insert("profile".into(), json!(self.profile(user).await?));
        let run = self.start(user, Some(goal), CARD_CHAT, input).await?;
        Ok(Started { goal_id: Some(goal), run_id: run.0 })
    }

    async fn start_fill(&self, user: Uuid, goal: &Goal, window: &str, now: DateTime<Utc>) -> Result<bool, ServerError> {
        let pool = &self.deps().pool;
        let goal_id = Uuid::parse_str(&goal.id).map_err(|_| ServerError::Db("bad goal id".into()))?;
        if card_run_busy(pool, user, goal_id).await? {
            return Ok(false);
        }
        let Some(adaptation) = claim_window(pool, user, Some(goal_id), Source::Review, window, "Fill the next two weeks' cards.", now).await? else {
            return Ok(false);
        };
        let tz = crate::triggers::goal_zone(pool, user, goal_id).await?;
        let mut input = Map::new();
        input.insert("goal".into(), Engine::goal_input(goal, tz, now));
        input.insert("profile".into(), json!(self.profile(user).await?));
        let result = self.start(user, Some(goal_id), FILL_CARDS, input).await;
        settle(pool, adaptation, &result).await?;
        Ok(result.is_ok())
    }

    /// Starts the weekly card fill of `user`'s active `goal` for the review slot on
    /// local `date`, once per slot. Skipped while a card run of the goal is busy.
    /// Returns whether a run started.
    pub(crate) async fn trigger_fill(&self, user: Uuid, goal: &Goal, date: chrono::NaiveDate, now: DateTime<Utc>) -> Result<bool, ServerError> {
        self.start_fill(user, goal, &fill_window(date), now).await
    }

    /// The one-time card backfill (Q71): every active goal with upcoming sessions
    /// lacking a full card that never had it gets
    /// its weekly review (zone of the owner's other reviews, else UTC)
    /// and one fill. Returns how many fills started.
    pub(crate) async fn scan_backfill(&self, now: DateTime<Utc>) -> Result<usize, ServerError> {
        let pool = self.deps().pool.clone();
        let goals: Vec<(Uuid, Uuid)> = db::query_as(
            "select g.user_id, g.id from public.goals g
             where not g.deleted and g.status = 'active'
               and not exists (select 1 from public.adaptations a where a.user_id = g.user_id and a.goal_id = g.id and a.window_key = $1)
               and exists (select 1 from public.actions x where x.user_id = g.user_id and x.goal_id = g.id and not x.deleted
                 and x.status = 'planned' and x.start_at >= $2 and x.start_at < $3 and (x.card is null or x.card->>'depth' = 'outline'))
             order by g.user_id, g.id limit 50",
        )
        .bind(BACKFILL_WINDOW)
        .bind(now)
        .bind(now + Duration::days(i64::from(DEFAULT_WINDOW_DAYS)))
        .fetch_all(&pool)
        .await?;
        let mut started = 0;
        for (user, goal_id) in goals {
            let zone: Option<String> = db::query_scalar(
                "select timezone from public.review_schedules where user_id = $1 order by (goal_id = $2) desc, goal_id limit 1",
            )
            .bind(user)
            .bind(goal_id)
            .fetch_optional(&pool)
            .await?;
            let tz = zone.as_deref().map(crate::triggers::zone_or_utc).unwrap_or(Tz::UTC);
            crate::triggers::ensure_review_row(&pool, user, goal_id, tz, now).await?;
            let Some(goal) = active_goal(&pool, user, goal_id).await? else { continue };
            if self.start_fill(user, &goal, BACKFILL_WINDOW, now).await? {
                started += 1;
            }
        }
        Ok(started)
    }

    /// Fills the next two weeks' cards right after a plan is written (B58): every
    /// finished new-goal or replan run of the last day (or only `run`) whose goal
    /// has upcoming sessions lacking a full card starts one fill, window
    /// `fill:plan:<run id>`, so each plan version fills once. A busy goal is
    /// retried by the next scan. Returns how many fills started.
    pub(crate) async fn fill_new_plans(&self, now: DateTime<Utc>, run: Option<Uuid>) -> Result<usize, ServerError> {
        let pool = self.deps().pool.clone();
        let plans: Vec<(Uuid, Uuid, Uuid)> = db::query_as(
            "select r.user_id, r.goal_id, r.id from public.runs r
             where r.state = 'done' and r.workflow in ('new-goal', 'replan') and r.goal_id is not null
               and r.updated_at > $1 and ($4::uuid is null or r.id = $4)
               and not exists (select 1 from public.adaptations a where a.user_id = r.user_id and a.goal_id = r.goal_id and a.window_key = 'fill:plan:' || r.id::text)
               and exists (select 1 from public.actions x where x.user_id = r.user_id and x.goal_id = r.goal_id and not x.deleted
                 and x.status = 'planned' and x.start_at >= $2 and x.start_at < $3 and (x.card is null or x.card->>'depth' = 'outline'))
             order by r.updated_at, r.id limit 50",
        )
        .bind(now - Duration::days(1))
        .bind(now)
        .bind(now + Duration::days(i64::from(DEFAULT_WINDOW_DAYS)))
        .bind(run)
        .fetch_all(&pool)
        .await?;
        let mut started = 0;
        for (user, goal_id, run_id) in plans {
            let Some(goal) = active_goal(&pool, user, goal_id).await? else { continue };
            if self.start_fill(user, &goal, &plan_fill_window(run_id), now).await? {
                started += 1;
            }
        }
        Ok(started)
    }
}

/// The window key of the card fill right after plan run `run` wrote its actions.
pub fn plan_fill_window(run: Uuid) -> String {
    format!("fill:plan:{run}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire() -> Value {
        json!({"why": "Legs carry your target.", "blocks": [
            {"kind": "workout", "title": null, "exercises": [{"name": "Back squat", "unit": "kg", "rest_s": 120, "cue": "Up 2.5 kg", "sets": [{"reps": 8, "weight": 62.5}, {"reps": 8, "weight": 62.5}]}], "items": null},
            {"kind": "steps", "title": "Recipe", "exercises": null, "items": [{"text": "Cook rice", "minutes": 15}]},
            {"kind": "checklist", "title": "Bring", "exercises": null, "items": [{"text": "Belt", "minutes": null}]}
        ]})
    }

    #[test]
    fn wire_cards_become_checked_domain_cards() {
        let c = card_from_wire(&wire(), Depth::Full, Some("Squat day".into())).unwrap();
        assert_eq!(c.blocks.len(), 3);
        assert_eq!(c.outline.as_deref(), Some("Squat day"));
        let Block::Workout { exercises } = &c.blocks[0] else { panic!("workout first") };
        assert_eq!((exercises[0].unit, exercises[0].sets[0].weight), (Some(Unit::Kg), Some(62.5)));
        let mut bad = wire();
        bad["blocks"][0]["kind"] = json!("dance");
        assert!(card_from_wire(&bad, Depth::Full, None).is_err());
        let mut no_why = wire();
        no_why["why"] = json!("");
        assert!(card_from_wire(&no_why, Depth::Full, None).is_err());
    }

    #[test]
    fn wire_round_trips_and_history_lines_read_well() {
        let c = card_from_wire(&wire(), Depth::Full, None).unwrap();
        assert_eq!(card_from_wire(&to_wire(&c), Depth::Full, None).unwrap(), c);
        let sets = [SetLog { block: 0, row: 0, set: 0, reps: 7, weight: Some(62.5), done: true }];
        let (planned, logged) = card_lines(Some(&c), &sets);
        assert_eq!(planned[0], "Back squat: 8 x 62.5 kg, 8 x 62.5 kg, rest 120 s");
        assert_eq!(logged, vec!["Back squat: 7 x 62.5 kg".to_string()]);
        let state: serde_json::Map<String, Value> = [("card_work".to_string(), json!({"slots": [], "history": []}))].into_iter().collect();
        assert_eq!(cards_gate(&state, &[]).unwrap(), "none");
    }

    #[test]
    fn cards_filled_names_bare_and_broken_cards() {
        let ok = json!({"cards": [{"action_id": "a", "card": wire()}], "outlines": []});
        assert_eq!(cards_filled(&ok, &[json!("cards")]), Ok(()));
        let bare = json!({"cards": [{"action_id": "b", "card": {"why": "x", "blocks": []}}]});
        assert!(cards_filled(&bare, &[json!("cards")]).unwrap_err().contains("b:"));
        let mut broken = wire();
        broken["blocks"][0]["exercises"][0]["sets"] = json!([]);
        let out = json!({"cards": [{"action_id": "c", "card": broken}]});
        assert!(cards_filled(&out, &[json!("cards")]).unwrap_err().contains("c:"));
        assert!(cards_filled(&ok, &[]).is_err());
    }
}
