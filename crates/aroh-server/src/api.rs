use crate::auth::{Service, UserId, Verifier};
use crate::engine::{Answer, Engine, GivenAnswer, KeyTest, RunAgain, RunLimit, RunSummary, RunView, SetKey, StartGoal, StartReplan, Started, SubmitLog, TickSummary, Waiting};
use crate::error::{ErrorBody, ErrorDetail, ServerError};
use crate::insights::{self, Insight};
use crate::personas::{EditPersona, Origin, PersonaView};
use crate::plugins::{PluginView, SavePlugin, Writer};
use crate::teams::Kind;
use crate::push::{ApnsEnv, DeviceInfo, Platform, PushSummary, RegisterDevice};
use crate::triage::{StartTriage, TriageEvent};
use crate::triggers::{ReviewSchedule, SetReview, SetTimezone, TimezoneSet};
use crate::vault::KeyInfo;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRef, Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use troupekit_host::TickOptions;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{IntoParams, Modify, OpenApi, ToSchema};
use uuid::Uuid;

/// What every route handler shares: the engine and the token verifier.
/// Cloned per request; both are behind `Arc`.
/// The verifier is reachable through `FromRef` for the auth extractors.
#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<Engine>,
    pub verifier: Arc<Verifier>,
}

impl FromRef<AppState> for Arc<Verifier> {
    fn from_ref(state: &AppState) -> Self {
        state.verifier.clone()
    }
}

fn body<T>(json: Result<Json<T>, JsonRejection>, echo: bool) -> Result<T, ServerError> {
    json.map(|Json(v)| v).map_err(|e| {
        let detail = if echo { e.body_text() } else { "invalid JSON body".to_string() };
        ServerError::BadRequest(detail)
    })
}

/// Query of `GET /v1/runs/{run_id}`: only events with a larger `seq`.
/// Omit it to read every event of the run.
/// Clients pass the last `seq` they saw.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EventsQuery {
    pub after_seq: Option<u64>,
}

/// Query of `GET /v1/runs`: only runs in this lifecycle `state` (such as
/// `waiting_human` for the Inbox) and only runs of `goal_id` (a goal's
/// chat). Omit either to drop that filter.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RunsQuery {
    pub state: Option<String>,
    pub goal_id: Option<Uuid>,
}

/// Query of `GET /v1/insights`: only insights of `goal_id`.
/// Omit it for every goal and the edit insights.
/// Newest first, at most 30.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct InsightsQuery {
    pub goal_id: Option<Uuid>,
}

/// Answer of `POST /v1/runs/{run_id}/answers`: the run that was queued.
/// Poll the run for its next step.
/// Answered with 202.
#[derive(Debug, Serialize, ToSchema)]
pub struct Accepted {
    pub run_id: Uuid,
}

/// Answer of `GET /healthz`: the process is up.
/// Unauthenticated; carries nothing else.
/// For smoke checks only.
#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    pub ok: bool,
}

#[utoipa::path(get, path = "/healthz", responses((status = 200, body = Health)))]
async fn health() -> Json<Health> {
    Json(Health { ok: true })
}

#[utoipa::path(get, path = "/v1/keys", tag = "keys", security(("bearer" = [])),
    responses((status = 200, description = "Key status per setting: vendor and last 4 only", body = [KeyInfo]), (status = 401, body = ErrorBody)))]
async fn list_keys(State(s): State<AppState>, UserId(user): UserId) -> Result<Json<Vec<KeyInfo>>, ServerError> {
    Ok(Json(s.engine.deps().vault.list(user).await?))
}

#[utoipa::path(put, path = "/v1/keys/{setting_id}", tag = "keys", security(("bearer" = [])),
    params(("setting_id" = Uuid, Path, description = "provider_settings or search_settings row id")),
    request_body = SetKey,
    responses((status = 200, body = KeyInfo), (status = 400, body = ErrorBody), (status = 422, description = "unknown setting and no model", body = ErrorBody)))]
async fn set_key(
    State(s): State<AppState>,
    UserId(user): UserId,
    Path(setting_id): Path<Uuid>,
    json: Result<Json<SetKey>, JsonRejection>,
) -> Result<Json<KeyInfo>, ServerError> {
    let req = body(json, false)?;
    Ok(Json(s.engine.set_key(user, setting_id, req).await?))
}

#[utoipa::path(delete, path = "/v1/keys/{setting_id}", tag = "keys", security(("bearer" = [])),
    params(("setting_id" = Uuid, Path)),
    responses((status = 204, description = "Deleted"), (status = 404, body = ErrorBody)))]
