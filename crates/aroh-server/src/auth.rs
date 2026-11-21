use crate::error::ServerError;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use troupekit_core::clock::Clock;
use troupekit_core::http::{read_limited, HttpRequest, Transport};
use uuid::Uuid;

const JWKS_MAX_BYTES: usize = 64 * 1024;
const JWKS_REFRESH_MS: i64 = 60_000;
const SERVICE_ROLE: &str = "service_role";
const USER_ROLE: &str = "authenticated";

/// Who a verified token speaks for: a signed-in user (the token's `sub`)
/// or the Supabase service role, which only the tick and hook routes
/// accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    User(Uuid),
    Service,
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: Option<String>,
    role: Option<String>,
    aud: Option<Value>,
}

struct Jwks {
    url: String,
    transport: Arc<dyn Transport>,
    cache: RwLock<(i64, JwkSet)>,
}

/// Checks Supabase JWTs: HS256 against the project secret, ES256 and RS256
/// against the project JWKS (fetched, cached, refreshed at most once a
/// minute on an unknown key id). Expiry is always required.
pub struct Verifier {
    secret: Option<DecodingKey>,
    jwks: Option<Jwks>,
    audience: String,
    clock: Arc<dyn Clock>,
}

fn unauthorized(m: &str) -> ServerError {
    ServerError::Unauthorized(m.into())
}

impl Verifier {
    /// Builds a verifier from the HS256 `secret` and or the JWKS at
    /// `jwks` (URL plus transport), accepting user tokens for `audience`.
    /// At least one of the two must be given.
    pub fn new(secret: Option<&str>, jwks: Option<(String, Arc<dyn Transport>)>, audience: &str, clock: Arc<dyn Clock>) -> Verifier {
        assert!(secret.is_some() || jwks.is_some(), "a verifier needs a secret or a JWKS");
        Verifier {
            secret: secret.map(|s| DecodingKey::from_secret(s.as_bytes())),
            jwks: jwks.map(|(url, transport)| Jwks { url, transport, cache: RwLock::new((i64::MIN, JwkSet { keys: vec![] })) }),
            audience: audience.to_string(),
            clock,
        }
    }

    async fn fetch(&self, jwks: &Jwks) -> Result<JwkSet, ServerError> {
        let mut res = jwks.transport.send(HttpRequest::get(&jwks.url)).await.map_err(|e| ServerError::Config(format!("jwks: {e}")))?;
        if res.status != 200 {
            return Err(ServerError::Config(format!("jwks: status {}", res.status)));
        }
        let (body, cut) = read_limited(res.body.as_mut(), JWKS_MAX_BYTES).await.map_err(|e| ServerError::Config(format!("jwks: {e}")))?;
        if cut {
            return Err(ServerError::Config("jwks: document too large".into()));
        }
        serde_json::from_slice(&body).map_err(|e| ServerError::Config(format!("jwks: {e}")))
    }

    async fn jwk_key(&self, kid: Option<&str>) -> Result<DecodingKey, ServerError> {
        let jwks = self.jwks.as_ref().ok_or_else(|| unauthorized("asymmetric tokens are not configured"))?;
        let kid = kid.ok_or_else(|| unauthorized("token has no key id"))?;
        let find = |set: &JwkSet| set.find(kid).map(DecodingKey::from_jwk);
        if let Some(key) = find(&jwks.cache.read().await.1) {
            return key.map_err(|_| unauthorized("unusable signing key"));
        }
        let mut cache = jwks.cache.write().await;
        let now = self.clock.now_ms();
        if cache.0 == i64::MIN || now - cache.0 >= JWKS_REFRESH_MS {
            *cache = (now, self.fetch(jwks).await?);
        }
        find(&cache.1).ok_or_else(|| unauthorized("unknown signing key"))?.map_err(|_| unauthorized("unusable signing key"))
    }

