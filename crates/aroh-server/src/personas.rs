use crate::error::ServerError;
use crate::plugins::{Writer, PLUGIN_TOOLS};
use crate::rows::{list_rows, put_row};
use crate::teams::{Kind, TEAMS};
use crate::workflows::{shipped, PERSONAS};
use aroh_domain::rows::Persona as Row;
use serde::{Deserialize, Serialize};
use troupekit_agent::Persona;
use utoipa::ToSchema;

/// Prefix of a safety guardrail (health or finance rules). An owner's
/// edited copy must keep every safety guardrail of its built-in.
/// Checked by `check`.
pub const SAFETY: &str = "Safety:";

/// Body of `PUT /v1/personas/{name}`: the full persona TOML that replaces
/// the owner's library copy.
/// Validated before it is stored.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EditPersona {
    pub toml: String,
}

/// Where a listed persona's text comes from: the server's shipped file or
/// the owner's library copy, which runs use instead.
/// Serialized in snake case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Builtin,
    Library,
}

/// One persona as the library lists it: name, display name and role from
/// its text, the team kinds that use it for research or planning, where
/// the text comes from, whether a built-in exists, and the TOML itself.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PersonaView {
    pub name: String,
    pub display_name: String,
    pub role: String,
    pub teams: Vec<Kind>,
    pub source: Origin,
    pub builtin: bool,
    pub toml: String,
}

fn invalid(e: impl std::fmt::Display) -> ServerError {
    ServerError::Setup(format!("invalid_persona: {e}"))
}

fn teams_of(name: &str) -> Vec<Kind> {
    TEAMS.iter().filter(|t| t.research == name || t.plan == name).map(|t| t.kind).collect()
}

fn view(name: &str, toml: &str, source: Origin) -> PersonaView {
    let parsed = Persona::parse(&format!("{name}.toml"), toml).ok();
    PersonaView {
        name: name.to_string(),
        display_name: parsed.as_ref().map_or_else(|| name.to_string(), |p| p.display_name.clone()),
        role: parsed.map(|p| p.role).unwrap_or_default(),
        teams: teams_of(name),
        source,
        builtin: shipped(name).is_some(),
        toml: toml.to_string(),
    }
}

/// Checks persona `text` as the library copy called `name`: it parses, its
/// name is `name`, its tools and evals exist on the server, and against the
/// built-in it keeps the same output and every safety guardrail.
pub fn check(name: &str, text: &str) -> Result<Persona, ServerError> {
    let file = format!("{name}.toml");
    let p = Persona::parse(&file, text).map_err(invalid)?;
    if p.name != name {
        return Err(invalid(format!("{file}: name: `{}` must equal `{name}`", p.name)));
    }
    if let Some((i, tool)) = p.tools.iter().enumerate().find(|(_, t)| !PLUGIN_TOOLS.contains(&t.as_str())) {
        return Err(invalid(format!("{file}: tools[{i}]: unknown tool `{tool}`")));
    }
    let evals = crate::evals::registry();
    if let Some((i, e)) = p.evals.iter().enumerate().find(|(_, e)| e.name != "judge" && evals.get(&e.name).is_none()) {
        return Err(invalid(format!("{file}: evals[{i}]: unknown eval `{}`", e.name)));
    }
    if let Some(base) = shipped(name) {
        let base = Persona::parse(&file, base).expect("shipped personas parse");
        if p.output != base.output {
            return Err(invalid(format!("{file}: output: must stay the built-in's, the workflow reads it")));
        }
        if let Some(g) = base.guardrails.iter().find(|g| g.starts_with(SAFETY) && !p.guardrails.contains(g)) {
            return Err(invalid(format!("{file}: guardrails: keep `{g}`")));
        }
    }
    Ok(p)
}

impl Writer<'_> {
    async fn library(&self) -> Result<Vec<Row>, ServerError> {
        Ok(list_rows::<Row>(self.pool, self.user, &[], false).await?.into_iter().filter(|p| p.goal_id.is_none()).collect())
    }

    /// Lists every shipped persona, replaced by the owner's library copy
    /// when there is one, then library personas without a built-in.
    /// Returns them in shipped order.
    pub async fn list_personas(&self) -> Result<Vec<PersonaView>, ServerError> {
        let library = self.library().await?;
        let mut out: Vec<PersonaView> = PERSONAS
            .iter()
            .map(|(name, text)| match library.iter().find(|r| r.name == *name) {
                Some(row) => view(name, &row.toml, Origin::Library),
                None => view(name, text, Origin::Builtin),
            })
            .collect();
        out.extend(library.iter().filter(|r| shipped(&r.name).is_none()).map(|r| view(&r.name, &r.toml, Origin::Library)));
        Ok(out)
    }

    /// Copies built-in persona `name` into the owner's library as an
    /// editable row. Returns it, `NotFound` for an unknown built-in, or
    /// `Conflict` when the library already holds that name.
    pub async fn copy_persona(&self, name: &str) -> Result<PersonaView, ServerError> {
        let text = shipped(name).ok_or_else(|| ServerError::NotFound(format!("built-in persona {name}")))?;
        if self.library().await?.iter().any(|r| r.name == name) {
            return Err(ServerError::Conflict(format!("persona {name} is already in the library")));
        }
        let mut row = Row {
            id: aroh_domain::derived_id(&format!("persona-library:{}", self.user), name, 0),
            user_id: self.user.to_string(),
            goal_id: None,
            name: name.to_string(),
            toml: text.to_string(),
            updated_at: String::new(),
            deleted: false,
        };
        put_row(self.pool, self.hlc, self.user, &mut row).await?;
        Ok(view(name, text, Origin::Library))
    }

    /// Replaces the owner's library copy of `name` with `text` after
    /// `check`. Returns the stored copy, `NotFound` when the library has
    /// no such persona, or `Setup` naming the file and field.
    pub async fn edit_persona(&self, name: &str, text: &str) -> Result<PersonaView, ServerError> {
        let mut row = self.library().await?.into_iter().find(|r| r.name == name).ok_or_else(|| ServerError::NotFound(format!("library persona {name}")))?;
        check(name, text)?;
        row.toml = text.to_string();
        put_row(self.pool, self.hlc, self.user, &mut row).await?;
        Ok(view(name, text, Origin::Library))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_keeps_runs_working_and_safety_rules_in_place() {
        for (name, text) in PERSONAS {
            assert!(check(name, text).is_ok(), "{name}");
        }
        let coach = shipped("practice-coach").unwrap();
        let err = |name: &str, text: &str| check(name, text).unwrap_err().to_string();
        assert!(err("coach", coach).contains("must equal `coach`"));
        assert!(err("practice-coach", &coach.replace("summary = \"string\"", "summary = \"int\"")).contains("output"));
        assert!(err("practice-coach", &coach.replace("evals = [", "evals = [\"vibes(items)\", ")).contains("unknown eval `vibes`"));
        let budget = shipped("budget-coach").unwrap();
        let without: String = budget.lines().filter(|l| !l.contains(SAFETY)).collect::<Vec<_>>().join("\n");
        assert!(err("budget-coach", &without).contains("keep `Safety:"));
        assert_eq!(teams_of("practice-coach"), [Kind::Music]);
        assert_eq!(teams_of("goal-researcher"), [Kind::General, Kind::Fitness, Kind::Habits]);
        assert!(teams_of("plugin-builder").is_empty());
    }
}
