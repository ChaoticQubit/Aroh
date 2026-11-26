//! On-device reading of goal files: the text of a PDF, plain text or Markdown file, or an
//! image read by OCR or Claude (Q91), capped at the first
//! `FILE_TEXT_LIMIT` characters, so the server never needs the bytes.

use aroh_app::diary::{reply_text, ANTHROPIC_URL, ANTHROPIC_VERSION};
use aroh_app::image_text::{self, Next, Ocr};
use aroh_domain::rows::{IngestStatus, FILE_TEXT_LIMIT};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Runtime};
use tauri_plugin_aroh_ios::IosExt;

/// Longest wait for one Claude transcription.
const TRANSCRIBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// The answer of the Swift `image_ocr` command.
#[derive(Deserialize)]
struct OcrAnswer {
    text: String,
    confidence: f64,
}

/// The answer of the Swift `image_copy` command: a downscaled JPEG as base64.
#[derive(Deserialize)]
struct CopyAnswer {
    jpeg: String,
}

/// Reads the text of an image of type `mime` from `bytes` (Q91): Apple Vision OCR on iPhone,
/// then a Claude transcription with the device key when the OCR is unsure or missing.
/// Output: the capped text, or none. Never fails: every error falls back to the OCR text or
/// none, and neither the image, its text nor the key is logged.
pub async fn image_text<R: Runtime>(app: &AppHandle<R>, mime: &str, bytes: &[u8]) -> Option<String> {
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    let ocr = app.ios().call::<OcrAnswer>("image_ocr", json!({ "data": data.as_str() })).await.ok().map(|a| Ocr { text: a.text, confidence: a.confidence });
    let key = crate::commands::diary::claude_key();
    let claude = match (image_text::next_step(ocr.as_ref(), key.is_some()), key) {
        (Next::AskClaude, Some(key)) => match claude_image(app, mime, data).await {
            Some((media_type, image)) => transcribe(ANTHROPIC_URL, &key, &image_text::transcribe_request(&media_type, &image)).await.ok(),
            None => None,
        },
        _ => None,
    };
    image_text::chosen_text(ocr.as_ref(), claude.as_deref()).map(|t| cap(&t)).filter(|t| !t.is_empty())
}

/// The image to send to Claude, as media type and base64: a downscaled JPEG from Swift, or
/// the original `data` of type `mime` where the platform has no copier. Output: none when
/// neither fits the API's types and size limit.
async fn claude_image<R: Runtime>(app: &AppHandle<R>, mime: &str, data: String) -> Option<(String, String)> {
    let copy = app.ios().call::<CopyAnswer>("image_copy", json!({ "data": data.as_str() })).await.ok();
    if let Some(CopyAnswer { jpeg }) = copy {
        return image_text::sendable("image/jpeg", jpeg.len()).then(|| ("image/jpeg".to_string(), jpeg));
    }
    image_text::sendable(mime, data.len()).then(|| (mime.to_string(), data))
}

/// Sends transcription `body` to the Messages endpoint `url` with Claude key `key`.
/// Output: the reply text, or a fixed reason that never carries the image, text or key.
pub async fn transcribe(url: &str, key: &str, body: &Value) -> Result<String, String> {
    let http = reqwest::Client::builder().timeout(TRANSCRIBE_TIMEOUT).build().map_err(|_| "no HTTP client".to_string())?;
    let response = http
        .post(url)
        .header("x-api-key", key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(body)
        .send()
        .await
        .map_err(|_| "the model could not be reached".to_string())?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(format!("the model answered {status}"));
    }
    let reply: Value = response.json().await.map_err(|_| "the model reply was not JSON".to_string())?;
    reply_text(&reply).map_err(|e| e.to_string())
}

/// Reads the text of a file named `name` of type `mime` from `bytes`. Call it off the main
/// thread: a long PDF takes a while. Output: `Done` with the text (none for an image or a
/// type without text), or `Failed` when a PDF or text file cannot be read. Never panics.
pub fn extract(mime: &str, name: &str, bytes: &[u8]) -> (IngestStatus, Option<String>) {
    let lower = name.to_ascii_lowercase();
    let pdf = mime == "application/pdf" || lower.ends_with(".pdf");
    let text = mime.starts_with("text/") || [".txt", ".md", ".markdown"].iter().any(|e| lower.ends_with(e));
    let read = if pdf {
        std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(bytes)).ok().and_then(Result::ok)
    } else if text {
        std::str::from_utf8(bytes).ok().map(str::to_string)
    } else {
        return (IngestStatus::Done, None);
    };
    match read.map(|t| cap(&t)).filter(|t| !t.is_empty()) {
        Some(t) => (IngestStatus::Done, Some(t)),
        None => (IngestStatus::Failed, None),
    }
}

/// The trimmed text with whitespace runs folded, cut to the first `FILE_TEXT_LIMIT` characters.
fn cap(text: &str) -> String {
    let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let out: String = folded.chars().take(FILE_TEXT_LIMIT).collect();
    assert!(out.chars().count() <= FILE_TEXT_LIMIT);
    out
}

