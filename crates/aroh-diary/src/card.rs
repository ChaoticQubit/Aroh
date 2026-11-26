use crate::{invalid, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Most entries one day may hold.
pub const ENTRIES_MAX: usize = 200;
/// Most characters in one entry.
pub const ENTRY_CHARS_MAX: usize = 2000;
/// Most characters in a summary or the notes for tomorrow.
pub const SUMMARY_CHARS_MAX: usize = 1000;
/// Most highlights on a card.
pub const HIGHLIGHTS_MAX: usize = 10;
/// Most characters in one highlight or one food, water or workout line.
pub const LINE_CHARS_MAX: usize = 300;

/// How an entry was made: typed, or dictated on device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Text,
    Voice,
}

/// One diary entry as sealed into a `diary_entries` row: the owner's words
/// verbatim, when and how they were logged. Only the owner changes the words
/// (B120); `at` and `source` stay as first logged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub at: DateTime<Utc>,
    pub text: String,
    pub source: Source,
}

/// One day's structured card as sealed into a `diary_days` row. It is derived
/// from the day's entries; `entry_ids` lists the entries it covers, so an
/// entry missing from it means the card is stale and gets rebuilt. `edited_at`
/// is set once the owner edits the card and is left out of unedited cards, so
/// readers built before B120 still open them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Card {
    pub v: u8,
    pub day: String,
    pub entry_ids: Vec<String>,
    pub summary: String,
    pub mood: u8,
    pub energy: u8,
    pub highlights: Vec<String>,
    pub food: Option<String>,
    pub water: Option<String>,
    pub workout: Option<String>,
    pub notes_for_tomorrow: Option<String>,
    pub structured_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<DateTime<Utc>>,
}

/// The fields of a card the owner edits on the phone (B120). Text is trimmed,
/// an empty food, water, workout or note line clears it and blank highlights
/// are dropped before the card's bounds are checked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardEdit {
    pub summary: String,
    pub mood: u8,
    pub energy: u8,
    pub highlights: Vec<String>,
    pub food: Option<String>,
    pub water: Option<String>,
    pub workout: Option<String>,
    pub notes_for_tomorrow: Option<String>,
}

/// What an agent run may read about one day: never the verbatim entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentDay {
    pub day: String,
    pub summary: String,
    pub mood: u8,
    pub energy: u8,
    pub notes_for_tomorrow: Option<String>,
}

pub(crate) fn chars(field: &'static str, s: &str, min: usize, max: usize) -> Result<()> {
    let n = s.chars().count();
    if n < min || n > max {
        return Err(invalid(field, "length out of range"));
    }
    if s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(invalid(field, "control characters"));
    }
    Ok(())
}

pub(crate) fn opt_chars(field: &'static str, s: &Option<String>, max: usize) -> Result<()> {
    s.as_deref().map_or(Ok(()), |s| chars(field, s, 1, max))
}

pub(crate) fn scale(field: &'static str, n: u8) -> Result<()> {
    if (1..=5).contains(&n) {
        Ok(())
    } else {
        Err(invalid(field, "must be 1 to 5"))
    }
}

/// Checks that `day` is a `YYYY-MM-DD` calendar date.
/// Returns `Invalid` naming `day` otherwise.
pub fn check_day(day: &str) -> Result<()> {
    match NaiveDate::parse_from_str(day, "%Y-%m-%d") {
        Ok(d) if d.format("%Y-%m-%d").to_string() == day => Ok(()),
        _ => Err(invalid("day", "not a YYYY-MM-DD date")),
    }
}

/// Parses `bytes` as strict JSON of `T`. The error never quotes the input,
/// since serde messages can echo values.
pub(crate) fn strict<T: DeserializeOwned>(field: &'static str, bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).map_err(|_| invalid(field, "not the expected JSON"))
}

impl Entry {
    /// Checks the entry: 1 to 2000 characters, no control characters but
    /// newline and tab. Returns `Invalid` naming the field.
    pub fn validate(&self) -> Result<()> {
        chars("text", &self.text, 1, ENTRY_CHARS_MAX)
    }

    /// Serializes the entry for sealing, in zeroizing memory.
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        self.validate().expect("entries are validated before sealing");
        Zeroizing::new(serde_json::to_vec(self).expect("entries serialize"))
    }

    /// Parses and validates an opened entry. Returns `Invalid` on bad JSON or
    /// bounds; the message never quotes the text.
    pub fn from_bytes(bytes: &[u8]) -> Result<Entry> {
        let e: Entry = strict("entry", bytes)?;
        e.validate()?;
        Ok(e)
    }
}

