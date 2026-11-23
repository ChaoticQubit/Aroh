//! Offline category detection from goal text and the category hue palette,
//! shared by the server team pick and the app's New Goal form. No model call.

use crate::seed::SEEDED;

/// What a goal's words are about, by keyword. Checked in declaration order,
/// so "learn piano" is `Music` before `Learning`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    Music,
    Finance,
    Fitness,
    Habits,
    Learning,
}

impl Topic {
    /// The seeded category slug a goal about this topic gets.
    /// Music goals belong to Hobby and Skill.
    /// Every topic maps to one seeded slug.
    pub fn slug(self) -> &'static str {
        match self {
            Topic::Music => "hobby-skill",
            Topic::Finance => "financial",
            Topic::Fitness => "health-fitness",
            Topic::Habits => "habit",
            Topic::Learning => "learning",
        }
    }
}

const KEYWORDS: [(Topic, &[&str]); 5] = [
    (
        Topic::Music,
        &[
            "piano", "guitar", "violin", "viola", "cello", "drum", "drums", "ukulele", "flute", "clarinet", "saxophone", "trumpet", "harp", "sing",
            "singing", "song", "songs", "music", "musical", "instrument", "chord", "chords", "sonata", "etude", "nocturne", "concerto", "sheet music",
        ],
    ),
    (
        Topic::Finance,
        &[
            "save", "saving", "savings", "budget", "budgeting", "debt", "debts", "money", "invest", "investing", "investment", "retirement",
            "emergency fund", "spend", "spending", "expenses", "income", "loan", "mortgage", "pay off", "euro", "euros", "dollar", "dollars",
        ],
    ),
    (
        Topic::Fitness,
        &[
            "run", "running", "5k", "10k", "marathon", "gym", "workout", "workouts", "fitness", "abs", "six pack", "muscle", "muscles", "strength",
            "lifting", "weight", "kg", "lbs", "yoga", "swim", "swimming", "cycling", "steps", "push ups", "pull ups", "squat", "squats", "cardio",
            "triathlon", "hiking",
        ],
    ),
    (
        Topic::Habits,
        &[
            "habit", "habits", "routine", "daily", "every day", "every morning", "every night", "meditate", "meditation", "journal", "journaling",
            "floss", "quit", "quitting", "smoking", "sleep", "wake up", "drink water", "screen time", "gratitude", "streak",
        ],
    ),
    (
        Topic::Learning,
        &[
            "learn", "learning", "study", "studying", "course", "class", "exam", "exams", "language", "spanish", "french", "german", "italian",
            "japanese", "chinese", "mandarin", "korean", "read", "reading", "book", "books", "certificate", "certification", "degree", "lesson",
            "lessons", "programming", "coding", "python", "math",
        ],
    ),
];

/// Returns the first topic whose keywords appear as whole words in `text`.
/// Case and punctuation are ignored. `None` when no keyword matches.
pub fn infer(text: &str) -> Option<Topic> {
    let words: String = text.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    let padded = format!(" {} ", words.split_whitespace().collect::<Vec<_>>().join(" "));
    KEYWORDS.iter().find(|(_, words)| words.iter().any(|w| padded.contains(&format!(" {w} ")))).map(|(t, _)| *t)
}

/// Guesses the seeded category slug of a goal from its title and description.
/// Input: the title and optional description. Output: the slug, or `None`
/// when the words match no topic.
pub fn guess(title: &str, description: Option<&str>) -> Option<&'static str> {
    let text = match description {
        Some(d) if !d.trim().is_empty() => format!("{title}\n{d}"),
        _ => title.to_string(),
    };
    infer(&text).map(Topic::slug)
}

/// The 6 category hues, `#rrggbb`, in slot order (the seeded colors).
pub fn hues() -> [&'static str; 6] {
    SEEDED.map(|(_, _, color)| color)
}

/// Returns the first of the 6 hues no category in `used` has, so a new
/// category gets the next free hue. `None` past 6, when the owner picks one.
/// Input: the colors of the owner's live categories.
pub fn next_hue(used: &[&str]) -> Option<&'static str> {
    hues().into_iter().find(|h| !used.iter().any(|u| u.eq_ignore_ascii_case(h)))
}

/// Returns the slot 0..6 of `color` among the 6 hues, ignoring case.
/// `None` for a color outside the palette.
pub fn hue_slot(color: &str) -> Option<usize> {
    hues().iter().position(|h| h.eq_ignore_ascii_case(color))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_the_category_from_title_and_description() {
        assert_eq!(guess("Finish reading Dune by Jan 31", None), Some("learning"));
        assert_eq!(guess("Run a half marathon", None), Some("health-fitness"));
        assert_eq!(guess("Learn Clair de Lune on piano", None), Some("hobby-skill"));
        assert_eq!(guess("Save 5000 euro", None), Some("financial"));
        assert_eq!(guess("Meditate every morning", None), Some("habit"));
        assert_eq!(guess("Ship my side project", None), None);
        assert_eq!(guess("Book club", Some("Finish reading Dune")), Some("learning"));
        assert_eq!(guess("Ship it", Some("  ")), None);
        assert_eq!(guess("RUN!", None), Some("health-fitness"));
        assert_eq!(guess("Unsaved draft", None), None);
    }

    #[test]
    fn next_hue_takes_the_first_free_slot_and_none_past_six() {
        let all = hues();
        assert_eq!(next_hue(&[]), Some(all[0]));
        assert_eq!(next_hue(&[all[0], all[2]]), Some(all[1]));
        assert_eq!(next_hue(&[&all[0].to_uppercase(), all[1]]), Some(all[2]));
        assert_eq!(next_hue(&all), None);
        assert_eq!(next_hue(&["#000000"]), Some(all[0]));
        for (i, h) in all.iter().enumerate() {
            assert_eq!(hue_slot(h), Some(i));
        }
        assert_eq!(hue_slot(&all[3].to_uppercase()), Some(3));
        assert_eq!(hue_slot("#123456"), None);
    }
}