async fn delete_key(State(s): State<AppState>, UserId(user): UserId, Path(setting_id): Path<Uuid>) -> Result<StatusCode, ServerError> {
    if s.engine.deps().vault.delete(user, setting_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ServerError::NotFound(format!("key {setting_id}")))
    }
}

#[utoipa::path(post, path = "/v1/keys/{setting_id}/test", tag = "keys", security(("bearer" = [])),
    params(("setting_id" = Uuid, Path, description = "provider_settings row id")),
    responses((status = 200, description = "One 1-token call with the stored key", body = KeyTest), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
async fn test_key(State(s): State<AppState>, UserId(user): UserId, Path(setting_id): Path<Uuid>) -> Result<Json<KeyTest>, ServerError> {
    Ok(Json(s.engine.test_key(user, setting_id).await?))
}

#[utoipa::path(post, path = "/v1/goals", tag = "runs", security(("bearer" = [])),
    request_body = StartGoal,
    responses((status = 202, description = "New-goal run queued", body = Started), (status = 400, body = ErrorBody), (status = 422, description = "no usable provider key", body = ErrorBody)))]
async fn start_goal(
    State(s): State<AppState>,
    UserId(user): UserId,
    json: Result<Json<StartGoal>, JsonRejection>,
) -> Result<(StatusCode, Json<Started>), ServerError> {
    let req = body(json, true)?;
    Ok((StatusCode::ACCEPTED, Json(s.engine.start_goal(user, req).await?)))
}

#[utoipa::path(post, path = "/v1/goals/{goal_id}/replan", tag = "runs", security(("bearer" = [])),
    params(("goal_id" = Uuid, Path)),
    request_body = StartReplan,
    responses((status = 202, description = "Replan run queued", body = Started), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
async fn start_replan(
    State(s): State<AppState>,
    UserId(user): UserId,
    Path(goal_id): Path<Uuid>,
    json: Result<Json<StartReplan>, JsonRejection>,
) -> Result<(StatusCode, Json<Started>), ServerError> {
    let req = body(json, true)?;
    Ok((StatusCode::ACCEPTED, Json(s.engine.start_replan(user, goal_id, req).await?)))
}

#[utoipa::path(post, path = "/v1/goals/{goal_id}/run-again", tag = "runs", security(("bearer" = [])),
    params(("goal_id" = Uuid, Path)),
    request_body = RunAgain,
    responses((status = 202, description = "Run again queued: fresh research and plan with the saved answers", body = Started), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
async fn run_again(
    State(s): State<AppState>,
    UserId(user): UserId,
    Path(goal_id): Path<Uuid>,
    json: Result<Json<RunAgain>, JsonRejection>,
) -> Result<(StatusCode, Json<Started>), ServerError> {
    let req = body(json, true)?;
    Ok((StatusCode::ACCEPTED, Json(s.engine.run_again(user, goal_id, req).await?)))
}

#[utoipa::path(post, path = "/v1/logs", tag = "runs", security(("bearer" = [])),
    request_body = SubmitLog,
    responses((status = 202, description = "Log-intake run queued", body = Started), (status = 400, body = ErrorBody), (status = 422, body = ErrorBody)))]
async fn submit_log(
    State(s): State<AppState>,
    UserId(user): UserId,
    json: Result<Json<SubmitLog>, JsonRejection>,
) -> Result<(StatusCode, Json<Started>), ServerError> {
    let req = body(json, true)?;
    Ok((StatusCode::ACCEPTED, Json(s.engine.submit_log(user, req).await?)))
}

#[utoipa::path(post, path = "/v1/calendar/triage", tag = "runs", security(("bearer" = [])),
    request_body = StartTriage,
    responses((status = 202, description = "Calendar-triage run queued", body = Started), (status = 400, body = ErrorBody), (status = 409, description = "another triage is still open", body = ErrorBody), (status = 422, body = ErrorBody)))]
async fn start_triage(
    State(s): State<AppState>,
    UserId(user): UserId,
    json: Result<Json<StartTriage>, JsonRejection>,
) -> Result<(StatusCode, Json<Started>), ServerError> {
    let req = body(json, true)?;
    Ok((StatusCode::ACCEPTED, Json(s.engine.start_triage(user, req).await?)))
}

#[utoipa::path(get, path = "/v1/runs", tag = "runs", security(("bearer" = [])),
    params(RunsQuery),
    responses((status = 200, description = "The 50 newest runs, for the Inbox", body = [RunSummary])))]
