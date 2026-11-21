pub mod auth;
pub mod config;
pub mod merge;
pub mod remote;
pub mod store;

use rusqlite::Connection;

pub use auth::{request_otp, sign_in_password, sign_out, verify_otp, Session};
pub use config::{save as configure, validate_url};

/// Every failure aroh-sync reports. Messages never carry tokens or keys.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid row: {0}")]
    InvalidRow(String),
    #[error("config: {0}")]
    Config(String),
    #[error("sync is not configured")]
    NotConfigured,
    #[error("not signed in")]
    NotSignedIn,
    #[error("auth: {0}")]
    Auth(String),
    #[error("keychain: {0}")]
    Keychain(#[from] keyring::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server answered {0}: {1}")]
    Status(u16, String),
}

/// What one `sync_now` call did. `at` is unix milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncReport {
    pub pushed: usize,
    pub pulled: usize,
    pub at: i64,
}

/// Sync state for the settings screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncStatus {
    pub signed_in: bool,
    pub email: Option<String>,
    pub last_sync_at: Option<i64>,
    pub last_error: Option<String>,
}

fn now_millis() -> i64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock after 1970");
    i64::try_from(since.as_millis()).expect("time fits i64")
}

/// Pushes local changes, pulls remote ones and records the outcome in
/// `sync_state` (`last_sync_at` or `last_error`).
/// Input: the open database and a callback that feeds each remote HLC to the
/// local clock (`aroh-core` `Hlc::receive`).
pub async fn sync_now(conn: &mut Connection, on_remote: &mut dyn FnMut(&str)) -> Result<SyncReport, Error> {
    store::ensure_state(conn)?;
    let outcome = match config::load() {
        Ok(config) => remote::sync_all(conn, &config, on_remote).await,
        Err(e) => Err(e),
    };
    match outcome {
        Ok(moved) => {
            let at = now_millis();
            store::set_state(conn, "last_sync_at", &at.to_string())?;
            conn.execute("DELETE FROM sync_state WHERE key = 'last_error'", [])?;
            Ok(SyncReport { pushed: moved.pushed, pulled: moved.pulled, at })
        }
        Err(e) => {
            store::set_state(conn, "last_error", &e.to_string())?;
            Err(e)
        }
    }
}

/// Reads sign-in state from the keychain and the last sync outcome from
/// `sync_state`.
pub fn status(conn: &Connection) -> Result<SyncStatus, Error> {
    store::ensure_state(conn)?;
    let session = auth::stored()?;
    Ok(SyncStatus {
        signed_in: session.is_some(),
        email: session.and_then(|s| s.email),
        last_sync_at: store::get_state(conn, "last_sync_at")?.and_then(|v| v.parse().ok()),
        last_error: store::get_state(conn, "last_error")?,
    })
}
