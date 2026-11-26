//! The text of an attached image (Q91): on-device OCR first, a Claude transcription when the
//! OCR is unsure or missing and the device holds a Claude key. Pure choices and the request
//! body only; the caller runs OCR and sends the request.

use crate::diary::STRUCTURE_MODEL;
use serde_json::{json, Value};

/// Mean OCR confidence at or above which the device keeps the OCR text.
pub const OCR_FLOOR: f64 = 0.75;

/// Most base64 characters of one image sent to Claude, the API's per-image limit.
pub const MAX_IMAGE_BASE64: usize = 5 * 1024 * 1024;

/// What Claude answers for an image without readable text.
pub const NO_TEXT: &str = "NO_TEXT";

/// Put before OCR text kept below `OCR_FLOOR`, so research knows it may be wrong.
pub const LOW_CONFIDENCE: &str = "[Read on device with low confidence] ";

/// Image types the Messages API accepts.
const SENDABLE: [&str; 4] = ["image/jpeg", "image/png", "image/gif", "image/webp"];

/// System prompt of the transcription request.
const TRANSCRIBE_SYSTEM: &str =
    "You transcribe images for a personal planning app. The text you return is stored as the readable content of a file the owner attached to a goal.";

/// User instruction of the transcription request.
const TRANSCRIBE_ASK: &str = "Transcribe all readable text in this image faithfully, in reading order, keeping line breaks. Do not summarize, translate, correct or add commentary. For a diagram, chart or table, transcribe its labels and cells. Answer with the transcription only. If the image has no readable text, answer exactly NO_TEXT.";

/// One on-device OCR result: the top candidate text of every observation and their mean
/// confidence in 0..=1 (0 when nothing was found).
#[derive(Debug, Clone, PartialEq)]
pub struct Ocr {
    pub text: String,
    pub confidence: f64,
}

/// The next step for an image's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// The OCR text is sure enough to keep.
    KeepOcr,
    /// Ask Claude for a transcription.
    AskClaude,
    /// No key: keep whatever OCR read, marked low confidence.
    KeepLow,
}

/// Chooses the next step from `ocr` (none when the platform has no OCR or it failed) and
/// whether a Claude key is held. Empty text or a confidence below `OCR_FLOOR` (NaN included)
/// is unsure. Output: the step.
pub fn next_step(ocr: Option<&Ocr>, has_key: bool) -> Next {
    let sure = ocr.is_some_and(|o| !o.text.trim().is_empty() && o.confidence >= OCR_FLOOR);
    if sure {
        Next::KeepOcr
    } else if has_key {
        Next::AskClaude
    } else {
        Next::KeepLow
    }
}

/// The text to store: sure OCR text as is; else Claude's transcription when it answered
/// (`NO_TEXT` gives none); else the OCR text marked low confidence; else none.
/// `claude` is the transcription, or none when Claude was not asked or failed.
pub fn chosen_text(ocr: Option<&Ocr>, claude: Option<&str>) -> Option<String> {
    if next_step(ocr, false) == Next::KeepOcr {
        return ocr.map(|o| o.text.clone());
    }
    if let Some(t) = claude {
        let t = t.trim();
        return (!t.is_empty() && t != NO_TEXT).then(|| t.to_string());
    }
    ocr.map(|o| o.text.trim()).filter(|t| !t.is_empty()).map(|t| format!("{LOW_CONFIDENCE}{t}"))
}

/// Whether an image of type `mime` whose base64 is `base64_len` characters long can go to
/// the Messages API as is. Output: true for an accepted type within `MAX_IMAGE_BASE64`.
pub fn sendable(mime: &str, base64_len: usize) -> bool {
    SENDABLE.contains(&mime) && base64_len > 0 && base64_len <= MAX_IMAGE_BASE64
}

/// The Messages API body asking Claude to transcribe one image of type `media_type` given as
/// base64 `data`: one user turn, image first, no sampling settings, no prefill. Holds the
/// image, so the caller sends it and drops it without logging.
pub fn transcribe_request(media_type: &str, data: &str) -> Value {
    assert!(sendable(media_type, data.len()), "only an accepted image within the size limit is sent");
    json!({
        "model": STRUCTURE_MODEL,
        "max_tokens": 16000,
        "output_config": {"effort": "medium"},
        "system": TRANSCRIBE_SYSTEM,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "image", "source": {"type": "base64", "media_type": media_type, "data": data}},
                {"type": "text", "text": TRANSCRIBE_ASK},
            ],
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ocr(text: &str, confidence: f64) -> Ocr {
        Ocr { text: text.into(), confidence }
    }

    #[test]
    fn the_threshold_picks_ocr_claude_or_low() {
        assert_eq!(next_step(Some(&ocr("Chapter 1", 0.75)), true), Next::KeepOcr);
        assert_eq!(next_step(Some(&ocr("Chapter 1", 0.9)), false), Next::KeepOcr);
        assert_eq!(next_step(Some(&ocr("Chapter 1", 0.7499)), true), Next::AskClaude);
        assert_eq!(next_step(Some(&ocr("Chapter 1", 0.5)), false), Next::KeepLow);
        assert_eq!(next_step(Some(&ocr("Chapter 1", f64::NAN)), true), Next::AskClaude);
        assert_eq!(next_step(Some(&ocr("  ", 0.99)), true), Next::AskClaude);
        assert_eq!(next_step(Some(&ocr("", 0.0)), false), Next::KeepLow);
        assert_eq!(next_step(None, true), Next::AskClaude);
        assert_eq!(next_step(None, false), Next::KeepLow);
    }

    #[test]
    fn the_stored_text_follows_the_choice() {
        let sure = ocr("Chapter 1", 0.8);
        assert_eq!(chosen_text(Some(&sure), Some("ignored")).as_deref(), Some("Chapter 1"));
        let low = ocr(" Chaptr 1 ", 0.4);
        assert_eq!(
            chosen_text(Some(&low), Some("Chapter 1\nThe joy of x")).as_deref(),
            Some("Chapter 1\nThe joy of x")
        );
        assert_eq!(chosen_text(Some(&low), Some(" NO_TEXT ")), None);
        assert_eq!(chosen_text(Some(&low), None).as_deref(), Some("[Read on device with low confidence] Chaptr 1"));
        assert_eq!(chosen_text(Some(&ocr("", 0.0)), None), None);
        assert_eq!(chosen_text(None, Some("Joy of x")).as_deref(), Some("Joy of x"));
        assert_eq!(chosen_text(None, None), None);
    }

    #[test]
    fn only_accepted_images_within_the_limit_are_sendable() {
        assert!(sendable("image/jpeg", 10));
        assert!(sendable("image/png", MAX_IMAGE_BASE64));
        assert!(!sendable("image/png", MAX_IMAGE_BASE64 + 1));
        assert!(!sendable("image/heic", 10));
        assert!(!sendable("image/jpeg", 0));
    }

    #[test]
    fn the_request_sends_one_image_turn_without_sampling_or_prefill() {
        let body = transcribe_request("image/jpeg", "AAAA");
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["max_tokens"], 16000);
        assert!(body.get("temperature").is_none() && body.get("top_p").is_none() && body.get("thinking").is_none());
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        let image = &messages[0]["content"][0];
        assert_eq!(
            (image["type"].as_str(), image["source"]["media_type"].as_str(), image["source"]["data"].as_str()),
            (Some("image"), Some("image/jpeg"), Some("AAAA"))
        );
        assert!(messages[0]["content"][1]["text"].as_str().unwrap().contains(NO_TEXT));
    }
}