async fn list_runs(State(s): State<AppState>, UserId(user): UserId, Query(q): Query<RunsQuery>) -> Result<Json<Vec<RunSummary>>, ServerError> {
    Ok(Json(s.engine.list_runs(user, q.state.as_deref(), q.goal_id).await?))
}

#[utoipa::path(get, path = "/v1/runs/{run_id}", tag = "runs", security(("bearer" = [])),
    params(("run_id" = Uuid, Path), EventsQuery),
    responses((status = 200, body = RunView), (status = 404, body = ErrorBody)))]
async fn get_run(
    State(s): State<AppState>,
    UserId(user): UserId,
    Path(run_id): Path<Uuid>,
    Query(q): Query<EventsQuery>,
) -> Result<Json<RunView>, ServerError> {
    Ok(Json(s.engine.run_view(user, run_id, q.after_seq).await?))
}

#[utoipa::path(post, path = "/v1/runs/{run_id}/answers", tag = "runs", security(("bearer" = [])),
    params(("run_id" = Uuid, Path)),
    request_body = Answer,
    responses((status = 202, body = Accepted), (status = 404, body = ErrorBody), (status = 409, description = "not waiting there, answer does not fit, or the run moved", body = ErrorBody)))]
async fn answer(
    State(s): State<AppState>,
    UserId(user): UserId,
    Path(run_id): Path<Uuid>,
    json: Result<Json<Answer>, JsonRejection>,
) -> Result<(StatusCode, Json<Accepted>), ServerError> {
    let req = body(json, true)?;
    s.engine.answer(user, run_id, req).await?;
    Ok((StatusCode::ACCEPTED, Json(Accepted { run_id })))
}

#[utoipa::path(post, path = "/v1/runs/{run_id}/cancel", tag = "runs", security(("bearer" = [])),
    params(("run_id" = Uuid, Path)),
    responses((status = 202, description = "Stopped, or already finished", body = Accepted), (status = 404, body = ErrorBody)))]
async fn cancel_run(State(s): State<AppState>, UserId(user): UserId, Path(run_id): Path<Uuid>) -> Result<(StatusCode, Json<Accepted>), ServerError> {
    s.engine.cancel(user, run_id).await?;
    Ok((StatusCode::ACCEPTED, Json(Accepted { run_id })))
}

#[utoipa::path(post, path = "/v1/runs/{run_id}/retry", tag = "runs", security(("bearer" = [])),
    params(("run_id" = Uuid, Path)),
    responses((status = 202, body = Accepted), (status = 404, body = ErrorBody), (status = 409, description = "the run is not stopped", body = ErrorBody)))]
async fn retry_run(State(s): State<AppState>, UserId(user): UserId, Path(run_id): Path<Uuid>) -> Result<(StatusCode, Json<Accepted>), ServerError> {
    s.engine.retry(user, run_id).await?;
    Ok((StatusCode::ACCEPTED, Json(Accepted { run_id })))
}

#[utoipa::path(get, path = "/v1/settings/run-limit", tag = "runs", security(("bearer" = [])),
    responses((status = 200, body = RunLimit)))]
async fn get_run_limit(State(s): State<AppState>, UserId(user): UserId) -> Result<Json<RunLimit>, ServerError> {
    Ok(Json(s.engine.run_limit(user).await?))
}

#[utoipa::path(put, path = "/v1/settings/run-limit", tag = "runs", security(("bearer" = [])),
    request_body = RunLimit,
    responses((status = 200, body = RunLimit), (status = 400, description = "minutes outside 1 to 120", body = ErrorBody)))]
async fn set_run_limit(State(s): State<AppState>, UserId(user): UserId, json: Result<Json<RunLimit>, JsonRejection>) -> Result<Json<RunLimit>, ServerError> {
    let req = body(json, true)?;
    Ok(Json(s.engine.set_run_limit(user, req).await?))
}

#[utoipa::path(get, path = "/v1/insights", tag = "insights", security(("bearer" = [])),
    params(InsightsQuery),
    responses((status = 200, description = "What the engine noticed and did without being asked, newest first", body = [Insight])))]
async fn list_insights(State(s): State<AppState>, UserId(user): UserId, Query(q): Query<InsightsQuery>) -> Result<Json<Vec<Insight>>, ServerError> {
    Ok(Json(insights::list(&s.engine.deps().pool, user, q.goal_id).await?))
}

