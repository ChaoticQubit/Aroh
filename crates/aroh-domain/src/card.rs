//! Action cards (Q71): the structured content of one action (why, workout table,
//! steps, checklist) and the per-set session log, with their bounds.

use crate::{invalid, Result};
use serde::{Deserialize, Serialize};

/// Most blocks on one card.
pub const MAX_BLOCKS: usize = 8;
/// Most rows (exercises, steps, items) in one block.
pub const MAX_ROWS: usize = 30;
/// Most sets of one exercise.
pub const MAX_SETS: usize = 12;
/// Longest text of a card field (why, outline, a row's text).
pub const MAX_TEXT: usize = 2000;
/// Largest reps (or seconds) of one set.
pub const MAX_REPS: u32 = 3600;
/// Largest load of one set, in the exercise's unit.
pub const MAX_WEIGHT: f64 = 2000.0;
/// Longest rest after a set, in seconds.
pub const MAX_REST_S: u32 = 1800;
/// Most logged sets of one session.
pub const MAX_LOGGED: usize = MAX_BLOCKS * MAX_ROWS * MAX_SETS;

/// How much a card holds: `full` in the next two weeks, `outline` (one line) later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Depth {
    Full,
    Outline,
}

/// Load unit of an exercise; `None` on the exercise means bodyweight or seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Kg,
    Lb,
}

/// One planned set: reps (or seconds) and load, `weight` null for bodyweight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetPlan {
    pub reps: u32,
    pub weight: Option<f64>,
}

/// One exercise row of a workout block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exercise {
    pub name: String,
    pub unit: Option<Unit>,
    pub rest_s: u32,
    pub cue: Option<String>,
    pub sets: Vec<SetPlan>,
}

/// One ordered step of a steps block, with its minutes when known.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub text: String,
    pub minutes: Option<u32>,
}

/// One checklist row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub text: String,
}

/// A structured block: a workout table, ordered steps (a recipe), or a checklist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Block {
    Workout { exercises: Vec<Exercise> },
    Steps { title: String, items: Vec<Step> },
    Checklist { title: String, items: Vec<Item> },
}

/// The card of one action or the outline template of a series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Card {
    pub v: u8,
    pub depth: Depth,
    pub why: String,
    pub blocks: Vec<Block>,
    pub outline: Option<String>,
}

/// How the session felt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Easy,
    Ok,
    Hard,
}

/// One logged set, addressed by block, row and set index of the card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetLog {
    pub block: u16,
    pub row: u16,
    pub set: u16,
    pub reps: u32,
    pub weight: Option<f64>,
    pub done: bool,
}

/// One ticked checklist row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checked {
    pub block: u16,
    pub row: u16,
}

fn text(field: &'static str, s: &str, min: usize) -> Result<()> {
    let n = s.chars().count();
    if n < min || n > MAX_TEXT {
        return Err(invalid(field, format!("{min} to {MAX_TEXT} characters")));
    }
    Ok(())
}

fn weight(field: &'static str, w: Option<f64>) -> Result<()> {
    match w {
        Some(w) if !(w.is_finite() && (0.0..=MAX_WEIGHT).contains(&w)) => Err(invalid(field, format!("0 to {MAX_WEIGHT}"))),
        _ => Ok(()),
    }
}

impl Card {
    /// Checks version, bounds and text lengths. A `full` card needs a why;
    /// an `outline` card needs its outline line or blocks. Returns `Invalid` naming the first bad value.
    pub fn check(&self) -> Result<()> {
        if self.v != 1 {
            return Err(invalid("card.v", "version 1"));
        }
        if self.blocks.len() > MAX_BLOCKS {
            return Err(invalid("card.blocks", format!("at most {MAX_BLOCKS}")));
        }
        match self.depth {
            Depth::Full => text("card.why", &self.why, 1)?,
            Depth::Outline if self.blocks.is_empty() => text("card.outline", self.outline.as_deref().unwrap_or(""), 1)?,
            Depth::Outline => {}
        }
        text("card.why", &self.why, 0)?;
        if let Some(o) = &self.outline {
            text("card.outline", o, 0)?;
        }
        for b in &self.blocks {
            b.check()?;
        }
        Ok(())
    }

    /// The outline copy of this card for a newly materialized occurrence: same
    /// content, depth `outline`, so the weekly fill knows it still has to write it.
    pub fn as_outline(&self) -> Card {
        Card { depth: Depth::Outline, ..self.clone() }
    }
}

impl Block {
    fn check(&self) -> Result<()> {
        match self {
            Block::Workout { exercises } => {
                if exercises.len() > MAX_ROWS {
                    return Err(invalid("card.exercises", format!("at most {MAX_ROWS}")));
                }
                for e in exercises {
                    text("card.exercise.name", &e.name, 1)?;
                    if let Some(c) = &e.cue {
                        text("card.exercise.cue", c, 0)?;
                    }
                    if e.rest_s > MAX_REST_S {
                        return Err(invalid("card.exercise.rest_s", format!("at most {MAX_REST_S}")));
                    }
                    if e.sets.is_empty() || e.sets.len() > MAX_SETS {
                        return Err(invalid("card.exercise.sets", format!("1 to {MAX_SETS}")));
                    }
                    for s in &e.sets {
                        if s.reps > MAX_REPS {
                            return Err(invalid("card.set.reps", format!("at most {MAX_REPS}")));
                        }
                        weight("card.set.weight", s.weight)?;
                    }
                }
                Ok(())
            }
            Block::Steps { title, items } => {
                text("card.block.title", title, 0)?;
                if items.len() > MAX_ROWS {
                    return Err(invalid("card.steps", format!("at most {MAX_ROWS}")));
                }
                items.iter().try_for_each(|s| text("card.step.text", &s.text, 0))
            }
            Block::Checklist { title, items } => {
                text("card.block.title", title, 0)?;
                if items.len() > MAX_ROWS {
                    return Err(invalid("card.items", format!("at most {MAX_ROWS}")));
                }
                items.iter().try_for_each(|i| text("card.item.text", &i.text, 0))
            }
        }
    }
}

