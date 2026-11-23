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

/// Body of `PUT /v1/personas/{name}`: either the full persona TOML that
/// replaces the owner's library copy, or one or more fields (display name,
/// role, instructions, guardrails) set on the stored copy. Never both.
/// The result is validated by `check` before it is stored.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EditPersona {
    pub toml: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub instructions: Option<String>,
    pub guardrails: Option<Vec<String>>,
}

impl EditPersona {
    fn has_fields(&self) -> bool {
        self.display_name.is_some() || self.role.is_some() || self.instructions.is_some() || self.guardrails.is_some()
    }

    /// The persona text this edit stores over `current`, the copy `name`:
    /// `toml` verbatim, or `current` with the set fields replaced.
    /// `BadRequest` when both or neither are set, `Setup` when `current`
    /// is not TOML.
    pub fn text(&self, name: &str, current: &str) -> Result<String, ServerError> {
        match (&self.toml, self.has_fields()) {
            (Some(_), true) => Err(ServerError::BadRequest("set toml or the fields, not both".into())),
            (None, false) => Err(ServerError::BadRequest("set toml or at least one field".into())),
            (Some(text), false) => Ok(text.clone()),
            (None, true) => apply(name, current, self),
        }
    }
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

/// One persona as the library lists it: name, display name, role,
/// instructions and guardrails from its text, the built-in's safety
/// guardrails an edit must keep (`locked`), the team kinds that use it,
/// where the text comes from, whether a built-in exists, and the TOML.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PersonaView {
    pub name: String,
    pub display_name: String,
    pub role: String,
    pub instructions: String,
    pub guardrails: Vec<String>,
    pub locked: Vec<String>,
    pub teams: Vec<Kind>,
    pub source: Origin,
    pub builtin: bool,
    pub toml: String,
}

fn invalid(e: impl std::fmt::Display) -> ServerError {
    ServerError::Setup(format!("invalid_persona: {e}"))
}

/// Sets the fields of `edit` on persona text `current` of `name` through
/// a TOML table, so every other key is kept. Instructions replace an
/// `instructions_file`. Returns the new text, or `Setup` when `current`
/// is not TOML.
fn apply(name: &str, current: &str, edit: &EditPersona) -> Result<String, ServerError> {
    let mut table: toml::Table = current.parse().map_err(|e: toml::de::Error| invalid(format!("{name}.toml: {}", e.to_string().trim())))?;
    let text = |v: &String| toml::Value::String(v.clone());
    if let Some(v) = &edit.display_name {
        table.insert("display_name".into(), text(v));
    }
    if let Some(v) = &edit.role {
        table.insert("role".into(), text(v));
    }
    if let Some(v) = &edit.instructions {
        table.remove("instructions_file");
        table.insert("instructions".into(), text(v));
    }
    if let Some(rules) = &edit.guardrails {
        table.insert("guardrails".into(), toml::Value::Array(rules.iter().map(text).collect()));
    }
    let out = toml::to_string(&table).expect("a parsed table serializes");
    assert_eq!(out.parse::<toml::Table>().ok().as_ref(), Some(&table), "the written text reads back as the table");
    Ok(out)
}

fn safety_rules(name: &str) -> Vec<String> {
    shipped(name)
        .and_then(|text| Persona::parse(&format!("{name}.toml"), text).ok())
        .map(|p| p.guardrails.into_iter().filter(|g| g.starts_with(SAFETY)).collect())
        .unwrap_or_default()
}

fn teams_of(name: &str) -> Vec<Kind> {
    TEAMS.iter().filter(|t| t.research == name || t.plan == name).map(|t| t.kind).collect()
}

fn view(name: &str, toml: &str, source: Origin) -> PersonaView {
    let parsed = Persona::parse(&format!("{name}.toml"), toml).ok();
    PersonaView {
        name: name.to_string(),
        display_name: parsed.as_ref().map_or_else(|| name.to_string(), |p| p.display_name.clone()),
        role: parsed.as_ref().map(|p| p.role.clone()).unwrap_or_default(),
        instructions: parsed.as_ref().map(|p| p.instructions.clone()).unwrap_or_default(),
        guardrails: parsed.map(|p| p.guardrails).unwrap_or_default(),
        locked: safety_rules(name),
        teams: teams_of(name),
        source,
        builtin: shipped(name).is_some(),
        toml: toml.to_string(),
    }
}

