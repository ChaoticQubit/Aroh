use crate::error::ServerError;
use crate::rows::{get_row, list_rows, put_row, ServerClock};
use crate::db::PgPool;
use aroh_domain::rows::{Goal, Persona, Plugin, Tracker};
use aroh_plugins::versions::{enable, history, next_version};
use aroh_plugins::Bundle;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// The file name every validation error of a stored or generated bundle
/// carries, as in `plugin.toml: sections[0].title: missing`.
/// Bundles arrive as text, not files.
pub const FILE: &str = "plugin.toml";

/// Tools a plugin may name for its personas: the ones every run registers.
/// Any other tool name fails validation.
/// Kept in step with `RunIo::build`.
pub const PLUGIN_TOOLS: &[&str] = &["web_search", "web_fetch"];

/// Body of `PUT /v1/goals/{goal_id}/plugins`: a bundle to store as the
/// goal's next version, and whether to enable it (default true).
/// The bundle is validated first.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SavePlugin {
    pub bundle_toml: String,
    pub enable: Option<bool>,
}

/// One plugin version as the API returns it: the row's fields.
/// `enabled` marks the version the goal dashboard renders.
/// Newest first in lists.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PluginView {
    pub id: String,
    pub goal_id: Option<String>,
    pub name: String,
    pub version: i64,
    pub enabled: bool,
    pub bundle_toml: String,
}

impl From<&Plugin> for PluginView {
    fn from(p: &Plugin) -> Self {
        PluginView { id: p.id.clone(), goal_id: p.goal_id.clone(), name: p.name.clone(), version: p.version, enabled: p.enabled, bundle_toml: p.bundle_toml.clone() }
    }
}

fn invalid(e: impl std::fmt::Display) -> ServerError {
    ServerError::Setup(format!("invalid_plugin: {e}"))
}

/// Validates bundle `text` as the server stores it: the bundle rules, tools
/// from `PLUGIN_TOOLS` only, and every persona by the persona parser.
/// Returns the bundle, or `Setup` naming the file and field.
pub fn check(text: &str) -> Result<Bundle, ServerError> {
    let bundle = aroh_plugins::parse(FILE, text).map_err(invalid)?;
    bundle.check_tools(FILE, PLUGIN_TOOLS).map_err(invalid)?;
    for (i, p) in bundle.personas.iter().enumerate() {
        troupekit_agent::Persona::parse(&format!("{}.toml", p.name), &p.toml)
            .map_err(|e| invalid(format!("{FILE}: personas[{i}].toml: {e}")))?;
    }
    Ok(bundle)
}

/// Who writes and where: the pool, the server clock and the user.
/// Every function here acts only on that user's rows.
/// Built by the tool and the API alike.
pub struct Writer<'a> {
    pub pool: &'a PgPool,
    pub hlc: &'a ServerClock,
    pub user: Uuid,
}

impl Writer<'_> {
    async fn goal(&self, goal: Uuid) -> Result<Goal, ServerError> {
        get_row::<Goal>(self.pool, self.user, &goal.to_string()).await?.ok_or_else(|| ServerError::NotFound(format!("goal {goal}")))
    }

    async fn rows(&self, goal: Uuid) -> Result<Vec<Plugin>, ServerError> {
        list_rows::<Plugin>(self.pool, self.user, &[("goal_id", goal.to_string())], true).await
    }

    async fn write_all(&self, rows: Vec<Plugin>) -> Result<(), ServerError> {
        for mut row in rows {
            put_row(self.pool, self.hlc, self.user, &mut row).await?;
        }
        Ok(())
    }

    /// Stores bundle `text` as the next version of `goal`'s plugin, under
    /// row `id` (a replayed call finds its row and keeps its version), adds
    /// the bundle's missing trackers and its personas, and enables it when asked.
    pub async fn store(&self, goal: Uuid, text: &str, id: String, enabled: bool) -> Result<Plugin, ServerError> {
        let bundle = check(text)?;
        let goal_row = self.goal(goal).await?;
        let goal_text = goal_row.id.clone();
        let rows = self.rows(goal).await?;
        let row = match rows.iter().find(|p| p.id == id) {
            Some(existing) => existing.clone(),
            None => {
                let mut row = Plugin {
                    id: id.clone(),
                    user_id: self.user.to_string(),
                    goal_id: Some(goal_text.clone()),
                    name: bundle.name.clone(),
                    version: next_version(&rows, Some(&goal_text), &bundle.name),
                    bundle_toml: text.to_string(),
                    enabled: false,
                    updated_at: String::new(),
                    deleted: false,
                };
                put_row(self.pool, self.hlc, self.user, &mut row).await?;
                row
            }
        };
        self.trackers(&goal_text, &bundle).await?;
        self.personas(&goal_text, &bundle).await?;
        if enabled {
            return self.enable(&row.id).await;
        }
        Ok(row)
    }

    async fn trackers(&self, goal: &str, bundle: &Bundle) -> Result<(), ServerError> {
        let existing = list_rows::<Tracker>(self.pool, self.user, &[("goal_id", goal.to_string())], false).await?;
        for mut row in aroh_plugins::missing_trackers(bundle, &existing, &self.user.to_string(), goal) {
            put_row(self.pool, self.hlc, self.user, &mut row).await?;
        }
        Ok(())
    }

    async fn personas(&self, goal: &str, bundle: &Bundle) -> Result<(), ServerError> {
        for p in &bundle.personas {
            let id = aroh_domain::derived_id(&format!("plugin-persona:{goal}"), &p.name, 0);
            let previous = get_row::<Persona>(self.pool, self.user, &id).await?.map(|r| r.updated_at).unwrap_or_default();
            let mut row = Persona {
                id,
                user_id: self.user.to_string(),
                goal_id: Some(goal.to_string()),
                name: p.name.clone(),
                toml: p.toml.clone(),
                updated_at: previous,
                deleted: false,
            };
            put_row(self.pool, self.hlc, self.user, &mut row).await?;
        }
        Ok(())
    }

    /// Makes plugin row `id` the only enabled version of its goal: the
    /// target is written first, then the others are turned off.
    /// Returns the enabled row, or `NotFound`.
    pub async fn enable(&self, id: &str) -> Result<Plugin, ServerError> {
        let target = self.get(id).await?;
        let goal = target.goal_id.as_deref().and_then(|g| Uuid::parse_str(g).ok()).ok_or_else(|| ServerError::NotFound(format!("plugin {id} has no goal")))?;
        let rows = self.rows(goal).await?;
        let goal_text = goal.to_string();
        let writes = enable(&rows, &goal_text, Some(id)).ok_or_else(|| ServerError::NotFound(format!("plugin {id}")))?;
        self.write_all(writes).await?;
        let enabled = self.get(id).await?;
        assert!(enabled.enabled, "the target was written first");
        Ok(enabled)
    }

    /// Returns the user's live plugin row `id`.
    /// `NotFound` when missing, deleted or another user's.
    /// Reads one row.
    pub async fn get(&self, id: &str) -> Result<Plugin, ServerError> {
        get_row::<Plugin>(self.pool, self.user, id).await?.ok_or_else(|| ServerError::NotFound(format!("plugin {id}")))
    }

    /// Lists the live plugin versions of `goal`, newest first.
    /// `NotFound` when the user has no such goal.
    /// Deleted versions are left out.
    pub async fn list(&self, goal: Uuid) -> Result<Vec<Plugin>, ServerError> {
        self.goal(goal).await?;
        let rows = self.rows(goal).await?;
        Ok(history(&rows, &goal.to_string()).into_iter().cloned().collect())
    }
}
