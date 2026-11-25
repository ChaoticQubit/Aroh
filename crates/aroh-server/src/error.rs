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
    #[error("{message}")]
    Refused { code: &'static str, message: String },
}

/// Codes the database raises as `code: message` from its limit triggers and the
/// account delete function; the engine answers them like its own refusals.
pub const DB_REFUSALS: [&str; 4] = ["goal_limit", "file_too_large", "storage_full", "files_remain"];

/// Reads a database refusal out of error `message` (`code: text`).
/// Returns the code and the text, or `None` for any other error.
/// Only codes in `DB_REFUSALS` count.
pub fn db_refusal(message: &str) -> Option<(&'static str, String)> {
    let (code, text) = message.split_once(": ")?;
    let code = DB_REFUSALS.iter().find(|c| **c == code)?;
    Some((code, text.to_string()))
}

impl From<db::Error> for ServerError {
    fn from(e: db::Error) -> Self {
        if matches!(e, db::Error::RowNotFound) {
            return ServerError::NotFound("no such row".into());
        }
        let refusal = e.as_database_error().and_then(|d| db_refusal(d.message()));
        match refusal {
            Some((code, message)) => ServerError::Refused { code, message },
            None => ServerError::Db(e.to_string()),
        }
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

/// Code and message of an error response. Codes: bad_request, unauthorized,
/// forbidden, not_found, conflict, setup, internal, and the refusals
/// consent_required, daily_run_limit, goal_limit, file_too_large, storage_full,
/// files_remain, account_disabled.
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
            ServerError::Refused { code, .. } => (refusal_status(code), code),
        }
    }
}

fn refusal_status(code: &str) -> StatusCode {
    match code {
        "consent_required" | "account_disabled" => StatusCode::FORBIDDEN,
        "daily_run_limit" => StatusCode::TOO_MANY_REQUESTS,
        _ => StatusCode::CONFLICT,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_refusals_keep_their_code_and_status() {
        assert_eq!(db_refusal("goal_limit: You have 5 active goals."), Some(("goal_limit", "You have 5 active goals.".to_string())));
        assert_eq!(db_refusal("files_remain: try again").map(|r| r.0), Some("files_remain"));
        assert_eq!(db_refusal("duplicate key: value"), None);
        assert_eq!(db_refusal("goal_limit"), None);
        let status = |code| ServerError::Refused { code, message: String::new() }.status();
        assert_eq!(status("consent_required"), (StatusCode::FORBIDDEN, "consent_required"));
        assert_eq!(status("daily_run_limit"), (StatusCode::TOO_MANY_REQUESTS, "daily_run_limit"));
        assert_eq!(status("goal_limit"), (StatusCode::CONFLICT, "goal_limit"));
        assert_eq!(status("account_disabled").0, StatusCode::FORBIDDEN);
        assert_eq!(ServerError::Refused { code: "goal_limit", message: "Archive one.".into() }.to_string(), "Archive one.");
    }

    #[test]
    fn b96_a_missing_row_is_not_found_and_other_database_errors_stay_internal() {
        assert_eq!(ServerError::from(db::Error::RowNotFound).status(), (StatusCode::NOT_FOUND, "not_found"));
        assert_eq!(ServerError::from(db::Error::Pool("gone".into())).status(), (StatusCode::INTERNAL_SERVER_ERROR, "internal"));
    }
}
