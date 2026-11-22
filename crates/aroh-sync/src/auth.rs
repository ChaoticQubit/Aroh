use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::{self, Config};
use crate::{check, http, Error, Result};

const SESSION_ACCOUNT: &str = "supabase-session";
const REFRESH_MARGIN_SECS: i64 = 60;

/// A GoTrue session as stored in the keychain: tokens, expiry (unix
/// seconds), the account id (`sub`) and email. Debug output hides tokens.
#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub user_id: String,
    pub email: Option<String>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("expires_at", &self.expires_at)
            .field("user_id", &self.user_id)
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
    id: String,
    email: Option<String>,
}

fn now_secs() -> i64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock after 1970");
    i64::try_from(since.as_secs()).expect("time fits i64")
}

fn parse(t: TokenResponse) -> Result<Session> {
    if t.access_token.is_empty() || t.refresh_token.is_empty() {
        return Err(Error::Auth("empty token in response".into()));
    }
    let user = t.user.ok_or_else(|| Error::Auth("no user in response".into()))?;
    let user_id = uuid::Uuid::parse_str(&user.id).map_err(|_| Error::Auth("account id is not a uuid".into()))?;
    Ok(Session {
        access_token: t.access_token,
        refresh_token: t.refresh_token,
        expires_at: t.expires_at.unwrap_or_else(|| now_secs() + t.expires_in),
        user_id: user_id.hyphenated().to_string(),
        email: user.email,
    })
}

async fn token(config: &Config, grant: &str, body: serde_json::Value) -> Result<Session> {
    let response = http()
        .post(format!("{}/auth/v1/token?grant_type={grant}", config.url))
        .header("apikey", &config.anon_key)
        .json(&body)
        .send()
        .await?;
    parse(check(response).await?.json().await?)
}

fn save(session: &Session) -> Result<()> {
    config::write(SESSION_ACCOUNT, &serde_json::to_string(session).expect("session serializes"))
}

/// Signs in with email and password and stores the session in the keychain.
/// Input: the project config and the owner's credentials (never stored).
/// Output: the new session.
pub async fn sign_in_password(config: &Config, email: &str, password: &str) -> Result<Session> {
    let session = password_session(config, email, password).await?;
    save(&session)?;
    Ok(session)
}

/// Signs in with email and password and returns the session without
/// touching the keychain. Used by `sign_in_password` and the live tests.
pub async fn password_session(config: &Config, email: &str, password: &str) -> Result<Session> {
    token(config, "password", json!({"email": email.trim(), "password": password})).await
}

/// Creates an account through GoTrue sign-up and returns its session
/// without touching the keychain. Errors when the project requires email
/// confirmation (no session comes back) or the email is taken.
pub async fn sign_up_session(config: &Config, email: &str, password: &str) -> Result<Session> {
    let response = http()
        .post(format!("{}/auth/v1/signup", config.url))
        .header("apikey", &config.anon_key)
        .json(&json!({"email": email.trim(), "password": password}))
        .send()
        .await?;
    let body: serde_json::Value = check(response).await?.json().await?;
    if body.get("access_token").is_none() {
        return Err(Error::Auth("confirm the email, then sign in".into()));
    }
    parse(serde_json::from_value(body).map_err(|_| Error::Auth("unexpected sign-up response".into()))?)
}

/// Creates an account and stores its session in the keychain.
/// Input: the project config and the new credentials.
/// Output: the new session.
pub async fn sign_up(config: &Config, email: &str, password: &str) -> Result<Session> {
    let session = sign_up_session(config, email, password).await?;
    save(&session)?;
    Ok(session)
}

/// Revokes the session on the server when reachable and always removes it
/// from the keychain.
pub async fn sign_out(config: Option<&Config>) -> Result<()> {
    if let (Some(config), Some(session)) = (config, stored()?) {
        let _ = http()
            .post(format!("{}/auth/v1/logout", config.url))
            .header("apikey", &config.anon_key)
            .bearer_auth(&session.access_token)
            .send()
            .await;
    }
    config::delete(SESSION_ACCOUNT)
}

/// The stored session, if any, without refreshing it. A corrupt entry
/// reads as signed out.
pub fn stored() -> Result<Option<Session>> {
    Ok(config::read(SESSION_ACCOUNT)?.and_then(|json| serde_json::from_str(&json).ok()))
}

/// A session whose access token is valid for at least a minute, refreshed
/// through GoTrue (and stored again) when it is about to expire.
/// Errors with `NotSignedIn` when there is no session.
pub async fn valid_session(config: &Config) -> Result<Session> {
    let session = stored()?.ok_or(Error::NotSignedIn)?;
    if session.expires_at - REFRESH_MARGIN_SECS > now_secs() {
        return Ok(session);
    }
    let mut fresh = token(config, "refresh_token", json!({"refresh_token": session.refresh_token})).await?;
    if fresh.email.is_none() {
        fresh.email = session.email;
    }
    save(&fresh)?;
    Ok(fresh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(id: &str) -> TokenResponse {
        TokenResponse {
            access_token: "ACCESS-SECRET".into(),
            refresh_token: "REFRESH-SECRET".into(),
            expires_in: 3600,
            expires_at: None,
            user: Some(User { id: id.into(), email: Some("me@aroh.test".into()) }),
        }
    }

    #[test]
    fn session_parses_and_hides_tokens() {
        let s = parse(response("0190F2A0-0000-7000-8000-00000000000A")).unwrap();
        assert_eq!(s.user_id, "0190f2a0-0000-7000-8000-00000000000a");
        assert!(s.expires_at > now_secs());
        let shown = format!("{s:?}");
        assert!(!shown.contains("ACCESS-SECRET") && !shown.contains("REFRESH-SECRET"));
        assert!(parse(response("not-a-uuid")).is_err());
        assert!(parse(TokenResponse { user: None, ..response("0190f2a0-0000-7000-8000-00000000000a") }).is_err());
    }
}
