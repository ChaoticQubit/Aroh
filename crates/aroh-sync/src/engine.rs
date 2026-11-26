use crate::{http, Error, Result};
use serde_json::Value;

/// Largest engine response body read, in bytes.
pub const MAX_BODY: usize = 1 << 20;

/// What the engine answered: HTTP status and the JSON body (`null` when
/// empty or not JSON). Error statuses are answers too; the caller maps them.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Answer {
    pub status: u16,
    pub body: Value,
}

/// Checks one engine call at the trust boundary: method GET, POST, PUT or
/// DELETE; path under `/v1/` with only `[A-Za-z0-9_-/]` segments, no `..`
/// or `//`, and an optional query of `[A-Za-z0-9_=&-]`. Output: the method.
pub fn check_call(method: &str, path: &str) -> Result<reqwest::Method> {
    let method = match method {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        _ => return Err(Error::Config(format!("engine method {method} is not allowed"))),
    };
    let bad = || Error::Config("engine path is not allowed".into());
    if path.len() > 512 {
        return Err(bad());
    }
    let (route, query) = path.split_once('?').unwrap_or((path, ""));
    let route_ok = route.starts_with("/v1/")
        && !route.contains("..")
        && !route.contains("//")
        && route.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'/' || b == b'-' || b == b'_');
    let query_ok = query.bytes().all(|b| b.is_ascii_alphanumeric() || b"_=&-".contains(&b));
    if !route_ok || !query_ok {
        return Err(bad());
    }
    Ok(method)
}

/// Sends one checked call to the engine at `base` with the user's bearer
/// token and an optional JSON body. Output: the answer; network failures
/// are errors. The token and body are never logged.
pub async fn call(base: &str, token: &str, method: &str, path: &str, body: Option<&Value>) -> Result<Answer> {
    send(base, token, method, path, body, None).await
}

/// Longest wait for an engine call that makes one model turn.
pub const MODEL_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(150);

/// Like `call`, for an engine route that makes one model turn (structuring a
/// diary day, reading an image): waits up to `MODEL_CALL_TIMEOUT` instead of
/// the client's 30 seconds. Output: the answer; network failures are errors.
pub async fn call_model(base: &str, token: &str, method: &str, path: &str, body: Option<&Value>) -> Result<Answer> {
    send(base, token, method, path, body, Some(MODEL_CALL_TIMEOUT)).await
}

async fn send(base: &str, token: &str, method: &str, path: &str, body: Option<&Value>, timeout: Option<std::time::Duration>) -> Result<Answer> {
    let method = check_call(method, path)?;
    aroh_domain::rows::check_url("server_url", base).map_err(|e| Error::Config(e.to_string()))?;
    let mut request = http().request(method, format!("{}{path}", base.trim_end_matches('/'))).bearer_auth(token);
    if let Some(t) = timeout {
        request = request.timeout(t);
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await?;
    let status = response.status().as_u16();
    let bytes = response.bytes().await?;
    if bytes.len() > MAX_BODY {
        return Err(Error::Http("engine answer too large".into()));
    }
    Ok(Answer { status, body: serde_json::from_slice(&bytes).unwrap_or(Value::Null) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_path_rules() {
        for ok in ["/v1/keys", "/v1/runs?goal_id=0190f2a0-0000-7000-8000-00000000000a&state=waiting_human", "/v1/runs/abc?after_seq=4", "/v1/keys/x/test", "/v1/diary/structure", "/v1/files/transcribe"] {
            assert!(check_call("GET", ok).is_ok(), "{ok}");
        }
        for bad in ["/healthz", "/v1/../tick", "//evil.com/v1/x", "http://evil.com/v1/x", "/v1/x?a=b#c", "/v1/x?a=%2e", "/v1/x y"] {
            assert!(check_call("GET", bad).is_err(), "{bad}");
        }
        assert!(check_call("PATCH", "/v1/keys").is_err());
        assert_eq!(check_call("DELETE", "/v1/keys/x").unwrap(), reqwest::Method::DELETE);
    }
}