/// Checks one session log's sets and ticked rows against the bounds.
/// Returns `Invalid` naming the first bad value.
pub fn check_log(sets: &[SetLog], checked: &[Checked]) -> Result<()> {
    if sets.len() > MAX_LOGGED || checked.len() > MAX_BLOCKS * MAX_ROWS {
        return Err(invalid("action_logs.sets", "too many rows"));
    }
    for s in sets {
        if usize::from(s.block) >= MAX_BLOCKS || usize::from(s.row) >= MAX_ROWS || usize::from(s.set) >= MAX_SETS {
            return Err(invalid("action_logs.sets", "index out of range"));
        }
        if s.reps > MAX_REPS {
            return Err(invalid("action_logs.sets", format!("reps at most {MAX_REPS}")));
        }
        weight("action_logs.sets", s.weight)?;
    }
    if checked.iter().any(|c| usize::from(c.block) >= MAX_BLOCKS || usize::from(c.row) >= MAX_ROWS) {
        return Err(invalid("action_logs.checked", "index out of range"));
    }
    Ok(())
}

/// Returns the id of the one session log of `action_id`: derived, so every device
/// writes the same row and LWW keeps one log per session.
pub fn log_id(action_id: &str) -> String {
    let millis = uuid::Uuid::parse_str(action_id).ok().and_then(|u| u.get_timestamp()).map_or(0, |t| {
        let (secs, nanos) = t.to_unix();
        secs * 1000 + u64::from(nanos) / 1_000_000
    });
    crate::derived_id("action_logs", action_id, millis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn leg_day() -> Card {
        serde_json::from_value(json!({
            "v": 1, "depth": "full", "why": "Legs.", "outline": null,
            "blocks": [
                {"kind": "workout", "exercises": [{"name": "Back squat", "unit": "kg", "rest_s": 120, "cue": null, "sets": [{"reps": 8, "weight": 62.5}]}]},
                {"kind": "steps", "title": "Warm up", "items": [{"text": "Bike", "minutes": 5}]},
                {"kind": "checklist", "title": "Bring", "items": [{"text": "Belt"}]}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn a_card_round_trips_and_checks() {
        let c = leg_day();
        c.check().unwrap();
        let back: Card = serde_json::from_value(serde_json::to_value(&c).unwrap()).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn bad_cards_name_the_field() {
        let mut c = leg_day();
        c.why = String::new();
        assert!(matches!(c.check(), Err(crate::Error::Invalid { field: "card.why", .. })));
        let mut c = leg_day();
        if let Block::Workout { exercises } = &mut c.blocks[0] {
            exercises[0].sets[0].weight = Some(-1.0);
        }
        assert!(matches!(c.check(), Err(crate::Error::Invalid { field: "card.set.weight", .. })));
        let mut c = leg_day();
        if let Block::Workout { exercises } = &mut c.blocks[0] {
            exercises[0].sets.clear();
        }
        assert!(matches!(c.check(), Err(crate::Error::Invalid { field: "card.exercise.sets", .. })));
        let outline = Card { depth: Depth::Outline, outline: None, blocks: vec![], ..leg_day() };
        assert!(matches!(outline.check(), Err(crate::Error::Invalid { field: "card.outline", .. })));
        Card { depth: Depth::Outline, outline: None, ..leg_day() }.check().unwrap();
        assert!(serde_json::from_value::<Card>(json!({"v": 1, "depth": "full", "why": "x", "blocks": [], "outline": null, "extra": 1})).is_err());
    }

    #[test]
    fn outline_copy_keeps_content() {
        let o = leg_day().as_outline();
        assert_eq!(o.depth, Depth::Outline);
        assert_eq!(o.blocks, leg_day().blocks);
    }

    #[test]
    fn logs_check_indexes_and_ids_are_stable() {
        let ok = [SetLog { block: 0, row: 0, set: 0, reps: 8, weight: Some(62.5), done: true }];
        check_log(&ok, &[Checked { block: 1, row: 0 }]).unwrap();
        let bad = [SetLog { block: 0, row: 0, set: MAX_SETS as u16, reps: 8, weight: None, done: true }];
        assert!(check_log(&bad, &[]).is_err());
        let a = "0195f0a0-0000-7000-8000-000000000001";
        assert_eq!(log_id(a), log_id(a));
        assert_ne!(log_id(a), log_id("0195f0a0-0000-7000-8000-000000000002"));
        uuid::Uuid::parse_str(&log_id(a)).unwrap();
    }
}
