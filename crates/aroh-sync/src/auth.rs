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

/// Trades a native identity token (Sign in with Apple) for a session without touching
/// the keychain. `nonce` is the raw nonce whose SHA-256 the app handed to Apple; GoTrue
/// hashes it again and compares. Errors on an empty token or a refusal.
pub async fn id_token_session(config: &Config, provider: &str, id_token: &str, nonce: &str) -> Result<Session> {
    if id_token.is_empty() || nonce.is_empty() {
        return Err(Error::Auth("Apple returned no identity token".into()));
    }
    token(config, "id_token", json!({"provider": provider, "id_token": id_token, "nonce": nonce})).await
}

/// Signs in with a native identity token and stores the session in the keychain.
/// Input: the project config, the provider ("apple"), the token and the raw nonce.
/// Output: the new session.
pub async fn sign_in_id_token(config: &Config, provider: &str, id_token: &str, nonce: &str) -> Result<Session> {
    let session = id_token_session(config, provider, id_token, nonce).await?;
    save(&session)?;
    Ok(session)
}

/// What a GoTrue sign-up gave back: a session, or none because the project
/// requires the new address to be confirmed first ("check your email").
#[derive(Debug)]
pub enum SignUp {
    SignedIn(Session),
    ConfirmEmail,
}

async fn sign_up_reply(config: &Config, email: &str, password: &str) -> Result<SignUp> {
    let response = http()
        .post(format!("{}/auth/v1/signup", config.url))
        .header("apikey", &config.anon_key)
        .json(&json!({"email": email.trim(), "password": password}))
        .send()
        .await?;
    let body: serde_json::Value = check(response).await?.json().await?;
    if body.get("access_token").is_none() {
        return Ok(SignUp::ConfirmEmail);
    }
    Ok(SignUp::SignedIn(parse(serde_json::from_value(body).map_err(|_| Error::Auth("unexpected sign-up response".into()))?)?))
}

/// Creates an account through GoTrue sign-up and returns its session
/// without touching the keychain. Errors when the project requires email
/// confirmation (no session comes back) or the email is taken.
pub async fn sign_up_session(config: &Config, email: &str, password: &str) -> Result<Session> {
    match sign_up_reply(config, email, password).await? {
        SignUp::SignedIn(session) => Ok(session),
        SignUp::ConfirmEmail => Err(Error::Auth("confirm the email, then sign in".into())),
    }
}

/// Creates an account and stores its session in the keychain when one comes back.
/// Input: the project config and the new credentials.
/// Output: the session, or `ConfirmEmail` when the address must be confirmed first.
pub async fn sign_up(config: &Config, email: &str, password: &str) -> Result<SignUp> {
    let reply = sign_up_reply(config, email, password).await?;
    if let SignUp::SignedIn(session) = &reply {
        save(session)?;
    }
    Ok(reply)
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

    /// Serves one HTTP request on a local port with a GoTrue token reply.
    /// Returns the project URL and a handle that yields the raw request head.
    async fn gotrue_once() -> (String, tokio::task::JoinHandle<String>) {
        gotrue_reply(json!({"access_token": "a", "refresh_token": "r", "expires_in": 3600, "user": {"id": "0190f2a0-0000-7000-8000-00000000000a"}})).await
    }

    /// Serves one HTTP request on a local port with `body` as the JSON reply.
    /// Returns the project URL and a handle that yields the raw request head.
    async fn gotrue_reply(body: serde_json::Value) -> (String, tokio::task::JoinHandle<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let served = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut seen = Vec::new();
            let mut chunk = [0u8; 4096];
            while !String::from_utf8_lossy(&seen).contains("\r\n\r\n") {
                let n = socket.read(&mut chunk).await.unwrap();
                assert!(n > 0, "the client closed before sending headers");
                seen.extend_from_slice(&chunk[..n]);
            }
            let body = body.to_string();
            let reply = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
            socket.write_all(reply.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&seen).to_lowercase()
        });
        (url, served)
    }

    #[tokio::test]
    async fn a_sign_up_that_needs_email_confirmation_is_check_your_email_not_an_error() {
        let (url, served) = gotrue_reply(json!({"id": "0190f2a0-0000-7000-8000-00000000000a", "email": "me@aroh.test", "confirmation_sent_at": "2026-11-23T12:00:00Z"})).await;
        let config = Config::new(&url, "sb_publishable_test_key_0001").unwrap();
        let reply = sign_up_reply(&config, "me@aroh.test", "secret-password").await.unwrap();
        assert!(matches!(reply, SignUp::ConfirmEmail), "{reply:?}");
        assert!(served.await.unwrap().starts_with("post /auth/v1/signup "));
        let (url, _) = gotrue_once().await;
        let config = Config::new(&url, "sb_publishable_test_key_0001").unwrap();
        assert!(matches!(sign_up_reply(&config, "me@aroh.test", "secret-password").await.unwrap(), SignUp::SignedIn(_)));
    }

    #[tokio::test]
    async fn apple_sign_in_trades_the_id_token_and_raw_nonce_for_a_session() {
        let (url, served) = gotrue_once().await;
        let config = Config::new(&url, "sb_publishable_test_key_0001").unwrap();
        let session = id_token_session(&config, "apple", "header.payload.sig", "raw-nonce").await.unwrap();
        assert_eq!(session.user_id, "0190f2a0-0000-7000-8000-00000000000a");
        let head = served.await.unwrap();
        assert!(head.starts_with("post /auth/v1/token?grant_type=id_token "), "{head}");
        assert!(id_token_session(&config, "apple", "", "raw-nonce").await.is_err(), "an empty token never reaches the server");
    }

    #[tokio::test]
    async fn sign_in_sends_the_publishable_key_as_apikey_and_never_as_a_bearer() {
        let key = "sb_publishable_FixtureKeyForTests_0123456789";
        let (url, served) = gotrue_once().await;
        let config = Config::new(&url, key).unwrap();
        let session = password_session(&config, "me@aroh.test", "secret-password").await.unwrap();
        assert_eq!(session.user_id, "0190f2a0-0000-7000-8000-00000000000a");
        let head = served.await.unwrap();
        assert!(head.starts_with("post /auth/v1/token?grant_type=password "), "{head}");
        assert!(head.contains(&format!("apikey: {}", key.to_lowercase())), "{head}");
        assert!(!head.contains("authorization:"), "a publishable key is not a JWT, so it never goes in Authorization: {head}");
    }
}