impl Card {
    /// Checks the card: version 1, a real day, at most 200 covered entries,
    /// mood and energy 1 to 5, and every text within its bound.
    /// Returns `Invalid` naming the field.
    pub fn validate(&self) -> Result<()> {
        if self.v != 1 {
            return Err(invalid("v", "unknown card version"));
        }
        check_day(&self.day)?;
        if self.entry_ids.len() > ENTRIES_MAX {
            return Err(invalid("entry_ids", "too many entries"));
        }
        for id in &self.entry_ids {
            match uuid::Uuid::parse_str(id) {
                Ok(u) if u.hyphenated().to_string() == *id => {}
                _ => return Err(invalid("entry_ids", "not a uuid")),
            }
        }
        chars("summary", &self.summary, 1, SUMMARY_CHARS_MAX)?;
        scale("mood", self.mood)?;
        scale("energy", self.energy)?;
        if self.highlights.len() > HIGHLIGHTS_MAX {
            return Err(invalid("highlights", "too many"));
        }
        for h in &self.highlights {
            chars("highlights", h, 1, LINE_CHARS_MAX)?;
        }
        opt_chars("food", &self.food, LINE_CHARS_MAX)?;
        opt_chars("water", &self.water, LINE_CHARS_MAX)?;
        opt_chars("workout", &self.workout, LINE_CHARS_MAX)?;
        opt_chars("notes_for_tomorrow", &self.notes_for_tomorrow, SUMMARY_CHARS_MAX)
    }