#[utoipa::path(get, path = "/v1/goals/{goal_id}/review", tag = "insights", security(("bearer" = [])),
    params(("goal_id" = Uuid, Path)),
    responses((status = 200, description = "The goal's weekly review", body = ReviewSchedule), (status = 404, body = ErrorBody)))]
async fn get_review(State(s): State<AppState>, UserId(user): UserId, Path(goal_id): Path<Uuid>) -> Result<Json<ReviewSchedule>, ServerError> {
    Ok(Json(s.engine.review(user, goal_id).await?))
}

#[utoipa::path(put, path = "/v1/goals/{goal_id}/review", tag = "insights", security(("bearer" = [])),
    params(("goal_id" = Uuid, Path)),
    request_body = SetReview,
    responses((status = 200, body = ReviewSchedule), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn set_review(
    State(s): State<AppState>,
    UserId(user): UserId,
    Path(goal_id): Path<Uuid>,
    json: Result<Json<SetReview>, JsonRejection>,
) -> Result<Json<ReviewSchedule>, ServerError> {
    let req = body(json, true)?;
    Ok(Json(s.engine.set_review(user, goal_id, req).await?))
}

#[utoipa::path(put, path = "/v1/timezone", tag = "insights", security(("bearer" = [])),
    request_body = SetTimezone,
    responses((status = 200, description = "Every weekly review of the owner now runs in the zone", body = TimezoneSet), (status = 400, body = ErrorBody)))]
async fn set_timezone(State(s): State<AppState>, UserId(user): UserId, json: Result<Json<SetTimezone>, JsonRejection>) -> Result<Json<TimezoneSet>, ServerError> {
    let req = body(json, true)?;
    Ok(Json(TimezoneSet { reviews: s.engine.set_timezone(user, &req.timezone).await? }))
}

fn writer(s: &AppState, user: Uuid) -> Writer<'_> {
    let deps = s.engine.deps();
    Writer { pool: &deps.pool, hlc: &deps.hlc, user }
}

#[utoipa::path(get, path = "/v1/goals/{goal_id}/plugins", tag = "plugins", security(("bearer" = [])),
    params(("goal_id" = Uuid, Path)),
    responses((status = 200, description = "The goal's plugin versions, newest first", body = [PluginView]), (status = 404, body = ErrorBody)))]
async fn list_plugins(State(s): State<AppState>, UserId(user): UserId, Path(goal_id): Path<Uuid>) -> Result<Json<Vec<PluginView>>, ServerError> {
    Ok(Json(writer(&s, user).list(goal_id).await?.iter().map(PluginView::from).collect()))
}

#[utoipa::path(put, path = "/v1/goals/{goal_id}/plugins", tag = "plugins", security(("bearer" = [])),
    params(("goal_id" = Uuid, Path)),
    request_body = SavePlugin,
    responses((status = 201, description = "Stored as the goal's next version", body = PluginView), (status = 404, body = ErrorBody), (status = 422, description = "invalid bundle: the error names the file and field", body = ErrorBody)))]
async fn save_plugin(
    State(s): State<AppState>,
    UserId(user): UserId,
    Path(goal_id): Path<Uuid>,
    json: Result<Json<SavePlugin>, JsonRejection>,
) -> Result<(StatusCode, Json<PluginView>), ServerError> {
    let req = body(json, true)?;
    let row = writer(&s, user).store(goal_id, &req.bundle_toml, Uuid::now_v7().to_string(), req.enable.unwrap_or(true)).await?;
    Ok((StatusCode::CREATED, Json(PluginView::from(&row))))
}

#[utoipa::path(get, path = "/v1/plugins/{plugin_id}", tag = "plugins", security(("bearer" = [])),
    params(("plugin_id" = Uuid, Path)),
    responses((status = 200, body = PluginView), (status = 404, body = ErrorBody)))]
async fn get_plugin(State(s): State<AppState>, UserId(user): UserId, Path(plugin_id): Path<Uuid>) -> Result<Json<PluginView>, ServerError> {
    Ok(Json(PluginView::from(&writer(&s, user).get(&plugin_id.to_string()).await?)))
}

#[utoipa::path(post, path = "/v1/plugins/{plugin_id}/enable", tag = "plugins", security(("bearer" = [])),
    params(("plugin_id" = Uuid, Path)),
    responses((status = 200, description = "Now the only enabled version of its goal", body = PluginView), (status = 404, body = ErrorBody)))]
async fn enable_plugin(State(s): State<AppState>, UserId(user): UserId, Path(plugin_id): Path<Uuid>) -> Result<Json<PluginView>, ServerError> {
    Ok(Json(PluginView::from(&writer(&s, user).enable(&plugin_id.to_string()).await?)))
}

