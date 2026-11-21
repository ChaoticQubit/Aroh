use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::{self, Config, SESSION_ACCOUNT};
use crate::Error;

const REFRESH_MARGIN_SECS: i64 = 60;

/// A GoTrue session as stored in the keychain. Debug output hides tokens.
#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub email: Option<String>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("expires_at", &self.expires_at)
            .field("email", &self.email)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    expires_at: Option<i64>,
    user: Option<User>,
}

#[derive(Deserialize)]
struct User {
    email: Option<String>,
}

pub(crate) fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("http client builds")
}

pub(crate) fn now_secs() -> i64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock after 1970");
    i64::try_from(since.as_secs()).expect("time fits i64")
}

/// Turns a non-2xx response into an error with a short body excerpt.
pub(crate) async fn check(response: reqwest::Response) -> Result<reqwest::Response, Error> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body: String = response.text().await.unwrap_or_default().chars().take(200).collect();
    Err(Error::Status(status.as_u16(), body))
}

async fn token(config: &Config, grant: &str, body: serde_json::Value) -> Result<Session, Error> {
    let response = http()
        .post(format!("{}/auth/v1/token?grant_type={grant}", config.url))
        .header("apikey", &config.anon_key)
        .json(&body)
        .send()
        .await?;
    store(check(response).await?.json().await?)
}

fn store(t: TokenResponse) -> Result<Session, Error> {
    if t.access_token.is_empty() || t.refresh_token.is_empty() {
        return Err(Error::Auth("empty token in response".into()));
    }
    let session = Session {
        access_token: t.access_token,
        refresh_token: t.refresh_token,
        expires_at: t.expires_at.unwrap_or_else(|| now_secs() + t.expires_in),
        email: t.user.and_then(|u| u.email),
    };
    config::write(SESSION_ACCOUNT, &serde_json::to_string(&session).expect("session serializes"))?;
    Ok(session)
}

/// Signs in with email and password and stores the session.
pub async fn sign_in_password(email: &str, password: &str) -> Result<Session, Error> {
    token(&config::load()?, "password", json!({"email": email, "password": password})).await
}

/// Sends a one-time sign-in code to `email`.
pub async fn request_otp(email: &str) -> Result<(), Error> {
    let config = config::load()?;
    let response = http()
        .post(format!("{}/auth/v1/otp", config.url))
        .header("apikey", &config.anon_key)
        .json(&json!({"email": email, "create_user": true}))
        .send()
        .await?;
    check(response).await?;
    Ok(())
}

/// Exchanges the emailed code for a session and stores it.
pub async fn verify_otp(email: &str, code: &str) -> Result<Session, Error> {
    let config = config::load()?;
    let response = http()
        .post(format!("{}/auth/v1/verify", config.url))
        .header("apikey", &config.anon_key)
        .json(&json!({"type": "email", "email": email, "token": code.trim()}))
        .send()
        .await?;
    store(check(response).await?.json().await?)
}

/// Revokes the session on the server when reachable and always removes it
/// from the keychain.
pub async fn sign_out() -> Result<(), Error> {
    if let (Ok(config), Some(session)) = (config::load(), stored()?) {
        let _ = http()
            .post(format!("{}/auth/v1/logout", config.url))
            .header("apikey", &config.anon_key)
            .bearer_auth(&session.access_token)
            .send()
            .await;
    }
    config::delete(SESSION_ACCOUNT)
}

/// The stored session, if any, without refreshing it.
pub fn stored() -> Result<Option<Session>, Error> {
    match config::read(SESSION_ACCOUNT)? {
        None => Ok(None),
        Some(json) => Ok(serde_json::from_str(&json).ok()),
    }
}

/// A session whose access token is valid for at least a minute, refreshed
/// through GoTrue when it is about to expire.
pub(crate) async fn valid_session(config: &Config) -> Result<Session, Error> {
    let session = stored()?.ok_or(Error::NotSignedIn)?;
    if session.expires_at - REFRESH_MARGIN_SECS > now_secs() {
        return Ok(session);
    }
    let mut fresh = token(config, "refresh_token", json!({"refresh_token": session.refresh_token})).await?;
    if fresh.email.is_none() {
        fresh.email = session.email;
        config::write(SESSION_ACCOUNT, &serde_json::to_string(&fresh).expect("session serializes"))?;
    }
    Ok(fresh)
}
