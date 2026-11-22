use crate::error::ServerError;
use crate::rows::list_rows;
use aroh_domain::rows::Persona as PersonaRow;
use crate::db::PgPool;
use std::collections::BTreeMap;
use std::sync::Arc;
use troupekit_agent::{builtin, MemorySource, PersonaLoader, PersonaSource};
use troupekit_graph::{NodeKind, Workflow};
use uuid::Uuid;

/// Workflow name of goal setup.
/// Intake, questions, research, plan, approval, then write.
/// Started by `POST /v1/goals`.
pub const NEW_GOAL: &str = "new-goal";
/// Workflow name of adaptation.
/// Proposal, autonomy gate, approval, then apply or reject.
/// Started by `POST /v1/goals/{goal_id}/replan`.
pub const REPLAN: &str = "replan";
/// Workflow name of free-text logging.
/// Maps the text to tracker entries and stores them.
/// Started by `POST /v1/logs`.
pub const LOG_INTAKE: &str = "log-intake";
/// Workflow name of learning from edits.
/// The preference analyst proposes one preference from similar edits.
/// Started by the trigger scan after 3 or more similar owner edits.
pub const LEARN_PREFERENCE: &str = "learn-preference";
/// Workflow name of device calendar triage.
/// Groups events, asks keep or ignore per group, then writes rules.
/// Started by `POST /v1/calendar/triage`.
pub const CALENDAR_TRIAGE: &str = "calendar-triage";

const WORKFLOWS: [(&str, &str); 5] = [
    (NEW_GOAL, include_str!("../workflows/new-goal.toml")),
    (REPLAN, include_str!("../workflows/replan.toml")),
    (LOG_INTAKE, include_str!("../workflows/log-intake.toml")),
    (LEARN_PREFERENCE, include_str!("../workflows/learn-preference.toml")),
    (CALENDAR_TRIAGE, include_str!("../workflows/calendar-triage.toml")),
];

const PERSONAS: [(&str, &str); 8] = [
    ("intake-interviewer", include_str!("../personas/intake-interviewer.toml")),
    ("goal-researcher", include_str!("../personas/goal-researcher.toml")),
    ("goal-planner", include_str!("../personas/goal-planner.toml")),
    ("adaptation-analyst", include_str!("../personas/adaptation-analyst.toml")),
    ("log-intake", include_str!("../personas/log-intake.toml")),
    ("calendar-triage", include_str!("../personas/calendar-triage.toml")),
    ("plugin-builder", include_str!("../personas/plugin-builder.toml")),
    ("preference-analyst", include_str!("../personas/preference-analyst.toml")),
];

/// Parses the built-in workflow called `name`.
/// Returns `NotFound` for an unknown name and `Config` when the shipped
/// TOML does not validate (a build defect the workflow test catches).
pub fn workflow(name: &str) -> Result<Workflow, ServerError> {
    let (file, text) = WORKFLOWS
        .iter()
        .find(|(n, _)| *n == name)
        .ok_or_else(|| ServerError::NotFound(format!("workflow {name}")))?;
    Workflow::parse(&format!("{file}.toml"), text).map_err(|e| ServerError::Config(e.to_string()))
}

/// The persona files shipped with the server engine, labelled `aroh`.
/// Searched after the user's own personas and before Troupekit's built-ins.
/// Returns the source.
pub fn aroh_personas() -> MemorySource {
    MemorySource::new("aroh", PERSONAS.iter().map(|(n, t)| (format!("{n}.toml"), (*t).to_string())))
}

/// Returns the persona names the agent nodes of `wf` use, sorted, once each.
/// The snapshot of a run holds exactly these.
/// Empty for a workflow without agent nodes.
pub fn persona_names(wf: &Workflow) -> Vec<String> {
    let mut names: Vec<String> = wf
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Agent { persona, .. } => Some(persona.clone()),
            _ => None,
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Takes the persona snapshot of a new run of `wf` for `user` and `goal`:
/// each persona from the goal folder, else the user library, else the Aroh
/// built-ins, else Troupekit's. Returns `Setup` when a persona is missing.
pub async fn snapshot(
    pool: &PgPool,
    user: Uuid,
    goal: Option<Uuid>,
    wf: &Workflow,
) -> Result<BTreeMap<String, String>, ServerError> {
    let rows = list_rows::<PersonaRow>(pool, user, &[], false).await?;
    let goal_text = goal.map(|g| g.to_string());
    let shipped: [Arc<dyn PersonaSource>; 2] = [Arc::new(aroh_personas()), Arc::new(builtin())];
    let mut out = BTreeMap::new();
    for name in persona_names(wf) {
        let own = rows
            .iter()
            .find(|p| p.name == name && p.goal_id.is_some() && p.goal_id == goal_text)
            .or_else(|| rows.iter().find(|p| p.name == name && p.goal_id.is_none()))
            .map(|p| p.toml.clone());
        let text = match own {
            Some(t) => Some(t),
            None => {
                let mut found = None;
                for src in &shipped {
                    if let Some(t) = src.read(&format!("{name}.toml")).await.map_err(|e| ServerError::Setup(e.to_string()))? {
                        found = Some(t);
                        break;
                    }
                }
                found
            }
        };
        let text = text.ok_or_else(|| ServerError::Setup(format!("persona `{name}` not found")))?;
        out.insert(name, text);
    }
    Ok(out)
}

/// Builds the persona loader of a run from its snapshot only, so a persona
/// edited after the run started never reaches it.
/// Returns the loader.
pub fn loader(snapshot: &BTreeMap<String, String>) -> PersonaLoader {
    let files = snapshot.iter().map(|(n, t)| (format!("{n}.toml"), t.clone()));
    PersonaLoader::new(vec![Arc::new(MemorySource::new("run", files))])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn workflows_parse_and_personas_load() {
        let loader = PersonaLoader::new(vec![Arc::new(aroh_personas())]);
        for (name, _) in WORKFLOWS {
            let wf = workflow(name).unwrap();
            assert_eq!(wf.name, name);
            for p in persona_names(&wf) {
                let persona = loader.load(&p).await.unwrap_or_else(|e| panic!("{p}: {e}"));
                assert_eq!(persona.model, "default");
            }
        }
        assert_eq!(persona_names(&workflow(NEW_GOAL).unwrap()), ["goal-planner", "goal-researcher", "intake-interviewer", "plugin-builder"]);
        assert!(workflow("nope").is_err());
    }
}
