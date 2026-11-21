pub mod merge;
pub mod store;

/// Every failure aroh-sync reports. Messages never carry tokens or keys.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid row: {0}")]
    InvalidRow(String),
}
