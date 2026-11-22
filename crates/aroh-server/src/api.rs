use crate::auth::{Service, UserId, Verifier};
use crate::engine::{Answer, Engine, KeyTest, RunSummary, RunView, SetKey, StartGoal, StartReplan, Started, SubmitLog, TickSummary, Waiting};
use crate::error::{ErrorBody, ErrorDetail, ServerError};
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
    paths(health, list_keys, set_key, delete_key, test_key, start_goal, start_replan, submit_log, list_runs, get_run, answer, tick, enqueue_hook),
    components(schemas(KeyInfo, SetKey, KeyTest, StartGoal, StartReplan, SubmitLog, Started, RunSummary, RunView, Waiting, Answer, Accepted, TickSummary, Health, ErrorBody, ErrorDetail)),
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
        .route("/v1/logs", post(submit_log))
        .route("/v1/runs", get(list_runs))
        .route("/v1/runs/{run_id}", get(get_run))
        .route("/v1/runs/{run_id}/answers", post(answer))
        .route("/v1/tick", post(tick))
        .route("/v1/hooks/enqueue", post(enqueue_hook))
        .with_state(state)
}
