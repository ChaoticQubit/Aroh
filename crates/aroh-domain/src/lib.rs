//! Aroh domain: row types, hybrid logical clock, validation and recurrence shared by
//! the app engine and the server engine. No I/O.

pub mod calendar;
pub mod card;
pub mod category;
pub mod dashboard;
pub mod hlc;
pub mod lww;
pub mod recur;
pub mod remind;
pub mod rows;
pub mod schema;
pub mod seed;
pub mod time;
pub mod travel;

/// Errors raised by domain checks. Messages never carry secrets.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("invalid hlc timestamp: {0}")]
    InvalidHlc(String),
    #[error("invalid {field}: {reason}")]
    Invalid { field: &'static str, reason: String },
    #[error("recurrence: {0}")]
    Recurrence(String),
}

/// Result alias for aroh-domain.
pub type Result<T> = std::result::Result<T, Error>;

/// Returns a fresh time-ordered row id (uuid v7) as a lowercase hyphenated string.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Builds an `Error::Invalid` for `field`, cutting `reason` to 200 chars.
/// Input: the field path (for example `goals.title`) and a human reason.
/// Output: the error value, ready to return.
pub fn invalid(field: &'static str, reason: impl Into<String>) -> Error {
    Error::Invalid { field, reason: reason.into().chars().take(200).collect() }
}

/// Returns a deterministic v7-shaped uuid for `key` within `scope`, with `millis` as
/// its timestamp and a v5 hash of scope and key as its random bits. Used for rows that
/// several engines derive independently (seeds, occurrences), so they collapse under LWW.
pub fn derived_id(scope: &str, key: &str, millis: u64) -> String {
    let namespace = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, scope.as_bytes());
    let hash = uuid::Uuid::new_v5(&namespace, key.as_bytes());
    let mut random = [0u8; 10];
    random.copy_from_slice(&hash.as_bytes()[..10]);
    uuid::Builder::from_unix_timestamp_millis(millis, &random).into_uuid().to_string()
}
