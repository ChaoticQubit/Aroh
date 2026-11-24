use crate::db;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use troupekit_core::error::CoreError;
use troupekit_graph::GraphError;
use troupekit_host::HostError;
use utoipa::ToSchema;

/// Why a server call failed. Each variant maps to one HTTP status and one
/// error code; messages never carry keys, tokens or database internals.
/// `Db` and `Crypto` details stay in the server log, never in a response.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("setup: {0}")]
    Setup(String),
    #[error("config: {0}")]
    Config(String),
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("database: {0}")]
    Db(String),
    #[error(transparent)]
    Domain(#[from] aroh_domain::Error),
}

impl From<db::Error> for ServerError {
    fn from(e: db::Error) -> Self {
        ServerError::Db(e.to_string())
    }
}

impl From<CoreError> for ServerError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::Conflict { .. } => ServerError::Conflict(e.to_string()),
            CoreError::NotFound(m) => ServerError::NotFound(m),
            CoreError::InvalidMessage(m) => ServerError::BadRequest(m),
            other => ServerError::Db(other.to_string()),
        }
    }
}

impl From<GraphError> for ServerError {
    fn from(e: GraphError) -> Self {
        match e {
            GraphError::Resume(m) => ServerError::Conflict(m),
            GraphError::Setup(m) => ServerError::Setup(m),
            GraphError::Store(c) => c.into(),
            other => ServerError::Db(other.to_string()),
        }
    }
}

impl From<HostError> for ServerError {
    fn from(e: HostError) -> Self {
        match e {
            HostError::Graph(g) => g.into(),
            HostError::Store(c) => c.into(),
            HostError::NoCheckpoint(run) => ServerError::NotFound(format!("checkpoint of run {run}")),
        }
    }
}

/// Turns a database error into the `CoreError` a Troupekit store trait returns.
/// The message keeps the database's wording, which never holds a key.
/// Returns `CoreError::Store`.
pub fn store_err(e: db::Error) -> CoreError {
    CoreError::Store(e.to_string())
}

/// The body of every error response: a stable code and a short message.
/// Clients branch on `code`; `message` is for people.
/// Serialized as `{"error": {"code", "message"}}`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

/// Code and message of an error response.
/// Codes: bad_request, unauthorized, forbidden, not_found, conflict,
/// setup, internal.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}

impl ServerError {
    /// Returns the HTTP status and error code of this error.
    /// Database, crypto and config failures are 500 `internal`.
    /// Domain validation failures are 400 `bad_request`.
    pub fn status(&self) -> (StatusCode, &'static str) {
        match self {
            ServerError::BadRequest(_) | ServerError::Domain(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ServerError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ServerError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            ServerError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ServerError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ServerError::Setup(_) => (StatusCode::UNPROCESSABLE_ENTITY, "setup"),
            ServerError::Config(_) | ServerError::Crypto(_) | ServerError::Db(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        }
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let (status, code) = self.status();
        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            eprintln!("aroh-server: {self}");
            "internal error".to_string()
        } else {
            self.to_string().chars().take(500).collect()
        };
        let body = ErrorBody { error: ErrorDetail { code: code.into(), message } };
        (status, Json(body)).into_response()
    }
}
