//! The text of an attached image (Q91, Q94): on-device OCR first, the engine's transcription
//! when the OCR is unsure or missing. Pure choices only; the caller runs OCR and calls the
//! engine, and the phone never calls a model provider.

/// Mean OCR confidence at or above which the device keeps the OCR text.
pub const OCR_FLOOR: f64 = 0.75;

/// Most base64 characters of one image sent to the engine; the engine refuses more.
pub const MAX_IMAGE_BASE64: usize = 4_000_000;

/// Put before OCR text kept below `OCR_FLOOR`, so research knows it may be wrong.
pub const LOW_CONFIDENCE: &str = "[Read on device with low confidence] ";

/// Image types the engine accepts.
const SENDABLE: [&str; 4] = ["image/jpeg", "image/png", "image/gif", "image/webp"];

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
    /// Ask the engine for a transcription.
    AskEngine,
}

/// Chooses the next step from `ocr` (none when the platform has no OCR or it failed). Empty
/// text or a confidence below `OCR_FLOOR` (NaN included) is unsure. Output: the step.
pub fn next_step(ocr: Option<&Ocr>) -> Next {
    if ocr.is_some_and(|o| !o.text.trim().is_empty() && o.confidence >= OCR_FLOOR) {
        Next::KeepOcr
    } else {
        Next::AskEngine
    }
}

/// The text to store: sure OCR text as is; else the engine's transcription when it answered
/// (an empty one gives none); else the OCR text marked low confidence; else none.
/// `engine` is the outer none when the engine was not asked or failed, and the inner none
/// when it found no readable text.
pub fn chosen_text(ocr: Option<&Ocr>, engine: Option<Option<&str>>) -> Option<String> {
    if next_step(ocr) == Next::KeepOcr {
        return ocr.map(|o| o.text.clone());
    }
    if let Some(answer) = engine {
        return answer.map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
    }
    ocr.map(|o| o.text.trim()).filter(|t| !t.is_empty()).map(|t| format!("{LOW_CONFIDENCE}{t}"))
}

/// Whether an image of type `mime` whose base64 is `base64_len` characters long can go to
/// the engine as is. Output: true for an accepted type within `MAX_IMAGE_BASE64`.
pub fn sendable(mime: &str, base64_len: usize) -> bool {
    SENDABLE.contains(&mime) && base64_len > 0 && base64_len <= MAX_IMAGE_BASE64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ocr(text: &str, confidence: f64) -> Ocr {
        Ocr { text: text.into(), confidence }
    }

    #[test]
    fn the_threshold_picks_ocr_or_the_engine() {
        assert_eq!(next_step(Some(&ocr("Chapter 1", 0.75))), Next::KeepOcr);
        assert_eq!(next_step(Some(&ocr("Chapter 1", 0.9))), Next::KeepOcr);
        assert_eq!(next_step(Some(&ocr("Chapter 1", 0.7499))), Next::AskEngine);
        assert_eq!(next_step(Some(&ocr("Chapter 1", f64::NAN))), Next::AskEngine);
        assert_eq!(next_step(Some(&ocr("  ", 0.99))), Next::AskEngine);
        assert_eq!(next_step(None), Next::AskEngine);
    }

    #[test]
    fn the_stored_text_follows_the_choice() {
        let sure = ocr("Chapter 1", 0.8);
        assert_eq!(chosen_text(Some(&sure), Some(Some("ignored"))).as_deref(), Some("Chapter 1"));
        let low = ocr(" Chaptr 1 ", 0.4);
        assert_eq!(chosen_text(Some(&low), Some(Some("Chapter 1\nThe joy of x"))).as_deref(), Some("Chapter 1\nThe joy of x"));
        assert_eq!(chosen_text(Some(&low), Some(None)), None, "the engine found no text");
        assert_eq!(chosen_text(Some(&low), None).as_deref(), Some("[Read on device with low confidence] Chaptr 1"));
        assert_eq!(chosen_text(Some(&ocr("", 0.0)), None), None);
        assert_eq!(chosen_text(None, Some(Some("Joy of x"))).as_deref(), Some("Joy of x"));
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
}