#[utoipa::path(get, path = "/v1/personas", tag = "personas", security(("bearer" = [])),
    responses((status = 200, description = "Every built-in persona, replaced by the owner's library copy when there is one", body = [PersonaView])))]
async fn list_personas(State(s): State<AppState>, UserId(user): UserId) -> Result<Json<Vec<PersonaView>>, ServerError> {
    Ok(Json(writer(&s, user).list_personas().await?))
}

#[utoipa::path(post, path = "/v1/personas/{name}/copy", tag = "personas", security(("bearer" = [])),
    params(("name" = String, Path, description = "Built-in persona name")),
    responses((status = 201, description = "Copied into the owner's library", body = PersonaView), (status = 404, body = ErrorBody), (status = 409, description = "already in the library", body = ErrorBody)))]
async fn copy_persona(State(s): State<AppState>, UserId(user): UserId, Path(name): Path<String>) -> Result<(StatusCode, Json<PersonaView>), ServerError> {
    Ok((StatusCode::CREATED, Json(writer(&s, user).copy_persona(&name).await?)))
}

#[utoipa::path(put, path = "/v1/personas/{name}", tag = "personas", security(("bearer" = [])),
    params(("name" = String, Path, description = "Library persona name")),
    request_body = EditPersona,
    responses((status = 200, description = "The edited library copy; later runs use it", body = PersonaView), (status = 400, description = "set toml or the fields, not both or neither", body = ErrorBody), (status = 404, description = "not in the library: copy it first", body = ErrorBody), (status = 422, description = "invalid persona: the error names the file and field", body = ErrorBody)))]
async fn edit_persona(
    State(s): State<AppState>,
    UserId(user): UserId,
    Path(name): Path<String>,
    json: Result<Json<EditPersona>, JsonRejection>,
) -> Result<Json<PersonaView>, ServerError> {
    let req = body(json, true)?;
    Ok(Json(writer(&s, user).edit_persona(&name, &req).await?))
}

#[utoipa::path(post, path = "/v1/tick", tag = "engine", security(("bearer" = [])),
    responses((status = 200, description = "Step mode: due jobs processed (service role only)", body = TickSummary), (status = 403, body = ErrorBody)))]
async fn tick(State(s): State<AppState>, _svc: Service) -> Result<Json<TickSummary>, ServerError> {
    Ok(Json(s.engine.tick(TickOptions::default()).await?))
}

#[utoipa::path(post, path = "/v1/hooks/enqueue", tag = "engine", security(("bearer" = [])),
    request_body(content = Object, description = "Database webhook payload; ignored"),
    responses((status = 200, description = "Same as tick (service role only)", body = TickSummary), (status = 403, body = ErrorBody)))]
async fn enqueue_hook(State(s): State<AppState>, _svc: Service) -> Result<Json<TickSummary>, ServerError> {
    Ok(Json(s.engine.tick(TickOptions::default()).await?))
}

#[utoipa::path(put, path = "/v1/devices/{device_id}", tag = "push", security(("bearer" = [])),
    params(("device_id" = Uuid, Path, description = "Id the app created for this install")),
    request_body = RegisterDevice,
    responses((status = 200, description = "Registered; the token is never echoed", body = DeviceInfo), (status = 400, body = ErrorBody), (status = 409, description = "device id of another account", body = ErrorBody)))]
async fn register_device(
    State(s): State<AppState>,
    UserId(user): UserId,
    Path(device_id): Path<Uuid>,
    json: Result<Json<RegisterDevice>, JsonRejection>,
) -> Result<Json<DeviceInfo>, ServerError> {
    let req = body(json, false)?;
    let now = crate::rows::utc(s.engine.deps().clock.now_ms());
    Ok(Json(crate::push::register(&s.engine.deps().pool, user, device_id, req, now).await?))
}

#[utoipa::path(delete, path = "/v1/devices/{device_id}", tag = "push", security(("bearer" = [])),
    params(("device_id" = Uuid, Path)),
    responses((status = 204, description = "Deleted, for sign-out"), (status = 404, body = ErrorBody)))]
async fn unregister_device(State(s): State<AppState>, UserId(user): UserId, Path(device_id): Path<Uuid>) -> Result<StatusCode, ServerError> {
    if crate::push::unregister(&s.engine.deps().pool, user, device_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ServerError::NotFound(format!("device {device_id}")))
    }
}

