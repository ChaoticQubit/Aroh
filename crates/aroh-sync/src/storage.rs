//! Supabase Storage for goal files: the owner's private `goal-files` bucket, one folder per
//! account, written with the owner's own session so storage RLS is the only gate.

use crate::{check, http, Config, Error, Result};
use std::time::Duration;

/// The private bucket that holds goal files.
pub const BUCKET: &str = "goal-files";

/// Largest file the app uploads, matching the bucket's `file_size_limit`.
pub const MAX_BYTES: usize = 25 * 1024 * 1024;

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// The object URL of `storage_path` (`goals/<goal_id>/<file>`) in `user_id`'s folder.
/// Refuses a path with characters outside `[A-Za-z0-9._/-]` or a `..` segment, so the
/// URL needs no escaping and cannot leave the owner's folder.
pub fn object_url(config: &Config, user_id: &str, storage_path: &str) -> Result<String> {
    let safe = storage_path.bytes().all(|b| b.is_ascii_alphanumeric() || b"._/-".contains(&b));
    if !safe || storage_path.split('/').any(|p| p.is_empty() || p == "." || p == "..") || user_id.is_empty() {
        return Err(Error::Config(format!("bad storage path {storage_path}")));
    }
    Ok(format!("{}/storage/v1/object/{BUCKET}/{user_id}/{storage_path}", config.url))
}

/// Uploads `bytes` of type `mime` to `storage_path` in `user_id`'s folder, replacing an
/// object at that path. Errors on a file over `MAX_BYTES`, a network failure or a non-2xx.
pub async fn upload(config: &Config, token: &str, user_id: &str, storage_path: &str, mime: &str, bytes: Vec<u8>) -> Result<()> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Config(format!("file is over {} MB", MAX_BYTES / 1024 / 1024)));
    }
    let url = object_url(config, user_id, storage_path)?;
    let response = http()
        .post(url)
        .timeout(UPLOAD_TIMEOUT)
        .header("apikey", &config.anon_key)
        .bearer_auth(token)
        .header("content-type", mime)
        .header("x-upsert", "true")
        .body(bytes)
        .send()
        .await?;
    check(response).await.map(|_| ())
}

/// Most objects one removal request names.
pub const REMOVE_BATCH: usize = 1000;

/// Removes the objects at `storage_paths` in `user_id`'s folder with one bulk request
/// (`DELETE object/goal-files`, body `prefixes`). A path with no object is not an error, so
/// a retry is safe. Errors on a bad path, a network failure or a non-2xx; nothing sent for none.
pub async fn remove(config: &Config, token: &str, user_id: &str, storage_paths: &[String]) -> Result<()> {
    assert!(storage_paths.len() <= REMOVE_BATCH, "callers send at most one batch");
    if storage_paths.is_empty() {
        return Ok(());
    }
    let mut prefixes = Vec::with_capacity(storage_paths.len());
    for path in storage_paths {
        object_url(config, user_id, path)?;
        prefixes.push(format!("{user_id}/{path}"));
    }
    let response = http()
        .delete(format!("{}/storage/v1/object/{BUCKET}", config.url))
        .header("apikey", &config.anon_key)
        .bearer_auth(token)
        .json(&serde_json::json!({ "prefixes": prefixes }))
        .send()
        .await?;
    check(response).await.map(|_| ())
}

