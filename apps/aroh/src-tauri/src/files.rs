//! On-device reading of goal files: the text of a PDF, plain text or Markdown file, capped
//! at the first `FILE_TEXT_LIMIT` characters, so the server never needs the bytes.

use aroh_domain::rows::{IngestStatus, FILE_TEXT_LIMIT};

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
