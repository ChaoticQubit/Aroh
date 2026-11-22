use aroh_domain::seed;
use serde::Serialize;
use troupekit_graph::{NodeKind, Workflow};
use utoipa::ToSchema;
use uuid::Uuid;

/// Persona the `new-goal` workflow names for its research node.
/// `staff` swaps it for the team's research persona.
/// Also the fitness and general research persona.
pub const RESEARCH_SLOT: &str = "goal-researcher";
/// Persona the `new-goal` workflow names for its plan and revise nodes.
/// `staff` swaps it for the team's plan persona.
/// Also the fitness and general plan persona.
pub const PLAN_SLOT: &str = "goal-planner";

/// The kind of goal a team is built for.
/// Serialized in snake case, as the persona list shows it.
/// `General` covers goals no other kind matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    General,
    Fitness,
    Learning,
    Music,
    Finance,
    Habits,
}

/// The personas and template one kind of goal gets: the research and plan
/// personas that fill the workflow slots, the plugin template the builder
/// starts from, and the seeded category slug a new goal of this kind gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Team {
    pub kind: Kind,
    pub category: Option<&'static str>,
    pub research: &'static str,
    pub plan: &'static str,
    pub template: &'static str,
}

/// Every team, one per kind.
/// Each research persona has the research slot's output and each plan
/// persona the plan slot's, so either fits the workflow (tested).
pub const TEAMS: [Team; 6] = [
    Team { kind: Kind::General, category: None, research: RESEARCH_SLOT, plan: PLAN_SLOT, template: "habits" },
    Team { kind: Kind::Fitness, category: Some("health-fitness"), research: RESEARCH_SLOT, plan: PLAN_SLOT, template: "fitness" },
    Team { kind: Kind::Learning, category: Some("learning"), research: "tutor", plan: "practice-planner", template: "learning" },
    Team { kind: Kind::Music, category: Some("hobby-skill"), research: "instrument-teacher", plan: "practice-coach", template: "music" },
    Team { kind: Kind::Finance, category: Some("financial"), research: "budget-coach", plan: "savings-planner", template: "finance" },
    Team { kind: Kind::Habits, category: Some("habit"), research: RESEARCH_SLOT, plan: "habit-coach", template: "habits" },
];

const KEYWORDS: [(Kind, &[&str]); 5] = [
    (
        Kind::Music,
        &[
            "piano", "guitar", "violin", "viola", "cello", "drum", "drums", "ukulele", "flute", "clarinet", "saxophone", "trumpet", "harp", "sing",
            "singing", "song", "songs", "music", "musical", "instrument", "chord", "chords", "sonata", "etude", "nocturne", "concerto", "sheet music",
        ],
    ),
    (
        Kind::Finance,
        &[
            "save", "saving", "savings", "budget", "budgeting", "debt", "debts", "money", "invest", "investing", "investment", "retirement",
            "emergency fund", "spend", "spending", "expenses", "income", "loan", "mortgage", "pay off", "euro", "euros", "dollar", "dollars",
        ],
    ),
    (
        Kind::Fitness,
        &[
            "run", "running", "5k", "10k", "marathon", "gym", "workout", "workouts", "fitness", "abs", "six pack", "muscle", "muscles", "strength",
            "lifting", "weight", "kg", "lbs", "yoga", "swim", "swimming", "cycling", "steps", "push ups", "pull ups", "squat", "squats", "cardio",
            "triathlon", "hiking",
        ],
    ),
    (
        Kind::Habits,
        &[
            "habit", "habits", "routine", "daily", "every day", "every morning", "every night", "meditate", "meditation", "journal", "journaling",
            "floss", "quit", "quitting", "smoking", "sleep", "wake up", "drink water", "screen time", "gratitude", "streak",
        ],
    ),
    (
        Kind::Learning,
        &[
            "learn", "learning", "study", "studying", "course", "class", "exam", "exams", "language", "spanish", "french", "german", "italian",
            "japanese", "chinese", "mandarin", "korean", "read", "reading", "book", "books", "certificate", "certification", "degree", "lesson",
            "lessons", "programming", "coding", "python", "math",
        ],
    ),
];

/// Returns the team of `kind`.
/// Every kind has exactly one team.
/// Linear scan over `TEAMS`.
pub fn team(kind: Kind) -> &'static Team {
    TEAMS.iter().find(|t| t.kind == kind).expect("every kind has a team")
}

fn infer(text: &str) -> Option<Kind> {
    let words: String = text.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    let padded = format!(" {} ", words.split_whitespace().collect::<Vec<_>>().join(" "));
    KEYWORDS.iter().find(|(_, words)| words.iter().any(|w| padded.contains(&format!(" {w} ")))).map(|(k, _)| *k)
}

