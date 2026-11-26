use crate::checkpoints::EventBuffer;
use crate::cutoff::{ContinueModel, RunBudget};
use crate::error::ServerError;
use crate::evals;
use crate::ledger::{PgLedger, UsageModel, UsageWriter};
use crate::rows::{get_row, list_rows, ServerClock};
use crate::teams::{self, Kind};
use aroh_domain::rows::{Action, ActionSeries, BusyBlock, Goal};
use crate::tools::{aroh_toolkit, ToolEnv};
use crate::vault::Vault;
use crate::workflows;
use aroh_domain::rows::{ProviderSetting, SearchSetting};
use async_trait::async_trait;
use serde_json::{Map, Value};
use crate::db::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use troupekit_agent::budget::UsageLedger;
use troupekit_agent::{AgentError, AgentIo, EvalRegistry, EventSink, ModelHandle, Persona, PersonaLoader};
use troupekit_core::clock::Clock;
use troupekit_core::ids::RunId;
use troupekit_graph::{Conditions, GraphIo};
use troupekit_providers::{Catalog, HttpModel, MeteredModel, Model};
use troupekit_core::tool::WEB_SEARCH;
use troupekit_tools::{
    hosted, typed_tool, web_fetch, BraveSearch, EnvSecrets, HostedTool, HttpLimits, SearchProvider, SearchToolkit,
    SearxngSearch, ToolContext, ToolOutput, ToolRegistry, Toolkit, Transport,
};
use uuid::Uuid;

/// The provider catalog the engine ships with: vendors, models, prices.
/// Loaded once at start with `Catalog::load`.
/// Prices marked unverified pause metered runs (Troupekit rule).
pub const CATALOG: &str = include_str!("../catalog/providers.toml");

const FETCH_MAX_CHARS: usize = 20_000;

/// Claude's server-side web search for one agent run: at most 5 searches,
/// about $0.05 on the owner's key (Q63).
/// Every other vendor calls the local `web_search` stand-in.
const SEARCH: HostedTool = HostedTool::WebSearch { max_uses: 5 };

/// Claude's server-side web fetch for one agent run: at most 5 pages of
/// at most 6,000 tokens each, close to the local `FETCH_MAX_CHARS`.
/// Fetches cost tokens only.
const FETCH: HostedTool = HostedTool::WebFetch { max_uses: 5, max_content_tokens: 6000 };
const BRAVE_BASE_URL: &str = "https://api.search.brave.com";

/// What every run io of the process shares: pool, vault, provider catalog,
/// the model and web transports, the clock, the server HLC and the
/// operator switches. Tests inject fake transports here.
pub struct Deps {
    pub pool: PgPool,
    pub vault: Vault,
    pub catalog: Catalog,
    pub models: Arc<dyn Transport>,
    pub web: Arc<dyn Transport>,
    pub clock: Arc<dyn Clock>,
    pub hlc: Arc<ServerClock>,
    pub allow_custom_base_url: bool,
    pub search_toml: Option<String>,
}

/// Picks the provider setting a persona `selector` means: `vendor/model`
/// takes that vendor's setting (default first), `default` the default
/// setting, any other alias the setting with that label, else the default.
pub fn resolve<'a>(settings: &'a [ProviderSetting], selector: &str) -> Result<(&'a ProviderSetting, String), String> {
    let default = || settings.iter().find(|s| s.is_default).or(if settings.len() == 1 { settings.first() } else { None });
    if let Some((vendor, model)) = selector.split_once('/') {
        let mut of_vendor: Vec<&ProviderSetting> = settings.iter().filter(|s| s.vendor == vendor).collect();
        of_vendor.sort_by_key(|s| !s.is_default);
        let s = of_vendor.first().ok_or_else(|| format!("no provider setting for vendor `{vendor}`"))?;
        return Ok((s, format!("{vendor}/{model}")));
    }
    let s = settings
        .iter()
        .find(|s| s.label == selector)
        .or_else(default)
        .ok_or_else(|| format!("no default provider setting for `{selector}`; pick one in Settings"))?;
    Ok((s, format!("{}/{}", s.vendor, s.model)))
}

