//! Aroh core: shared row types, SQLite schema, hybrid logical clock and repositories.

pub mod db;
pub use aroh_domain::hlc;
pub mod keychain;
pub mod models;
pub mod repo;

/// Errors raised by aroh-core operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid value {value:?} for {field}")]
    InvalidEnum { field: &'static str, value: String },
    #[error("row not found: {0}")]
    NotFound(String),
    #[error("database schema version {0} is newer than this app")]
    SchemaTooNew(i64),
    #[error("keychain: {0}")]
    Keychain(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
}

/// Result alias for aroh-core.
pub type Result<T> = std::result::Result<T, Error>;

/// Returns a fresh time-ordered row id (uuid v7) as a string.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}