    /// Serializes the card for sealing, in zeroizing memory.
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        self.validate().expect("cards are validated before sealing");
        Zeroizing::new(serde_json::to_vec(self).expect("cards serialize"))
    }

    /// Parses and validates an opened card. Returns `Invalid` on bad JSON or
    /// bounds; the message never quotes the card.
    pub fn from_bytes(bytes: &[u8]) -> Result<Card> {
        let c: Card = strict("card", bytes)?;
        c.validate()?;
        Ok(c)
    }

    /// Returns true when an entry in `entry_ids` is not covered by the card,
    /// so the card must be structured again.
    pub fn is_stale(&self, entry_ids: &[String]) -> bool {
        entry_ids.iter().any(|id| !self.entry_ids.contains(id))
    }

    /// Returns this card with the owner's `edit` applied at `at`: the same day,
    /// covered entries and structuring time, `edited_at` set. Returns `Invalid`
    /// naming the field when the edited card breaks a bound.
    pub fn with_edit(&self, edit: CardEdit, at: DateTime<Utc>) -> Result<Card> {
        if edit.highlights.len() > HIGHLIGHTS_MAX * 2 {
            return Err(invalid("highlights", "too many"));
        }
        let line = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let card = Card {
            summary: edit.summary.trim().to_string(),
            mood: edit.mood,
            energy: edit.energy,
            highlights: edit.highlights.into_iter().filter_map(|h| line(Some(h))).collect(),
            food: line(edit.food),
            water: line(edit.water),
            workout: line(edit.workout),
            notes_for_tomorrow: line(edit.notes_for_tomorrow),
            edited_at: Some(at),
            ..self.clone()
        };
        card.validate()?;
        assert_eq!(card.entry_ids, self.entry_ids, "an edit keeps the covered entries");
        Ok(card)
    }

    /// Returns what an agent run may read about this day.
    pub fn for_agents(&self) -> AgentDay {
        AgentDay {
            day: self.day.clone(),
            summary: self.summary.clone(),
            mood: self.mood,
            energy: self.energy,
            notes_for_tomorrow: self.notes_for_tomorrow.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const E1: &str = "0193a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";

    pub(crate) fn card() -> Card {
        Card {
            v: 1,
            day: "2026-11-26".into(),
            entry_ids: vec![E1.into()],
            summary: "A steady day.".into(),
            mood: 4,
            energy: 2,
            highlights: vec!["Piano clicked".into()],
            food: Some("Rice bowl".into()),
            water: None,
            workout: Some("Core, 30 min".into()),
            notes_for_tomorrow: Some("Stretch first.".into()),
            structured_at: "2026-11-26T21:21:00Z".parse().unwrap(),
            edited_at: None,
        }
    }

    fn edit() -> CardEdit {
        CardEdit {
            summary: "  Better than it looked.  ".into(),
            mood: 5,
            energy: 3,
            highlights: vec!["Piano clicked".into(), "  ".into(), "Called mum".into()],
            food: Some("Rice bowl, salad".into()),
            water: Some("   ".into()),
            workout: None,
            notes_for_tomorrow: Some("Sleep by 11.".into()),
        }
    }

    #[test]
    fn an_edit_keeps_the_covered_entries_and_marks_the_card() {
        let at = "2026-11-26T22:00:00Z".parse().unwrap();
        let edited = card().with_edit(edit(), at).unwrap();
        assert_eq!(edited.entry_ids, card().entry_ids);
        assert_eq!(edited.structured_at, card().structured_at);
        assert_eq!(edited.edited_at, Some(at));
        assert_eq!(edited.summary, "Better than it looked.");
        assert_eq!((edited.mood, edited.energy), (5, 3));
        assert_eq!(edited.highlights, vec!["Piano clicked".to_string(), "Called mum".to_string()]);
        assert_eq!(edited.water, None);
        assert_eq!(edited.workout, None);
        assert!(!edited.is_stale(&[E1.into()]));
        assert_eq!(Card::from_bytes(&edited.to_bytes()).unwrap(), edited);
    }

    #[test]
    fn an_edit_keeps_the_card_bounds() {
        let at = "2026-11-26T22:00:00Z".parse().unwrap();
        assert!(card().with_edit(CardEdit { summary: "   ".into(), ..edit() }, at).is_err());
        assert!(card().with_edit(CardEdit { mood: 0, ..edit() }, at).is_err());
        assert!(card().with_edit(CardEdit { energy: 6, ..edit() }, at).is_err());
        assert!(card().with_edit(CardEdit { highlights: vec!["h".into(); HIGHLIGHTS_MAX + 1], ..edit() }, at).is_err());
        assert!(card().with_edit(CardEdit { highlights: vec!["h".repeat(LINE_CHARS_MAX + 1)], ..edit() }, at).is_err());
        assert!(card().with_edit(CardEdit { food: Some("f".repeat(LINE_CHARS_MAX + 1)), ..edit() }, at).is_err());
        assert!(card().with_edit(CardEdit { notes_for_tomorrow: Some("n".repeat(SUMMARY_CHARS_MAX + 1)), ..edit() }, at).is_err());
        assert!(card().with_edit(CardEdit { summary: "bell\u{7}".into(), ..edit() }, at).is_err());
        assert!(card().with_edit(CardEdit { highlights: vec![" ".into(); HIGHLIGHTS_MAX + 1], ..edit() }, at).unwrap().highlights.is_empty());
        assert!(card().with_edit(CardEdit { highlights: vec![" ".into(); HIGHLIGHTS_MAX * 2 + 1], ..edit() }, at).is_err());
    }

    #[test]
    fn cards_before_edits_still_open_and_unedited_cards_serialize_as_before() {
        let old = br#"{"v":1,"day":"2026-11-26","entry_ids":["0193a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b"],"summary":"A steady day.","mood":4,"energy":2,"highlights":["Piano clicked"],"food":"Rice bowl","water":null,"workout":"Core, 30 min","notes_for_tomorrow":"Stretch first.","structured_at":"2026-11-26T21:21:00Z"}"#;
        assert_eq!(Card::from_bytes(old).unwrap(), card());
        assert_eq!(serde_json::to_vec(&card()).unwrap(), old.to_vec());
        let edited = card().with_edit(edit(), "2026-11-26T22:00:00Z".parse().unwrap()).unwrap();
        let text = String::from_utf8(edited.to_bytes().to_vec()).unwrap();
        assert!(text.contains(r#""edited_at":"2026-11-26T22:00:00Z""#));
    }

    #[test]
    fn entry_bounds() {
        let ok = Entry { at: "2026-11-26T07:45:00Z".parse().unwrap(), text: "Slept badly".into(), source: Source::Text };
        assert!(ok.validate().is_ok());
        assert_eq!(Entry::from_bytes(&ok.to_bytes()).unwrap(), ok);
        assert!(Entry { text: String::new(), ..ok.clone() }.validate().is_err());
        assert!(Entry { text: "a".repeat(ENTRY_CHARS_MAX + 1), ..ok.clone() }.validate().is_err());
        assert!(Entry { text: "a".repeat(ENTRY_CHARS_MAX), ..ok.clone() }.validate().is_ok());
        assert!(Entry { text: "bell\u{7}".into(), ..ok }.validate().is_err());
        assert!(Entry::from_bytes(br#"{"at":"2026-11-26T07:45:00Z","text":"x","source":"text","extra":1}"#).is_err());
    }

    #[test]
    fn card_bounds() {
        assert!(card().validate().is_ok());
        assert_eq!(Card::from_bytes(&card().to_bytes()).unwrap(), card());
        assert!(Card { mood: 0, ..card() }.validate().is_err());
        assert!(Card { energy: 6, ..card() }.validate().is_err());
        assert!(Card { day: "2026-13-01".into(), ..card() }.validate().is_err());
        assert!(Card { v: 2, ..card() }.validate().is_err());
        assert!(Card { highlights: vec!["h".into(); HIGHLIGHTS_MAX + 1], ..card() }.validate().is_err());
        assert!(Card { entry_ids: vec![E1.into(); ENTRIES_MAX + 1], ..card() }.validate().is_err());
        assert!(Card { entry_ids: vec!["nope".into()], ..card() }.validate().is_err());
        assert!(Card { summary: "s".repeat(SUMMARY_CHARS_MAX + 1), ..card() }.validate().is_err());
        assert!(Card { food: Some(String::new()), ..card() }.validate().is_err());
    }

    #[test]
    fn stale_when_an_entry_is_not_covered() {
        assert!(!card().is_stale(&[E1.into()]));
        assert!(card().is_stale(&[E1.into(), "0193a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5c".into()]));
    }

    #[test]
    fn agents_never_get_entries() {
        let text = serde_json::to_string(&card().for_agents()).unwrap();
        assert!(!text.contains(E1));
        assert!(!text.contains("Piano"));
        assert!(text.contains("A steady day."));
    }

    #[test]
    fn json_errors_never_quote_the_input() {
        let err = Entry::from_bytes(br#"{"at":"x","text":"SECRET-WORDS","source":"text"}"#).unwrap_err().to_string();
        assert!(!err.contains("SECRET"));
    }
}