    /// Verifies bearer `token` and returns its caller. Returns
    /// `Unauthorized` for a bad signature, algorithm, expiry, audience or
    /// subject, and for any role other than `authenticated` and `service_role`.
    pub async fn verify(&self, token: &str) -> Result<Caller, ServerError> {
        let header = decode_header(token).map_err(|_| unauthorized("malformed token"))?;
        let key = match header.alg {
            Algorithm::HS256 => self.secret.clone().ok_or_else(|| unauthorized("HS256 tokens are not configured"))?,
            Algorithm::ES256 | Algorithm::RS256 => self.jwk_key(header.kid.as_deref()).await?,
            _ => return Err(unauthorized("unsupported token algorithm")),
        };
        let mut validation = Validation::new(header.alg);
        validation.validate_aud = false;
        validation.set_required_spec_claims(&["exp"]);
        let claims = decode::<Claims>(token, &key, &validation).map_err(|_| unauthorized("invalid token"))?.claims;
        match claims.role.as_deref() {
            Some(SERVICE_ROLE) => Ok(Caller::Service),
            Some(USER_ROLE) => {
                let aud_ok = match &claims.aud {
                    Some(Value::String(a)) => *a == self.audience,
                    Some(Value::Array(list)) => list.iter().any(|a| a.as_str() == Some(self.audience.as_str())),
                    _ => false,
                };
                if !aud_ok {
                    return Err(unauthorized("wrong audience"));
                }
                let sub = claims.sub.as_deref().and_then(|s| Uuid::parse_str(s).ok()).ok_or_else(|| unauthorized("bad subject"))?;
                Ok(Caller::User(sub))
            }
            _ => Err(unauthorized("unsupported role")),
        }
    }
}

async fn caller(parts: &Parts, verifier: &Verifier) -> Result<Caller, ServerError> {
    let value = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| unauthorized("missing bearer token"))?;
    let token = value.strip_prefix("Bearer ").ok_or_else(|| unauthorized("missing bearer token"))?;
    verifier.verify(token.trim()).await
}

/// Extractor of a signed-in user's id from the bearer token.
/// Rejects a missing or bad token with 401 and the service role with 403.
/// Every user route takes it, so no route reads another user's rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserId(pub Uuid);

impl<S> FromRequestParts<S> for UserId
where
    Arc<Verifier>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ServerError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match caller(parts, &Arc::<Verifier>::from_ref(state)).await? {
            Caller::User(id) => Ok(UserId(id)),
            Caller::Service => Err(ServerError::Forbidden("user routes need a user token".into())),
        }
    }
}

/// Extractor that admits only the Supabase service role (`pg_cron` and
/// database webhooks). Rejects a missing or bad token with 401 and a
/// user token with 403.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Service;

