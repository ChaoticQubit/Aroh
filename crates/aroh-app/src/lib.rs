//! Aroh app engine: the deterministic work the app does on device over the rebuild
//! store (start-up tick, screen reads, writes, reminder planning). No AI, no network.

pub mod calendar;
pub mod card;
pub mod dashboard;
pub mod diary;
pub mod health;
pub mod image_text;
pub mod local;
pub mod location;
pub mod notebook;
pub mod remind;
pub mod remove;
pub mod sample;
pub mod snapshot;
pub mod tick;
pub mod undo;
pub mod travel;
pub mod write;

/// Errors raised by the app engine. Messages are safe to show and never carry secrets.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] aroh_store::Error),
    #[error(transparent)]
    Domain(#[from] aroh_domain::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid {0}")]
    Invalid(String),
}

/// Result alias for the app engine.
pub type Result<T> = std::result::Result<T, Error>;

/// Builds an `Error::Invalid` from `reason`, cut to 200 chars.
pub(crate) fn invalid(reason: impl Into<String>) -> Error {
    Error::Invalid(reason.into().chars().take(200).collect())
}
