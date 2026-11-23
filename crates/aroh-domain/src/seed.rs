//! Seeded goal categories (Q6), derived identically by every engine.

use crate::derived_id;
use crate::rows::{Autonomy, Category, Goal, GoalStatus};
use chrono::{DateTime, Utc};

/// Stamp of derived rows (seeds and materialized occurrences): the lowest HLC, so any edit by the owner wins under LWW,
/// and a zero node that no device has, so no device ever pushes them.
pub const SEED_STAMP: &str = "0000000000000-0000-0000000000000000";

/// Slug, name and color of each seeded category, in display order.
pub const SEEDED: [(&str, &str, &str); 6] = [
    ("health-fitness", "Health and Fitness", "#4f7a5a"),
    ("learning", "Learning", "#4a6fa5"),
    ("hobby-skill", "Hobby and Skill", "#8a6a9a"),
    ("financial", "Financial", "#a5803a"),
    ("career-projects", "Career and Projects", "#5a6b7a"),
    ("habit", "Habit", "#7a5a4f"),
];

/// Returns the seeded categories of `user_id` with deterministic ids, so the app and
/// server engines insert the same rows (insert-or-ignore) without duplicates.
pub fn categories(user_id: &str) -> Vec<Category> {
    SEEDED
        .iter()
        .map(|(slug, name, color)| Category {
            id: derived_id(user_id, slug, 0),
            user_id: user_id.to_string(),
            name: (*name).to_string(),
            color: (*color).to_string(),
            seeded: true,
            updated_at: SEED_STAMP.to_string(),
            deleted: false,
        })
        .collect()
}

/// The id of the built-in Routines goal (Q56). One fixed id for every user, so a device that
/// adopts an account and the account's other devices collapse onto one Routines goal under LWW.
pub fn routines_id() -> String {
    derived_id("aroh", "goal:routines", 0)
}

/// The built-in Routines goal of `user_id` in the seeded Habit category: autonomy Ask,
/// a fixed creation instant and [`SEED_STAMP`]. The app writes it with its own stamp on
/// first use, so it syncs like any goal the owner made.
pub fn routines_goal(user_id: &str) -> Goal {
    let habit = categories(user_id).into_iter().find(|c| c.name == "Habit").map(|c| c.id);
    assert!(habit.is_some(), "the Habit category is seeded");
    Goal {
        id: routines_id(),
        user_id: user_id.to_string(),
        category_id: habit,
        title: "Routines".into(),
        context: Some("Routines Aroh learned from your places, approved by you.".into()),
        deadline: None,
        status: GoalStatus::Active,
        autonomy: Autonomy::Ask,
        created_at: DateTime::<Utc>::UNIX_EPOCH,
        updated_at: SEED_STAMP.to_string(),
        deleted: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rows::{samples, Record};

    #[test]
    fn routines_goal_is_stable_per_user_and_valid() {
        let a = routines_goal(samples::USER);
        assert_eq!(a, routines_goal(samples::USER));
        a.validate().unwrap();
        assert_eq!((a.title.as_str(), a.autonomy), ("Routines", Autonomy::Ask));
        let habit = categories(samples::USER).into_iter().find(|c| c.name == "Habit").unwrap();
        assert_eq!(a.category_id.as_deref(), Some(habit.id.as_str()));
        let b = routines_goal("0190f2a0-0000-7000-8000-00000000000b");
        assert_eq!(a.id, b.id);
        assert_ne!(a.category_id, b.category_id);
    }

    #[test]
    fn seeds_are_stable_per_user_and_valid() {
        let a = categories(samples::USER);
        assert_eq!(a, categories(samples::USER));
        assert_eq!(a.len(), 6);
        let b = categories("0190f2a0-0000-7000-8000-00000000000b");
        assert!(a.iter().zip(&b).all(|(x, y)| x.id != y.id));
        assert!(a.iter().all(|c| c.validate().is_ok()));
        assert!(crate::hlc::parse(SEED_STAMP).is_ok());
    }
}
