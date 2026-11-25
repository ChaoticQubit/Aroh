//! Aroh app network crate: Supabase sign-in with the session in the OS keychain,
//! the PostgREST `Remote` that drives `aroh_store::sync`, engine requests and the Realtime
//! listener that wakes run views.

pub mod auth;
pub mod config;
pub mod engine;
pub mod realtime;
pub mod rest;
pub mod storage;

pub use auth::Session;
pub use config::Config;

/// The text of any 5xx answer: no status number, no body (B96).
pub const SERVER_PROBLEM: &str = "The server had a problem. Try again in a moment.";

/// Every failure aroh-sync reports. Messages never carry tokens or keys.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config: {0}")]
    Config(String),
    #[error("not signed in")]
    NotSignedIn,
    #[error("auth: {0}")]
    Auth(String),
    #[error("keychain: {0}")]
    Keychain(#[from] keyring::Error),
    #[error("network: {0}")]
    Http(String),
    #[error("{}", status_text(*.0, .1))]
    Status(u16, String),
    #[error("{0}: {1}")]
    Refused(&'static str, String),
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Error {
        Error::Http(e.without_url().to_string())
    }
}

/// The display text of a non-2xx answer: `SERVER_PROBLEM` for a 5xx, else the status and body excerpt.
fn status_text(status: u16, body: &str) -> String {
    if status >= 500 {
        return SERVER_PROBLEM.to_string();
    }
    format!("server answered {status}: {body}")
}

/// Result alias for aroh-sync.
pub type Result<T> = std::result::Result<T, Error>;

/// The process-wide HTTP client: rustls, 30 s timeout, no redirects, so a
/// bearer token is never replayed to another host. Built once, so every call
/// reuses its connection pool instead of a fresh TCP and TLS handshake.
static CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("http client builds")
});

/// The one shared client itself, for tests that check every call gets the same one.
fn shared() -> &'static reqwest::Client {
    &CLIENT
}

/// A handle to the shared HTTP client; clones share one connection pool.
/// Input: none. Output: a cheap clone of the process-wide client.
pub fn http() -> reqwest::Client {
    shared().clone()
}

/// Turns a non-2xx response into `Error::Status` with a 200-char body excerpt.
/// Input: a response. Output: the same response when it succeeded.
pub async fn check(response: reqwest::Response) -> Result<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body: String = response.text().await.unwrap_or_default().chars().take(200).collect();
    Err(Error::Status(status.as_u16(), body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_call_shares_one_client_and_its_connection_pool() {
        let first: *const reqwest::Client = shared();
        let second: *const reqwest::Client = shared();
        assert!(std::ptr::eq(first, second));
        let _ = http();
        assert!(std::ptr::eq(first, shared()));
    }

    #[test]
    fn b96_a_server_failure_reads_as_plain_text_and_a_refusal_keeps_its_words() {
        let failed = Error::Status(500, "{\"error\":{\"code\":\"internal\",\"message\":\"internal error\"}}".into()).to_string();
        assert_eq!(failed, SERVER_PROBLEM);
        assert_eq!(Error::Status(503, "<html>503</html>".into()).to_string(), SERVER_PROBLEM);
        assert_eq!(Error::Status(409, "conflict".into()).to_string(), "server answered 409: conflict");
        assert_eq!(Error::Refused("goal_limit", "You have 5 active goals.".into()).to_string(), "goal_limit: You have 5 active goals.");
    }
}
