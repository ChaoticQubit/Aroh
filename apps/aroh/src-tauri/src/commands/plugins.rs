//! Plugin commands: a goal's plugin versions with their validated trees, the version switch,
//! owner-edited bundles and the built-in templates. Every synced bundle is validated again here.

use super::app::after_write;
use super::with_core;
use crate::{AppState, Core};
use aroh_domain::rows::{Goal, Plugin, Tracker};
use aroh_plugins::versions::{active, enable, history, next_version};
use aroh_store::repo;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

const FILE: &str = "plugin.toml";

/// One plugin version of a goal as the goal screen reads it: the row, whether it is the
/// version the dashboard renders, and its validated tree, or the validator's error.
#[derive(Debug, Clone, Serialize)]
pub struct PluginVersion {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub enabled: bool,
    pub active: bool,
    pub tree: Option<Value>,
    pub error: Option<String>,
}

fn invalid(reason: impl ToString) -> aroh_app::Error {
    aroh_app::Error::Invalid(reason.to_string())
}

fn rows(c: &Core, goal_id: &str) -> aroh_app::Result<Vec<Plugin>> {
    Ok(repo::list::<Plugin>(&c.conn, &[("user_id", &c.user_id), ("goal_id", goal_id)])?)
}

fn versions(c: &Core, goal_id: &str) -> aroh_app::Result<Vec<PluginVersion>> {
    let rows = rows(c, goal_id)?;
    let on = active(&rows, goal_id).map(|p| p.id.clone());
    Ok(history(&rows, goal_id)
        .into_iter()
        .map(|p| {
            let parsed = aroh_plugins::parse(FILE, &p.bundle_toml);
            PluginVersion {
                id: p.id.clone(),
                name: p.name.clone(),
                version: p.version,
                enabled: p.enabled,
                active: on.as_deref() == Some(p.id.as_str()),
                error: parsed.as_ref().err().map(ToString::to_string),
                tree: parsed.ok().map(|b| b.tree),
            }
        })
        .collect())
}

fn switch(c: &mut Core, goal_id: &str, plugin_id: Option<&str>) -> aroh_app::Result<()> {
    let writes = enable(&rows(c, goal_id)?, goal_id, plugin_id).ok_or_else(|| invalid("plugin is not a version of this goal"))?;
    for mut row in writes {
        repo::put(&c.conn, &mut c.clock, &mut row)?;
    }
    Ok(())
}

/// Validates `text`, stores it as the goal's next version, enabled, and creates the trackers
/// it declares that the goal lacks. Personas stay in the bundle: only the server, which can
/// check persona files, turns them into persona rows.
fn save(c: &mut Core, goal_id: &str, text: &str) -> aroh_app::Result<()> {
    let bundle = aroh_plugins::parse(FILE, text).map_err(invalid)?;
    let goal: Goal = repo::get(&c.conn, goal_id)?;
    let existing = repo::list::<Tracker>(&c.conn, &[("user_id", &c.user_id), ("goal_id", &goal.id)])?;
    for mut t in aroh_plugins::missing_trackers(&bundle, &existing, &c.user_id, &goal.id) {
        repo::put(&c.conn, &mut c.clock, &mut t)?;
    }
    let mut row = Plugin {
        id: aroh_domain::new_id(),
        user_id: c.user_id.clone(),
        goal_id: Some(goal.id.clone()),
        name: bundle.name.clone(),
        version: next_version(&rows(c, &goal.id)?, Some(&goal.id), &bundle.name),
        bundle_toml: text.to_string(),
        enabled: true,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(&c.conn, &mut c.clock, &mut row)?;
    switch(c, &goal.id, Some(&row.id))
}

/// Lists the plugin versions of goal `goal_id`, newest first, each with its validated tree
/// or its validation error. The one marked `active` with a tree is the dashboard.
#[tauri::command]
pub fn plugins_list(state: State<'_, AppState>, goal_id: String) -> Result<Vec<PluginVersion>, String> {
    with_core(&state, |c| versions(c, &goal_id))
}

/// Makes `plugin_id` the only enabled version of goal `goal_id`, or with no id turns the
/// goal's plugins off so the derived dashboard shows. Returns the versions after the switch.
#[tauri::command]
pub fn plugins_enable(app: AppHandle, state: State<'_, AppState>, goal_id: String, plugin_id: Option<String>) -> Result<Vec<PluginVersion>, String> {
    let out = with_core(&state, |c| {
        switch(c, &goal_id, plugin_id.as_deref())?;
        versions(c, &goal_id)
    })?;
    after_write(&app);
    Ok(out)
}

/// Stores an owner-edited bundle as goal `goal_id`'s next enabled version after validating
/// it. Errors name the file and field. Returns the versions after the save.
#[tauri::command]
pub fn plugins_save(app: AppHandle, state: State<'_, AppState>, goal_id: String, bundle_toml: String) -> Result<Vec<PluginVersion>, String> {
    let out = with_core(&state, |c| {
        save(c, &goal_id, &bundle_toml)?;
        versions(c, &goal_id)
    })?;
    after_write(&app);
    Ok(out)
}

/// Stores the built-in template `name` (for example `fitness`) as goal `goal_id`'s next
/// enabled version. Returns the versions after the save.
#[tauri::command]
pub fn plugins_use_template(app: AppHandle, state: State<'_, AppState>, goal_id: String, name: String) -> Result<Vec<PluginVersion>, String> {
    let text = aroh_plugins::template(&name).ok_or_else(|| format!("no template named {name}"))?;
    let out = with_core(&state, |c| {
        save(c, &goal_id, text)?;
        versions(c, &goal_id)
    })?;
    after_write(&app);
    Ok(out)
}
