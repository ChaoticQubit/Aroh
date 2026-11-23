//! Aroh store: SQLite migrations, repositories and the last-write-wins sync engine.

pub mod codec;
pub mod db;
pub mod local;
pub mod repo;
pub mod sync;

/// Errors raised by aroh-store. Messages never carry tokens or keys.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Domain(#[from] aroh_domain::Error),
    #[error("invalid row: {0}")]
    InvalidRow(String),
    #[error("row not found: {0}")]
    NotFound(String),
    #[error("database schema version {0} is newer than this app")]
    SchemaTooNew(i64),
    #[error("remote: {0}")]
    Remote(String),
}

/// Result alias for aroh-store.
pub type Result<T> = std::result::Result<T, Error>;

/// Builds an `Error::InvalidRow` from `reason`, cut to 200 chars.
pub(crate) fn bad_row(reason: impl Into<String>) -> Error {
    Error::InvalidRow(reason.into().chars().take(200).collect())
}