/// Gives an owner's copy of a shipped persona the shipped `[output]`, which every copy must keep,
/// so a copy saved before the output grew a field still matches the workflow that reads it.
/// Returns `text` unchanged when its output already matches, for a persona that is not shipped,
/// or for a copy that does not parse.
pub fn with_shipped_output(name: &str, text: String) -> String {
    let Some(base) = shipped(name).and_then(|b| b.parse::<toml::Table>().ok()) else { return text };
    let (Ok(mut copy), Some(output)) = (text.parse::<toml::Table>(), base.get("output")) else { return text };
    if copy.get("output") == Some(output) {
        return text;
    }
    copy.insert("output".into(), output.clone());
    toml::to_string(&copy).expect("a parsed table serializes")
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

    /// Replaces the owner's library copy of `name` with the text of `edit`
    /// (see `EditPersona::text`) after `check`. Returns the stored copy,
    /// `NotFound` when the library has no such persona, `BadRequest` for
    /// a malformed edit, or `Setup` naming the file and field.
    pub async fn edit_persona(&self, name: &str, edit: &EditPersona) -> Result<PersonaView, ServerError> {
        let mut row = self.library().await?.into_iter().find(|r| r.name == name).ok_or_else(|| ServerError::NotFound(format!("library persona {name}")))?;
        let text = edit.text(name, &row.toml)?;
        check(name, &text)?;
        row.toml = text;
        put_row(self.pool, self.hlc, self.user, &mut row).await?;
        Ok(view(name, &row.toml, Origin::Library))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_copy_takes_the_shipped_output() {
        let base = shipped("intake-interviewer").unwrap();
        let old = base.replace(", lasting: {topic: string, label: string}?, known: {fact_id: string, value: string, source: string}?", "");
        assert_ne!(old, base, "the copy predates lasting and known");
        let fixed = with_shipped_output("intake-interviewer", old.clone());
        assert!(check("intake-interviewer", &fixed).is_ok(), "the copy matches the shipped output");
        assert!(check("intake-interviewer", &old).is_err());
        assert_eq!(with_shipped_output("no-such-persona", "x".into()), "x");
        let edited = base.replace("display_name = \"", "display_name = \"My ");
        assert_eq!(with_shipped_output("intake-interviewer", edited.clone()), edited, "a current copy keeps its text");
    }

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

    fn parse(name: &str, text: &str) -> Persona {
        Persona::parse(&format!("{name}.toml"), text).unwrap()
    }

    #[test]
    fn apply_rewrites_only_the_set_fields() {
        for (name, text) in PERSONAS {
            let same = apply(name, text, &EditPersona::default()).unwrap();
            assert_eq!(parse(name, &same), parse(name, text), "{name}");
            assert!(check(name, &same).is_ok(), "{name}");
        }
        let budget = shipped("budget-coach").unwrap();
        let base = parse("budget-coach", budget);
        let tricky = "Say \"hi\" with a \\ backslash\nthen a second line.\n";
        let rules = vec![base.guardrails[2].clone(), "Keep it short.".to_string()];
        let cases: [(EditPersona, Box<dyn Fn(&mut Persona)>); 4] = [
            (EditPersona { display_name: Some("Money mentor".into()), ..Default::default() }, Box::new(|p| p.display_name = "Money mentor".into())),
            (EditPersona { role: Some("A warm, plain-spoken coach.".into()), ..Default::default() }, Box::new(|p| p.role = "A warm, plain-spoken coach.".into())),
            (EditPersona { instructions: Some(tricky.into()), ..Default::default() }, Box::new(|p| p.instructions = tricky.into())),
            (EditPersona { guardrails: Some(rules.clone()), ..Default::default() }, Box::new(move |p| p.guardrails = rules.clone())),
        ];
        for (edit, change) in cases {
            let mut want = base.clone();
            change(&mut want);
            let out = edit.text("budget-coach", budget).unwrap();
            assert_eq!(parse("budget-coach", &out), want, "{edit:?}");
            assert!(check("budget-coach", &out).is_ok(), "{edit:?}");
        }
        let both = EditPersona { toml: Some(budget.into()), role: Some("x".into()), ..Default::default() };
        assert!(matches!(both.text("budget-coach", budget), Err(ServerError::BadRequest(_))));
        assert!(matches!(EditPersona::default().text("budget-coach", budget), Err(ServerError::BadRequest(_))));
        let whole = EditPersona { toml: Some("name = 3".into()), ..Default::default() };
        assert_eq!(whole.text("budget-coach", budget).unwrap(), "name = 3");
        let broken = EditPersona { role: Some("x".into()), ..Default::default() };
        assert!(matches!(broken.text("budget-coach", "name = "), Err(ServerError::Setup(_))));
    }

    #[test]
    fn view_lists_fields_and_locked_safety_rules() {
        let budget = view("budget-coach", shipped("budget-coach").unwrap(), Origin::Builtin);
        assert_eq!(budget.locked, ["Safety: no individual investment, tax or legal advice; refer to a licensed financial professional for those."]);
        assert_eq!(budget.guardrails.len(), 4);
        assert!(budget.instructions.starts_with("1. Search the web"));
        let tutor = view("tutor", shipped("tutor").unwrap(), Origin::Builtin);
        assert!(tutor.locked.is_empty());
        assert!(!tutor.guardrails.is_empty());
    }
}