/// Whether the project serves the `goal-files` bucket to this session, so Attach can show.
/// Any failure, including a project without Storage, is `false`.
pub async fn available(config: &Config, token: &str) -> bool {
    let request = http().get(format!("{}/storage/v1/bucket/{BUCKET}", config.url)).header("apikey", &config.anon_key).bearer_auth(token);
    matches!(request.send().await, Ok(r) if r.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn storage_once(status: &'static str) -> (Config, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let served = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut seen = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let text = String::from_utf8_lossy(&seen).to_lowercase();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text.lines().find_map(|l| l.strip_prefix("content-length: ")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                    if seen.len() >= end + 4 + length {
                        break;
                    }
                }
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                seen.extend_from_slice(&chunk[..n]);
            }
            let reply = format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{{}}");
            socket.write_all(reply.as_bytes()).await.unwrap();
            seen
        });
        (Config::new(&url, "sb_publishable_test_key_0001").unwrap(), served)
    }

    #[test]
    fn object_urls_stay_in_the_owner_folder() {
        let config = Config::new("https://x.supabase.co", "sb_publishable_test_key_0001").unwrap();
        assert_eq!(object_url(&config, "u1", "goals/g1/f1-plan.pdf").unwrap(), "https://x.supabase.co/storage/v1/object/goal-files/u1/goals/g1/f1-plan.pdf");
        for bad in ["goals/g1/../x", "goals//x", "goals/g1/a b.pdf", "/goals/g1/x", "goals/g1/%2e"] {
            assert!(object_url(&config, "u1", bad).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn upload_posts_the_bytes_with_the_owner_session() {
        let (config, served) = storage_once("200 OK").await;
        upload(&config, "jwt-token", "u1", "goals/g1/f1-plan.pdf", "application/pdf", b"%PDF-1.4 body".to_vec()).await.unwrap();
        let seen = String::from_utf8_lossy(&served.await.unwrap()).to_string();
        let head = seen.to_lowercase();
        assert!(seen.starts_with("POST /storage/v1/object/goal-files/u1/goals/g1/f1-plan.pdf HTTP/1.1"), "{seen}");
        assert!(head.contains("authorization: bearer jwt-token") && head.contains("apikey: sb_publishable_test_key_0001"));
        assert!(head.contains("content-type: application/pdf") && head.contains("x-upsert: true"));
        assert!(seen.ends_with("%PDF-1.4 body"));
    }

    #[tokio::test]
    async fn a_refused_upload_is_an_error_and_too_big_never_sends() {
        let (config, served) = storage_once("403 Forbidden").await;
        let err = upload(&config, "t", "u1", "goals/g1/f1-x.pdf", "application/pdf", vec![1]).await.unwrap_err();
        assert!(matches!(err, Error::Status(403, _)), "{err}");
        served.await.unwrap();
        let config = Config::new("http://127.0.0.1:9", "sb_publishable_test_key_0001").unwrap();
        let err = upload(&config, "t", "u1", "goals/g1/f1-x.pdf", "application/pdf", vec![0; MAX_BYTES + 1]).await.unwrap_err();
        assert!(matches!(err, Error::Config(_)), "{err}");
    }

    #[tokio::test]
    async fn remove_deletes_the_paths_in_the_owner_folder_in_one_request() {
        let (config, served) = storage_once("200 OK").await;
        remove(&config, "jwt-token", "u1", &["goals/g1/a.pdf".to_string(), "goals/g1/b.png".to_string()]).await.unwrap();
        let seen = String::from_utf8_lossy(&served.await.unwrap()).to_string();
        assert!(seen.starts_with("DELETE /storage/v1/object/goal-files HTTP/1.1"), "{seen}");
        assert!(seen.to_lowercase().contains("authorization: bearer jwt-token"));
        assert!(seen.ends_with(r#"{"prefixes":["u1/goals/g1/a.pdf","u1/goals/g1/b.png"]}"#), "{seen}");
        let config = Config::new("http://127.0.0.1:9", "sb_publishable_test_key_0001").unwrap();
        assert!(remove(&config, "t", "u1", &["goals/../x".to_string()]).await.is_err());
        remove(&config, "t", "u1", &[]).await.unwrap();
    }

    #[tokio::test]
    async fn storage_is_available_only_when_the_bucket_answers() {
        let (config, served) = storage_once("200 OK").await;
        assert!(available(&config, "t").await);
        assert!(String::from_utf8_lossy(&served.await.unwrap()).starts_with("GET /storage/v1/bucket/goal-files HTTP/1.1"));
        let (config, served) = storage_once("404 Not Found").await;
        assert!(!available(&config, "t").await);
        served.await.unwrap();
    }
}