/// Decodes a `%XX` escaped header value, as `encodeURIComponent` writes it.
/// Output: the text, or `None` for a bad escape or bytes that are not UTF-8.
pub fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAPTER: &[u8] = include_bytes!("../tests/fixtures/chapter.pdf");

    #[test]
    fn a_small_pdf_gives_its_text() {
        let (status, text) = extract("application/pdf", "chapter.pdf", CHAPTER);
        assert_eq!(status, IngestStatus::Done);
        assert!(text.unwrap().contains("Chapter one: the joy of x"));
    }

    #[test]
    fn a_malformed_pdf_fails_without_a_panic() {
        let mut broken = CHAPTER[..CHAPTER.len() / 2].to_vec();
        broken.extend_from_slice(b"garbage");
        assert_eq!(extract("application/pdf", "broken.pdf", &broken), (IngestStatus::Failed, None));
        assert_eq!(extract("application/pdf", "x.pdf", b"not a pdf at all"), (IngestStatus::Failed, None));
        assert_eq!(extract("application/pdf", "x.pdf", b""), (IngestStatus::Failed, None));
    }

    #[test]
    fn text_files_are_capped_and_images_keep_only_their_name() {
        let long = "word ".repeat(FILE_TEXT_LIMIT);
        let (status, text) = extract("text/markdown", "notes.md", long.as_bytes());
        assert_eq!((status, text.as_ref().map(|t| t.chars().count())), (IngestStatus::Done, Some(FILE_TEXT_LIMIT)));
        assert_eq!(extract("application/octet-stream", "notes.txt", b"  read on\n\nthe train ").1.as_deref(), Some("read on the train"));
        assert_eq!(extract("text/plain", "bad.txt", &[0xff, 0xfe]), (IngestStatus::Failed, None));
        assert_eq!(extract("image/jpeg", "shelf.jpg", &[1, 2, 3]), (IngestStatus::Done, None));
    }

    /// Serves one HTTP request on a local port with `status` and `reply`. Output: the base
    /// URL and a handle that yields the request head and its JSON body.
    fn capture_one(status: u16, reply: &'static str) -> (String, std::thread::JoinHandle<(String, Value)>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            let (head, body) = loop {
                let n = sock.read(&mut chunk).unwrap();
                assert!(n > 0, "client closed before sending a body");
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf).into_owned();
                let Some((head, body)) = text.split_once("\r\n\r\n") else { continue };
                let length: usize = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap())).unwrap();
                if body.len() >= length {
                    break (head.to_string(), body[..length].to_string());
                }
            };
            let response = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
            sock.write_all(response.as_bytes()).unwrap();
            (head, serde_json::from_str(&body).unwrap())
        });
        (base, task)
    }

    const REPLY: &str = r#"{"id":"msg_1","type":"message","role":"assistant","model":"claude-opus-5-5","content":[{"type":"text","text":"Chapter 1\nThe joy of x"}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":3,"output_tokens":4}}"#;

    #[test]
    fn a_transcription_posts_one_image_turn_with_the_key() {
        let (base, server) = capture_one(200, REPLY);
        let body = aroh_app::image_text::transcribe_request("image/jpeg", "/9j/AAAA");
        let text = tauri::async_runtime::block_on(transcribe(&format!("{base}/v1/messages"), "test-key", &body));
        assert_eq!(text.as_deref(), Ok("Chapter 1\nThe joy of x"));
        let (head, sent) = server.join().unwrap();
        let head = head.to_ascii_lowercase();
        assert!(head.starts_with("post /v1/messages "));
        assert!(head.contains("x-api-key: test-key") && head.contains(&format!("anthropic-version: {}", ANTHROPIC_VERSION.to_ascii_lowercase())));
        assert_eq!((sent["model"].as_str(), sent["max_tokens"].as_i64()), (Some("claude-opus-5-5"), Some(16000)));
        assert!(sent.get("temperature").is_none());
        let last = sent["messages"].as_array().unwrap().last().unwrap().clone();
        assert_eq!(last["role"], "user");
        assert_eq!((last["content"][0]["type"].as_str(), last["content"][0]["source"]["media_type"].as_str()), (Some("image"), Some("image/jpeg")));
    }

    #[test]
    fn a_failed_or_refused_transcription_names_only_the_reason() {
        let (base, server) = capture_one(529, r#"{"type":"error"}"#);
        let body = aroh_app::image_text::transcribe_request("image/png", "iVBO");
        let out = tauri::async_runtime::block_on(transcribe(&base, "test-key", &body));
        assert_eq!(out, Err("the model answered 529".to_string()));
        server.join().unwrap();
        let refusal = r#"{"content":[],"stop_reason":"refusal"}"#;
        let (base, server) = capture_one(200, refusal);
        let out = tauri::async_runtime::block_on(transcribe(&base, "test-key", &body)).unwrap_err();
        assert!(!out.contains("test-key"));
        server.join().unwrap();
    }

    #[test]
    fn header_values_decode_like_encode_uri_component() {
        assert_eq!(percent_decode("Joy%20of%20X%20%C3%A9.pdf").as_deref(), Some("Joy of X é.pdf"));
        assert_eq!(percent_decode("plain").as_deref(), Some("plain"));
        assert_eq!(percent_decode("bad%2"), None);
        assert_eq!(percent_decode("bad%zz"), None);
        assert_eq!(percent_decode("%ff"), None);
    }
}