/// Connects provider `setting` as `selector` for `user`: decrypts the key
/// for this one handle, connects through the catalog and wraps the model
/// so its usage is written. Returns `Setup` for a missing key or model.
pub async fn connect(
    deps: &Deps,
    writer: Arc<UsageWriter>,
    setting: &ProviderSetting,
    selector: &str,
) -> Result<ModelHandle, ServerError> {
    let setting_id = Uuid::parse_str(&setting.id).map_err(|_| ServerError::Setup("bad setting id".into()))?;
    let key = deps.vault.load(writer.user, setting_id).await?;
    let price = deps.catalog.price(selector).map_err(|e| ServerError::Setup(e.to_string()))?;
    let (vendor, model) = selector.split_once('/').expect("resolved selectors are vendor/model");
    let inner: Box<dyn Model> = match &setting.base_url {
        None => deps.catalog.connect_with(selector, key, deps.models.clone()).map_err(|e| ServerError::Setup(e.to_string()))?,
        Some(_) if !deps.allow_custom_base_url => {
            return Err(ServerError::Setup("custom base URLs work only on a self-hosted engine".into()))
        }
        Some(url) => {
            let spec = deps.catalog.vendor(vendor).ok_or_else(|| ServerError::Setup(format!("unknown vendor `{vendor}`")))?;
            let mut endpoint = spec.endpoint();
            endpoint.base_url = url.clone();
            let http = HttpModel::new(selector, model, endpoint, key, deps.models.clone())
                .map_err(|e| ServerError::Setup(e.to_string()))?;
            Box::new(MeteredModel::new(Box::new(http), price))
        }
    };
    let budget = Arc::new(RunBudget {
        pool: deps.pool.clone(),
        clock: deps.clock.clone(),
        price: price,
        user: writer.user,
        goal: writer.goal,
        run: writer.run,
    });
    let metered: Box<dyn Model> = Box::new(UsageModel::new(inner, vendor, model, writer));
    let model: Arc<dyn Model> = Arc::new(ContinueModel::new(metered, price, budget));
    Ok(ModelHandle { model, price })
}

