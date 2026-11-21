//! Seeded goal categories (Q6), derived identically by every engine.

use crate::derived_id;
use crate::rows::Category;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rows::{samples, Record};

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