impl<S> FromRequestParts<S> for Service
where
    Arc<Verifier>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ServerError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match caller(parts, &Arc::<Verifier>::from_ref(state)).await? {
            Caller::Service => Ok(Service),
            Caller::User(_) => Err(ServerError::Forbidden("this route needs the service role".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;
    use troupekit_core::clock::FixedClock;

    const SECRET: &str = "test-secret-with-enough-length-0123456789";

    fn token(claims: Value, secret: &str) -> String {
        encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(secret.as_bytes())).unwrap()
    }

    fn verifier() -> Verifier {
        Verifier::new(Some(SECRET), None, "authenticated", Arc::new(FixedClock::new(0)))
    }

    #[tokio::test]
    async fn accepts_users_and_service_and_rejects_the_rest() {
        let v = verifier();
        let user = Uuid::now_v7();
        let exp = 4_000_000_000u64;
        let ok = token(json!({"sub": user, "role": "authenticated", "aud": "authenticated", "exp": exp}), SECRET);
        assert_eq!(v.verify(&ok).await.unwrap(), Caller::User(user));
        let svc = token(json!({"role": "service_role", "exp": exp}), SECRET);
        assert_eq!(v.verify(&svc).await.unwrap(), Caller::Service);
        let cases = [
            token(json!({"sub": user, "role": "authenticated", "aud": "authenticated", "exp": exp}), "another-secret-of-enough-length-000000"),
            token(json!({"sub": user, "role": "authenticated", "aud": "authenticated", "exp": 1}), SECRET),
            token(json!({"sub": user, "role": "authenticated", "aud": "other", "exp": exp}), SECRET),
            token(json!({"sub": "not-a-uuid", "role": "authenticated", "aud": "authenticated", "exp": exp}), SECRET),
            token(json!({"sub": user, "role": "anon", "aud": "authenticated", "exp": exp}), SECRET),
            token(json!({"sub": user, "role": "authenticated", "aud": "authenticated"}), SECRET),
            "not.a.token".to_string(),
        ];
        for t in cases {
            assert!(matches!(v.verify(&t).await, Err(ServerError::Unauthorized(_))), "{t}");
        }
    }

    const EC_KEY: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgGFluezosGFhgpfpd
fSqvoeIOThpGhbfzt4Be3fed/ZWhRANCAAR0KWwNSdd6coz/DlxA2dQyEiorlWxb
9rz/No/PDpGSAXOZn7ZOZCC/SCn6ltDmnsq2ljk/aj2Cosk0btHnVGMR
-----END PRIVATE KEY-----
";

    struct JwksServer(std::sync::atomic::AtomicUsize);

    #[async_trait::async_trait]
    impl Transport for JwksServer {
        async fn send(&self, _req: HttpRequest) -> Result<troupekit_core::http::HttpResponse, troupekit_core::http::HttpError> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let set = json!({"keys": [{"kty": "EC", "crv": "P-256", "alg": "ES256", "use": "sig", "kid": "k1",
                "x": "dClsDUnXenKM_w5cQNnUMhIqK5VsW_a8_zaPzw6RkgE", "y": "c5mftk5kIL9IKfqW0OaeyraWOT9qPYKiyTRu0edUYxE"}]});
            Ok(troupekit_core::http::HttpResponse {
                status: 200,
                url: "https://project.supabase.co/auth/v1/.well-known/jwks.json".into(),
                headers: vec![("content-type".into(), "application/json".into())],
                body: Box::new(troupekit_core::http::ChunkBody::new(vec![set.to_string().into_bytes()])),
                truncated: false,
            })
        }
    }

    fn es256(kid: &str, claims: Value) -> String {
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(kid.into());
        encode(&header, &claims, &EncodingKey::from_ec_pem(EC_KEY.as_bytes()).unwrap()).unwrap()
    }

    #[tokio::test]
    async fn verifies_es256_against_the_jwks_and_limits_refetches() {
        let server = Arc::new(JwksServer(Default::default()));
        let jwks: Arc<dyn Transport> = server.clone();
        let clock = Arc::new(FixedClock::new(1_000_000));
        let v = Verifier::new(None, Some(("https://project.supabase.co/auth/v1/.well-known/jwks.json".into(), jwks)), "authenticated", clock.clone());
        let user = Uuid::now_v7();
        let claims = json!({"sub": user, "role": "authenticated", "aud": "authenticated", "exp": 4_000_000_000u64});
        assert_eq!(v.verify(&es256("k1", claims.clone())).await.unwrap(), Caller::User(user));
        assert_eq!(v.verify(&es256("k1", claims.clone())).await.unwrap(), Caller::User(user));
        assert!(v.verify(&es256("k2", claims.clone())).await.is_err());
        assert!(v.verify(&es256("k2", claims.clone())).await.is_err());
        assert_eq!(server.0.load(std::sync::atomic::Ordering::SeqCst), 1, "one fetch per minute at most");
        clock.advance(JWKS_REFRESH_MS);
        assert!(v.verify(&es256("k2", claims.clone())).await.is_err());
        assert_eq!(server.0.load(std::sync::atomic::Ordering::SeqCst), 2);
        let hs = token(claims, SECRET);
        assert!(matches!(v.verify(&hs).await, Err(ServerError::Unauthorized(_))), "HS256 needs the secret");
    }
}