#[utoipa::path(post, path = "/v1/push/tick", tag = "push", security(("bearer" = [])),
    responses((status = 200, description = "Step mode: one push dispatch pass (service role only)", body = PushSummary), (status = 403, body = ErrorBody)))]
async fn push_tick(State(s): State<AppState>, _svc: Service) -> Result<Json<PushSummary>, ServerError> {
    match s.engine.push() {
        Some(pusher) => Ok(Json(pusher.dispatch_once().await?)),
        None => Ok(Json(PushSummary::default())),
    }
}

struct Bearer;

impl Modify for Bearer {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let scheme = HttpBuilder::new().scheme(HttpAuthScheme::Bearer).bearer_format("JWT").build();
        openapi.components.get_or_insert_with(Default::default).add_security_scheme("bearer", SecurityScheme::Http(scheme));
    }
}

/// The OpenAPI document of the engine API, committed as
/// `docs/engine-api.yaml` and checked against the code by a test.
/// Every `/v1` route takes a Supabase JWT as a bearer token.
#[derive(OpenApi)]
#[openapi(
    info(title = "Aroh engine API", version = "1.0.0", description = "Server engine of Aroh: provider keys, goal runs, replans, logs and the run queue."),
    paths(health, list_keys, set_key, delete_key, test_key, start_goal, start_replan, run_again, submit_log, start_triage, list_runs, get_run, answer, cancel_run, retry_run, get_run_limit, set_run_limit, tick, enqueue_hook, register_device, unregister_device, push_tick,
        list_plugins, save_plugin, get_plugin, enable_plugin, list_insights, get_review, set_review, set_timezone,
        list_personas, copy_persona, edit_persona),
    components(schemas(KeyInfo, SetKey, KeyTest, StartGoal, StartReplan, RunAgain, GivenAnswer, SubmitLog, StartTriage, TriageEvent, Started, RunSummary, RunView, Waiting, Answer, Accepted, RunLimit, TickSummary, Health, ErrorBody, ErrorDetail, RegisterDevice, DeviceInfo, PushSummary, Platform, ApnsEnv,
        SavePlugin, PluginView, Insight, ReviewSchedule, SetReview, SetTimezone, TimezoneSet,
        EditPersona, PersonaView, Origin, Kind)),
    modifiers(&Bearer)
)]
pub struct ApiDoc;

/// Returns the OpenAPI document as YAML, the text of `docs/engine-api.yaml`.
/// Deterministic for the same code.
/// Panics only if utoipa cannot render its own document.
pub fn openapi_yaml() -> String {
    ApiDoc::openapi().to_yaml().expect("the document renders")
}

/// Builds the router of every route over `state`.
/// `serve` binds it; tests drive it in process.
/// Returns the router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/v1/keys", get(list_keys))
        .route("/v1/keys/{setting_id}", put(set_key).delete(delete_key))
        .route("/v1/keys/{setting_id}/test", post(test_key))
        .route("/v1/goals", post(start_goal))
        .route("/v1/goals/{goal_id}/replan", post(start_replan))
        .route("/v1/goals/{goal_id}/run-again", post(run_again))
        .route("/v1/logs", post(submit_log))
        .route("/v1/calendar/triage", post(start_triage))
        .route("/v1/runs", get(list_runs))
        .route("/v1/runs/{run_id}", get(get_run))
        .route("/v1/runs/{run_id}/answers", post(answer))
        .route("/v1/runs/{run_id}/cancel", post(cancel_run))
        .route("/v1/runs/{run_id}/retry", post(retry_run))
        .route("/v1/settings/run-limit", get(get_run_limit).put(set_run_limit))
        .route("/v1/tick", post(tick))
        .route("/v1/hooks/enqueue", post(enqueue_hook))
        .route("/v1/goals/{goal_id}/plugins", get(list_plugins).put(save_plugin))
        .route("/v1/plugins/{plugin_id}", get(get_plugin))
        .route("/v1/plugins/{plugin_id}/enable", post(enable_plugin))
        .route("/v1/personas", get(list_personas))
        .route("/v1/personas/{name}", put(edit_persona))
        .route("/v1/personas/{name}/copy", post(copy_persona))
        .route("/v1/devices/{device_id}", put(register_device).delete(unregister_device))
        .route("/v1/push/tick", post(push_tick))
        .route("/v1/insights", get(list_insights))
        .route("/v1/goals/{goal_id}/review", get(get_review).put(set_review))
        .route("/v1/timezone", put(set_timezone))
        .with_state(state)
}