/// Picks the team of a goal: the seeded category `slug` when there is one
/// (Hobby and Skill is music only when `text` names music), else the first
/// kind whose keywords appear in `text`, else the general team.
pub fn pick(slug: Option<&str>, text: &str) -> &'static Team {
    let kind = match slug {
        Some("health-fitness") => Kind::Fitness,
        Some("learning") => Kind::Learning,
        Some("financial") => Kind::Finance,
        Some("habit") => Kind::Habits,
        Some("career-projects") => Kind::General,
        Some("hobby-skill") if infer(text) == Some(Kind::Music) => Kind::Music,
        Some("hobby-skill") => Kind::General,
        _ => infer(text).unwrap_or(Kind::General),
    };
    team(kind)
}

/// Returns the seeded category slug whose id for `user` is `category_id`.
/// `None` for an owner-defined category or another user's id.
/// Ids come from `seed::categories`, so they match the app's.
pub fn seeded_slug(user: Uuid, category_id: &str) -> Option<&'static str> {
    seed::categories(&user.to_string()).iter().zip(seed::SEEDED.iter()).find(|(c, _)| c.id == category_id).map(|(_, (slug, _, _))| *slug)
}

/// Returns the id of `user`'s seeded category `slug`.
/// `None` for a slug that is not seeded.
/// Same ids as `seed::categories`.
pub fn seeded_id(user: Uuid, slug: &str) -> Option<String> {
    seed::categories(&user.to_string()).into_iter().zip(seed::SEEDED.iter()).find(|(_, (s, _, _))| *s == slug).map(|(c, _)| c.id)
}

/// Fills the persona slots of workflow `wf` with `team`: agent nodes on the
/// research slot get the team's research persona and nodes on the plan slot
/// its plan persona. Other nodes keep theirs.
pub fn staff(wf: &mut Workflow, team: &Team) {
    for node in &mut wf.nodes {
        if let NodeKind::Agent { persona, .. } = &mut node.kind {
            if persona == RESEARCH_SLOT {
                *persona = team.research.to_string();
            } else if persona == PLAN_SLOT {
                *persona = team.plan.to_string();
            }
        }
    }
    let names = crate::workflows::persona_names(wf);
    assert!(names.iter().any(|n| n == team.research) && names.iter().any(|n| n == team.plan), "the slots are staffed");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::{persona_names, workflow, NEW_GOAL};

    #[test]
    fn picks_the_team_from_the_category_then_the_text() {
        let kind = |slug: Option<&str>, text: &str| pick(slug, text).kind;
        assert_eq!(kind(None, "Learn Clair de Lune on piano by March"), Kind::Music);
        assert_eq!(kind(None, "Save 5000 euro for a trip"), Kind::Finance);
        assert_eq!(kind(None, "Run a 5k"), Kind::Fitness);
        assert_eq!(kind(None, "Six pack by Feb 1"), Kind::Fitness);
        assert_eq!(kind(None, "Meditate every morning"), Kind::Habits);
        assert_eq!(kind(None, "Learn Spanish to B1"), Kind::Learning);
        assert_eq!(kind(None, "Ship my side project"), Kind::General);
        assert_eq!(kind(Some("learning"), "Run a 5k"), Kind::Learning);
        assert_eq!(kind(Some("hobby-skill"), "Learn Clair de Lune on piano"), Kind::Music);
        assert_eq!(kind(Some("hobby-skill"), "Learn to paint"), Kind::General);
        assert_eq!(kind(Some("career-projects"), "Read more books"), Kind::General);
        assert_eq!(kind(None, "Unsaved draft"), Kind::General);
    }

    #[test]
    fn seeded_ids_round_trip_per_user() {
        let user = Uuid::now_v7();
        for (slug, _, _) in seed::SEEDED {
            let id = seeded_id(user, slug).unwrap();
            assert_eq!(seeded_slug(user, &id), Some(slug));
            assert_eq!(seeded_slug(Uuid::now_v7(), &id), None);
        }
        assert_eq!(seeded_id(user, "music"), None);
        for t in TEAMS {
            assert!(t.category.is_none_or(|c| seeded_id(user, c).is_some()), "{:?}", t.kind);
            assert!(aroh_plugins::template(t.template).is_some(), "{:?}", t.kind);
        }
    }

    #[test]
    fn staff_swaps_only_the_slots() {
        let mut wf = workflow(NEW_GOAL).unwrap();
        let before = wf.clone();
        staff(&mut wf, team(Kind::Music));
        assert_eq!(persona_names(&wf), ["instrument-teacher", "intake-interviewer", "plugin-builder", "practice-coach"]);
        let persona = |w: &Workflow, id: &str| match &w.nodes.iter().find(|n| n.id == id).unwrap().kind {
            NodeKind::Agent { persona, .. } => persona.clone(),
            _ => unreachable!("agent node"),
        };
        assert_eq!(persona(&wf, "research"), "instrument-teacher");
        assert_eq!(persona(&wf, "plan"), "practice-coach");
        assert_eq!(persona(&wf, "revise"), "practice-coach");
        assert_eq!(wf.edges, before.edges);
        let mut fitness = before.clone();
        staff(&mut fitness, team(Kind::Fitness));
        assert_eq!(fitness, before);
    }
}