async fn search_kit(deps: &Deps, user: Uuid) -> Result<Option<Toolkit>, ServerError> {
    let mut settings: Vec<SearchSetting> = list_rows::<SearchSetting>(&deps.pool, user, &[], false).await?;
    settings.retain(|s| s.enabled);
    settings.sort_by_key(|s| (s.priority, s.id.clone()));
    let mut providers: Vec<Arc<dyn SearchProvider>> = Vec::new();
    for s in &settings {
        let base = s.base_url.as_deref();
        let provider: Arc<dyn SearchProvider> = match s.provider.as_str() {
            "brave" => {
                let id = Uuid::parse_str(&s.id).map_err(|_| ServerError::Setup("bad setting id".into()))?;
                let Some(key) = deps.vault.load(user, id).await? else {
                    continue;
                };
                match BraveSearch::new(&s.id, base.unwrap_or(BRAVE_BASE_URL), key, deps.web.clone()) {
                    Ok(b) => Arc::new(b),
                    Err(_) => continue,
                }
            }
            "searxng" => match base.map(|b| SearxngSearch::new(&s.id, b, deps.web.clone())) {
                Some(Ok(p)) => Arc::new(p),
                _ => continue,
            },
            _ => continue,
        };
        providers.push(provider);
    }
    if !providers.is_empty() {
        let kit = SearchToolkit::new(providers).map_err(|e| ServerError::Setup(e.to_string()))?;
        return Ok(Some(kit.into_toolkit()));
    }
    match &deps.search_toml {
        Some(text) => SearchToolkit::from_toml(text, &EnvSecrets, HttpLimits::default())
            .map(|k| Some(k.into_toolkit()))
            .map_err(|e| ServerError::Config(e.to_string())),
        None => Ok(None),
    }
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct SearchArgs {
    #[allow(dead_code)]
    query: String,
}

fn no_search() -> Toolkit {
    let tool = typed_tool(
        "web_search",
        "Search the web. Not configured on this engine: every call returns an error.",
        |_a: SearchArgs, _ctx: ToolContext| async { Ok(ToolOutput::error("web search is not configured; plan from what you know")) },
    );
    Toolkit::new("search").with(Arc::new(tool))
}

/// Registers `web_search` and `web_fetch` as hosted tools (Q63): Claude runs
/// its own server-side search and fetch, capped per agent run, on the
/// owner's key. Other vendors call the stand-ins: `search` (or the stub)
/// and a client fetch through `web`, both fencing what they return.
fn register_web(tools: &mut ToolRegistry, search: Option<Toolkit>, web: Arc<dyn Transport>) -> Result<(), ServerError> {
    let setup = |e: troupekit_tools::ToolError| ServerError::Setup(e.to_string());
    let search = search.unwrap_or_else(no_search);
    let mut kit = Toolkit::new(search.name());
    for tool in search.tools() {
        kit = kit.with(if tool.spec().name == WEB_SEARCH { hosted(tool.clone(), SEARCH) } else { tool.clone() });
    }
    tools.register_toolkit(kit).map_err(setup)?;
    tools.register(hosted(web_fetch(web, FETCH_MAX_CHARS), FETCH)).map_err(setup)
}

/// Router condition `autonomy_gate()` of the replan workflow: `none` when
/// the proposal has no change; `ask` when the goal's autonomy is `ask` or the
/// proposal is big under `balanced`; else `apply` (Q19). Locked actions never
/// reach a proposal (Q53).
pub fn autonomy_gate(state: &Map<String, Value>, _args: &[Value]) -> Result<String, String> {
    let autonomy = state.get("autonomy").and_then(Value::as_str).ok_or("autonomy is not set")?;
    let record = state.get("proposal_record").ok_or("proposal_record is not set")?;
    if record.get("changes").and_then(Value::as_array).is_some_and(Vec::is_empty) {
        return Ok("none".into());
    }
    let size = record.get("size").and_then(Value::as_str).ok_or("proposal has no size")?;
    let ask = autonomy == "ask" || (autonomy == "balanced" && size != "small");
    Ok(if ask { "ask" } else { "apply" }.into())
}

async fn run_state(pool: &PgPool, vault: &Vault, run: RunId, key: &str) -> Result<Value, ServerError> {
    if crate::checkpoints::is_sealed(pool, run).await? {
        let store = crate::checkpoints::PgCheckpoints::new(pool.clone(), vault.clone(), Arc::new(EventBuffer::default()));
        let Some(cp) = troupekit_core::store::CheckpointStore::latest(&store, run).await? else { return Ok(Value::Null) };
        return Ok(cp.state.get("state").and_then(|s| s.get(key)).cloned().unwrap_or(Value::Null));
    }
    let value: Option<Option<Value>> =
        crate::db::query_scalar("select state->'state'->$2 from public.run_checkpoints where run_id = $1 order by step desc limit 1")
            .bind(run.0)
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(value.flatten().unwrap_or(Value::Null))
}

/// Days from `now` in `tz` through goal `g`'s deadline, at least [`evals::EVAL_WINDOW_DAYS`]
/// and at most [`evals::MAX_HORIZON_DAYS`]; the maximum when it has no readable deadline.
fn horizon(g: &Goal, tz: chrono_tz::Tz, now: chrono::DateTime<chrono::Utc>) -> u32 {
    let today = now.with_timezone(&tz).date_naive();
    let deadline = g.deadline.as_deref().and_then(|d| chrono::NaiveDate::parse_from_str(d.get(..10).unwrap_or(d), "%Y-%m-%d").ok());
    let days = deadline.map_or(i64::from(evals::MAX_HORIZON_DAYS), |d| (d - today).num_days() + 1);
    u32::try_from(days.clamp(i64::from(evals::EVAL_WINDOW_DAYS), i64::from(evals::MAX_HORIZON_DAYS))).expect("clamped to u32 range")
}

/// The busy slots `user`'s goal `goal` plans around and the horizon they cover, read from the
/// rows at `now` in `tz` (B89): `day_inputs` over the calendar entries (when the goal uses
/// calendars) and every action, with the occurrences of other goals' series that are not stored
/// yet expanded in memory, through the goal's `horizon`. None when the goal row is gone.
async fn live_busy(pool: &PgPool, user: Uuid, goal: Uuid, tz: chrono_tz::Tz, now: chrono::DateTime<chrono::Utc>) -> Result<Option<(Vec<evals::Slot>, u32)>, ServerError> {
    let Some(g) = get_row::<Goal>(pool, user, &goal.to_string()).await? else { return Ok(None) };
    let days = horizon(&g, tz, now);
    let blocks: Vec<BusyBlock> = if g.use_calendars { list_rows::<BusyBlock>(pool, user, &[], false).await? } else { Vec::new() };
    let mut actions: Vec<Action> = list_rows::<Action>(pool, user, &[], true).await?;
    for series in list_rows::<ActionSeries>(pool, user, &[], false).await?.into_iter().filter(|s| s.goal_id != g.id) {
        let stored: BTreeSet<_> = actions.iter().filter(|a| a.series_id.as_deref() == Some(series.id.as_str())).filter_map(|a| a.occurrence).collect();
        actions.extend(aroh_domain::recur::materialize(&series, now, days, &stored)?);
    }
    let until = now + chrono::Duration::days(i64::from(days));
    let (busy, _) = crate::engine::day_inputs(&blocks, &actions, &g.id, tz, now, until);
    Ok(Some((evals::busy_slots(&Value::Array(busy)).map_err(ServerError::Setup)?, days)))
}

/// Reads what a run plans around: the busy slots and the horizon in days the overlap check
/// expands plans over, from the rows now through the goal's deadline (at most a year) for a
/// planning run of a goal (one whose state has `busy`, B89), else from that state key over the
/// longest horizon;
/// the ids of the `calendar` entries marked locked; and the replan sequence bounds
/// (`calendar`, `ordered`, `daily_minutes`, `now` local to the goal's zone). Built on every
/// job pickup, so a plan after a change request sees the calendar as it is then. All empty
/// for a run without those keys, before its first checkpoint, or before the run exists.
pub async fn run_bounds(
    pool: &PgPool,
    vault: &Vault,
    user: Uuid,
    goal: Option<Uuid>,
    run: Option<RunId>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(Vec<evals::Slot>, BTreeSet<String>, crate::sequence::Bounds, u32), ServerError> {
    let Some(run) = run else { return Ok((Vec::new(), BTreeSet::new(), Default::default(), evals::MAX_HORIZON_DAYS)) };
    let planned = run_state(pool, vault, run, "busy").await?;
    let calendar = run_state(pool, vault, run, "calendar").await?;
    let fixed = calendar
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a.get("locked").and_then(Value::as_bool) == Some(true))
        .filter_map(|a| a.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    let ordered = run_state(pool, vault, run, "ordered").await?.as_bool().unwrap_or(false);
    let cap = run_state(pool, vault, run, "daily_minutes").await?.as_i64();
    let zone = run_state(pool, vault, run, "goal").await?.get("timezone").and_then(Value::as_str).map(crate::triggers::zone_or_utc).unwrap_or(chrono_tz::UTC);
    let sequence = crate::sequence::Bounds::from_calendar(&calendar, ordered, cap, now.with_timezone(&zone).naive_local()).map_err(ServerError::Setup)?;
    let live = match goal.filter(|_| !planned.is_null()) {
        Some(goal) => live_busy(pool, user, goal, zone, now).await?,
        None => None,
    };
    let (busy, days) = match live {
        Some(found) => found,
        None => (evals::busy_slots(&planned).map_err(ServerError::Setup)?, evals::MAX_HORIZON_DAYS),
    };
    Ok((busy, fixed, sequence, days))
}

/// The kind of `user`'s goal `goal` as its team reads it, so the intake
/// eval asks what a coach of that kind needs. General for a run with no
/// goal or a goal row the server lacks.
async fn goal_kind(pool: &PgPool, user: Uuid, goal: Option<Uuid>) -> Result<Kind, ServerError> {
    let Some(id) = goal else { return Ok(Kind::General) };
    let row = get_row::<Goal>(pool, user, &id.to_string()).await?;
    Ok(row.map_or(Kind::General, |g| {
        teams::for_goal(user, g.category_id.as_deref(), &crate::engine::about(&g.title, g.context.as_deref())).kind
    }))
}

/// The io of one run: its user, goal and run id, the user's provider
/// settings, a tool registry closed over the user, the domain evals, the
/// user's ledger, the step's event buffer and the run's persona snapshot.
pub struct RunIo {
    deps: Arc<Deps>,
    writer: Arc<UsageWriter>,
    settings: Vec<ProviderSetting>,
    tools: ToolRegistry,
    evals: EvalRegistry,
    ledger: PgLedger,
    events: Arc<EventBuffer>,
    personas: PersonaLoader,
    conditions: Conditions,
}

impl RunIo {
    /// Builds the io for `user`'s run `run` (none before the run exists)
    /// of `goal`, with persona snapshot `personas` and event buffer
    /// `events`. Reads the user's provider and search settings now.
    pub async fn build(
        deps: Arc<Deps>,
        user: Uuid,
        goal: Option<Uuid>,
        run: Option<RunId>,
        personas: &BTreeMap<String, String>,
        events: Arc<EventBuffer>,
    ) -> Result<RunIo, ServerError> {
        let settings = list_rows::<ProviderSetting>(&deps.pool, user, &[], false).await?;
        let mut tools = ToolRegistry::new();
        let sealed = match run {
            Some(r) => crate::checkpoints::is_sealed(&deps.pool, r).await?,
            None => false,
        };
        let diary_key = if sealed { deps.vault.diary_get(user).await?.map(Arc::new) } else { None };
        let env = ToolEnv { pool: deps.pool.clone(), hlc: deps.hlc.clone(), clock: deps.clock.clone(), user, goal, sealed, diary_key };
        tools.register_toolkit(aroh_toolkit(env.clone())).map_err(|e| ServerError::Setup(e.to_string()))?;
        let env_cards = env.clone();
        tools.register_toolkit(crate::triage::triage_toolkit(env)).map_err(|e| ServerError::Setup(e.to_string()))?;
        register_web(&mut tools, search_kit(&deps, user).await?, deps.web.clone())?;
        let (busy, fixed, sequence, days) = run_bounds(&deps.pool, &deps.vault, user, goal, run, crate::rows::utc(deps.clock.now_ms())).await?;
        let kind = goal_kind(&deps.pool, user, goal).await?;
        let mut conditions = Conditions::new();
        conditions.register("autonomy_gate", Arc::new(autonomy_gate)).map_err(|e| ServerError::Setup(e.to_string()))?;
        conditions.register("limits_gate", Arc::new(crate::limits::limits_gate)).map_err(|e| ServerError::Setup(e.to_string()))?;
        conditions.register("cards_gate", Arc::new(crate::cards::cards_gate)).map_err(|e| ServerError::Setup(e.to_string()))?;
        tools.register(crate::limits::tool()).map_err(|e| ServerError::Setup(e.to_string()))?;
        conditions.register("overlap_gate", Arc::new(crate::overlaps::overlap_gate)).map_err(|e| ServerError::Setup(e.to_string()))?;
        tools.register(crate::overlaps::tool(busy.clone(), days)).map_err(|e| ServerError::Setup(e.to_string()))?;
        for t in crate::cards::card_tools(env_cards) {
            tools.register(t).map_err(|e| ServerError::Setup(e.to_string()))?;
        }
        let writer = Arc::new(UsageWriter {
            pool: deps.pool.clone(),
            hlc: deps.hlc.clone(),
            clock: deps.clock.clone(),
            user,
            goal,
            run,
        });
        Ok(RunIo {
            ledger: PgLedger::new(deps.pool.clone(), user, deps.clock.clone()),
            writer,
            settings,
            tools,
            evals: evals::registry_for(busy, fixed, kind, sequence, days),
            events,
            personas: workflows::loader(personas),
            conditions,
            deps,
        })
    }

    /// Resolves `selector` and connects it, for the run's models and for
    /// key tests. Returns `Setup` naming what is missing.
    /// The key stays inside the returned handle.
    pub async fn connect(&self, selector: &str) -> Result<ModelHandle, ServerError> {
        let (setting, full) = resolve(&self.settings, selector).map_err(ServerError::Setup)?;
        connect(&self.deps, self.writer.clone(), setting, &full).await
    }
}

#[async_trait]
impl AgentIo for RunIo {
    async fn model(&self, selector: &str) -> Result<ModelHandle, AgentError> {
        self.connect(selector).await.map_err(|e| AgentError::Setup(e.to_string()))
    }

    fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    fn evals(&self) -> &EvalRegistry {
        &self.evals
    }

    fn ledger(&self) -> &dyn UsageLedger {
        &self.ledger
    }

    fn events(&self) -> &dyn EventSink {
        self.events.as_ref()
    }

    fn clock(&self) -> &dyn Clock {
        self.deps.clock.as_ref()
    }
}

#[async_trait]
impl GraphIo for RunIo {
    fn agent(&self) -> &dyn AgentIo {
        self
    }

    async fn persona(&self, name: &str) -> Result<Persona, AgentError> {
        self.personas.load(name).await
    }

    fn conditions(&self) -> &Conditions {
        &self.conditions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn setting(id: u32, label: &str, vendor: &str, default: bool) -> ProviderSetting {
        ProviderSetting {
            id: format!("0190f2a0-0000-7000-8000-{id:012x}"),
            user_id: "0190f2a0-0000-7000-8000-00000000000a".into(),
            label: label.into(),
            vendor: vendor.into(),
            model: format!("{vendor}-model"),
            base_url: None,
            is_default: default,
            updated_at: aroh_domain::seed::SEED_STAMP.into(),
            deleted: false,
        }
    }

    #[test]
    fn aliases_resolve_to_settings() {
        let s = vec![setting(1, "fast", "openai", false), setting(2, "main", "anthropic", true)];
        assert_eq!(resolve(&s, "default").unwrap().1, "anthropic/anthropic-model");
        assert_eq!(resolve(&s, "fast").unwrap().1, "openai/openai-model");
        assert_eq!(resolve(&s, "unknown-role").unwrap().1, "anthropic/anthropic-model");
        assert_eq!(resolve(&s, "openai/gpt-x").unwrap(), (&s[0], "openai/gpt-x".to_string()));
        assert!(resolve(&s, "gemini/x").is_err());
        assert!(resolve(&[], "default").is_err());
        let only = vec![setting(3, "solo", "anthropic", false)];
        assert_eq!(resolve(&only, "default").unwrap().0.label, "solo");
    }

    #[test]
    fn web_tools_are_hosted_with_local_stand_ins() {
        let web: Arc<dyn Transport> =
            crate::fake::FnTransport::new(|_| crate::fake::Reply { status: 404, content_type: "text/plain", body: String::new() });
        let mut tools = ToolRegistry::new();
        register_web(&mut tools, None, web).unwrap();
        let specs = tools.specs();
        let hosted = |name: &str| specs.iter().find(|s| s.name == name).and_then(|s| s.hosted);
        assert_eq!(hosted("web_search"), Some(HostedTool::WebSearch { max_uses: 5 }));
        assert_eq!(hosted("web_fetch"), Some(HostedTool::WebFetch { max_uses: 5, max_content_tokens: 6000 }));
    }

    #[test]
    fn autonomy_gate_routes() {
        let state = |autonomy: &str, size: &str| {
            let v = json!({"autonomy": autonomy, "proposal_record": {"size": size, "changes": [{"op": "move"}]}});
            v.as_object().unwrap().clone()
        };
        assert_eq!(autonomy_gate(&state("auto", "big"), &[]).unwrap(), "apply");
        assert_eq!(autonomy_gate(&state("balanced", "small"), &[]).unwrap(), "apply");
        assert_eq!(autonomy_gate(&state("balanced", "big"), &[]).unwrap(), "ask");
        assert_eq!(autonomy_gate(&state("ask", "small"), &[]).unwrap(), "ask");
        assert!(autonomy_gate(&Map::new(), &[]).is_err());
        let empty = json!({"autonomy": "ask", "proposal_record": {"size": "small", "changes": []}});
        assert_eq!(autonomy_gate(empty.as_object().unwrap(), &[]).unwrap(), "none");
    }
}
