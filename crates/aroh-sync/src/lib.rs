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
    #[error("a sync is already running")]
    Busy,
}

static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Holds the process-wide sync slot until dropped, so cursors are only ever
/// written by one sync at a time.
struct SyncGuard;

impl SyncGuard {
    fn acquire() -> Option<SyncGuard> {
        use std::sync::atomic::Ordering;
        RUNNING
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| SyncGuard)
    }
}

impl Drop for SyncGuard {
    fn drop(&mut self) {
        let was = RUNNING.swap(false, std::sync::atomic::Ordering::Release);
        assert!(was);
    }
}

/// What one `sync_now` call did. `at` is unix milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SyncReport {
    pub pushed: usize,
    pub pulled: usize,
    pub at: i64,
}

/// Sync state for the settings screen.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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
/// `sync_state` (`last_sync_at` or `last_error`). Returns `Error::Busy` at
/// once, writing nothing, when another sync in this process is running.
/// Input: the open database and a callback that feeds each remote HLC to the
/// local clock (`aroh-core` `Hlc::receive`).
pub async fn sync_now(conn: &mut Connection, on_remote: &mut (dyn FnMut(&str) + Send)) -> Result<SyncReport, Error> {
    let _guard = SyncGuard::acquire().ok_or(Error::Busy)?;
    store::ensure_state(conn)?;
    let outcome = match config::load() {
        Ok(config) => match auth::valid_session(&config).await {
            Ok(session) => remote::sync_all(conn, &config, &session.access_token, on_remote).await,
            Err(e) => Err(e),
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn second_sync_while_one_runs_is_refused() {
        let mut conn = Connection::open_in_memory().unwrap();
        let held = SyncGuard::acquire().unwrap();
        assert!(matches!(sync_now(&mut conn, &mut |_| {}).await, Err(Error::Busy)));
        assert!(SyncGuard::acquire().is_none());
        drop(held);
        assert!(SyncGuard::acquire().is_some());
    }

    #[test]
    fn sync_future_is_send() {
        fn send<T: Send>(_: T) {}
        let mut conn = Connection::open_in_memory().unwrap();
        send(sync_now(&mut conn, &mut |_| {}));
    }
}
