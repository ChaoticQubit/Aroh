//! On-device reading of goal files: the text of a PDF, plain text or Markdown file, or an
//! image read by OCR or, when the OCR is unsure, by the engine (Q91, Q94), capped at the
//! first `FILE_TEXT_LIMIT` characters, so the server never stores the bytes.

use crate::commands::engine::engine_call_model;
use crate::AppState;
use aroh_app::image_text::{self, Next, Ocr};
use aroh_domain::rows::{IngestStatus, FILE_TEXT_LIMIT};
use base64::Engine;
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Manager};
use tauri_plugin_aroh_ios::IosExt;

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

/// Reads the text of an image of type `mime` from `bytes` (Q91, Q94): Apple Vision OCR on
/// iPhone, then the engine's transcription when the OCR is unsure or missing. Output: the
/// capped text, or none. Never fails: every error falls back to the OCR text or none, and
/// neither the image nor its text is logged.
pub async fn image_text(app: &AppHandle, mime: &str, bytes: &[u8]) -> Option<String> {
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    let ocr = app.ios().call::<OcrAnswer>("image_ocr", json!({ "data": data.as_str() })).await.ok().map(|a| Ocr { text: a.text, confidence: a.confidence });
    let engine = match image_text::next_step(ocr.as_ref()) {
        Next::AskEngine => match engine_image(app, mime, data).await {
            Some((media_type, image)) => transcribe(app, &media_type, image).await,
            None => None,
        },
        Next::KeepOcr => None,
    };
    let engine = engine.as_ref().map(|t| t.as_deref());
    image_text::chosen_text(ocr.as_ref(), engine).map(|t| cap(&t)).filter(|t| !t.is_empty())
}

/// The image to send to the engine, as media type and base64: a downscaled JPEG from Swift,
/// or the original `data` of type `mime` where the platform has no copier. Output: none when
/// neither fits the engine's types and size limit.
async fn engine_image(app: &AppHandle, mime: &str, data: String) -> Option<(String, String)> {
    let copy = app.ios().call::<CopyAnswer>("image_copy", json!({ "data": data.as_str() })).await.ok();
    if let Some(CopyAnswer { jpeg }) = copy {
        return image_text::sendable("image/jpeg", jpeg.len()).then(|| ("image/jpeg".to_string(), jpeg));
    }
    image_text::sendable(mime, data.len()).then(|| (mime.to_string(), data))
}

/// Asks the engine to transcribe one image of `media_type` given as base64 `data`
/// (`POST /v1/files/transcribe`). Output: the outer none when the engine could not answer,
/// the inner none when it found no readable text. The image is dropped after the call.
async fn transcribe(app: &AppHandle, media_type: &str, data: String) -> Option<Option<String>> {
    let state = app.state::<AppState>();
    let body = json!({"media_type": media_type, "data": data});
    let answer = engine_call_model(&state, "/v1/files/transcribe", &body).await.ok()?;
    if answer.status != 200 {
        return None;
    }
    Some(answer.body["text"].as_str().map(str::to_string))
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

    #[test]
    fn header_values_decode_like_encode_uri_component() {
        assert_eq!(percent_decode("Joy%20of%20X%20%C3%A9.pdf").as_deref(), Some("Joy of X é.pdf"));
        assert_eq!(percent_decode("plain").as_deref(), Some("plain"));
        assert_eq!(percent_decode("bad%2"), None);
        assert_eq!(percent_decode("bad%zz"), None);
        assert_eq!(percent_decode("%ff"), None);
    }
}
