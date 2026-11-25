//! Plugin commands: a goal's plugin versions with their validated trees, the version switch,
//! owner-edited bundles, the built-in templates, every live plugin and one plugin's read-only
//! page. Every synced bundle is validated again here.

use super::app::after_write;
use super::with_core;
use crate::{AppState, Core};
use aroh_domain::rows::{Cadence, Goal, Plugin, Tracker, TrackerField};
use aroh_plugins::versions::{active, enable, history, next_version};
use aroh_store::repo;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

const FILE: &str = "plugin.toml";

/// One plugin version of a goal as the goal screen reads it: the row, whether it is the
/// version the dashboard renders, its validated tree, or the validator's error, and the
/// agent run that created it (none when the owner or an older app made it).
#[derive(Debug, Clone, Serialize)]
pub struct PluginVersion {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub enabled: bool,
    pub active: bool,
    pub tree: Option<Value>,
    pub error: Option<String>,
    pub created_run_id: Option<String>,
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
                created_run_id: p.created_run_id.clone(),
            }
        })
        .collect())
}

/// One live plugin version as the coach library and the plugin page name it: the row, the
/// bundle title (the name when the bundle does not validate), whether it is its goal's active
/// version, and its goal. A library plugin has no goal and is never active.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PluginSummary {
    pub id: String,
    pub name: String,
    pub title: String,
    pub version: i64,
    pub active: bool,
    pub goal_id: Option<String>,
    pub goal_title: Option<String>,
}

/// One tracker a bundle declares as the plugin page shows it: name, fields with units and the
/// check-in cadence, which the validated tree leaves out.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PluginTracker {
    pub name: String,
    pub fields: Vec<TrackerField>,
    pub cadence: Option<Cadence>,
}

/// One plugin version for its read-only page: the summary, the raw bundle, its validated tree
/// with its trackers, or the validator's error with no tree and no trackers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PluginDetail {
    #[serde(flatten)]
    pub summary: PluginSummary,
    pub bundle_toml: String,
    pub tree: Option<Value>,
    pub error: Option<String>,
    pub trackers: Vec<PluginTracker>,
}

/// The summary of plugin `p` among the user's live `rows` and live `goals`, or none when its
/// goal is gone, so a plugin of a deleted goal is never listed or opened.
fn summary(p: &Plugin, rows: &[Plugin], goals: &[Goal]) -> Option<PluginSummary> {
    let goal = match p.goal_id.as_deref() {
        Some(id) => Some(goals.iter().find(|g| g.id == id && !g.deleted)?),
        None => None,
    };
    let on = goal.and_then(|g| active(rows, &g.id)).is_some_and(|a| a.id == p.id);
    let title = aroh_plugins::parse(FILE, &p.bundle_toml).map(|b| b.title).unwrap_or_else(|_| p.name.clone());
    Some(PluginSummary {
        id: p.id.clone(),
        name: p.name.clone(),
        title,
        version: p.version,
        active: on,
        goal_id: goal.map(|g| g.id.clone()),
        goal_title: goal.map(|g| g.title.clone()),
    })
}

/// Every live plugin of the user, one per goal and name: the goal's active version of that
/// name, else its newest. Sorted by goal title (library plugins last), then title.
/// `rows` and `goals` are the user's live rows.
fn summaries(rows: &[Plugin], goals: &[Goal]) -> Vec<PluginSummary> {
    let live: Vec<&Plugin> = rows.iter().filter(|p| !p.deleted).collect();
    let mut out: Vec<PluginSummary> = Vec::new();
    for p in &live {
        let same: Vec<&&Plugin> = live.iter().filter(|q| q.goal_id == p.goal_id && q.name == p.name).collect();
        let on = p.goal_id.as_deref().and_then(|g| active(rows, g)).filter(|a| a.name == p.name).map(|a| a.id.clone());
        let newest = same.iter().max_by(|a, b| a.version.cmp(&b.version).then_with(|| a.id.cmp(&b.id))).map(|q| q.id.clone());
        if on.or(newest).as_deref() != Some(p.id.as_str()) {
            continue;
        }
        out.extend(summary(p, rows, goals));
    }
    out.sort_by(|a, b| (a.goal_title.is_none(), &a.goal_title, &a.title, &a.id).cmp(&(b.goal_title.is_none(), &b.goal_title, &b.title, &b.id)));
    assert!(out.len() <= live.len(), "one summary per goal and name at most");
    out
}

/// The read-only page of live plugin `id` among the user's `rows` and `goals`, or none when it
/// is deleted, unknown or its goal is gone.
fn detail(rows: &[Plugin], goals: &[Goal], id: &str) -> Option<PluginDetail> {
    let p = rows.iter().find(|p| p.id == id && !p.deleted)?;
    let summary = summary(p, rows, goals)?;
    let parsed = aroh_plugins::parse(FILE, &p.bundle_toml);
    let trackers = parsed.as_ref().map(|b| b.trackers.iter().map(|t| PluginTracker { name: t.name.clone(), fields: t.fields.clone(), cadence: t.cadence.clone() }).collect()).unwrap_or_default();
    Some(PluginDetail {
        summary,
        bundle_toml: p.bundle_toml.clone(),
        error: parsed.as_ref().err().map(ToString::to_string),
        tree: parsed.ok().map(|b| b.tree),
        trackers,
    })
}

fn everything(c: &Core) -> aroh_app::Result<(Vec<Plugin>, Vec<Goal>)> {
    let rows = repo::list::<Plugin>(&c.conn, &[("user_id", &c.user_id)])?;
    let goals = repo::list::<Goal>(&c.conn, &[("user_id", &c.user_id)])?;
    Ok((rows, goals))
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
        created_run_id: None,
        updated_at: String::new(),
        deleted: false,
    };
    repo::put(&c.conn, &mut c.clock, &mut row)?;
    switch(c, &goal.id, Some(&row.id))
}

/// Lists the plugin versions of goal `goal_id`, newest first, each with its validated tree
/// or its validation error. The one marked `active` with a tree is the dashboard.
#[tauri::command(async)]
pub fn plugins_list(state: State<'_, AppState>, goal_id: String) -> Result<Vec<PluginVersion>, String> {
    with_core(&state, |c| versions(c, &goal_id))
}

/// Makes `plugin_id` the only enabled version of goal `goal_id`, or with no id turns the
/// goal's plugins off so the derived dashboard shows. Returns the versions after the switch.
#[tauri::command(async)]
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
#[tauri::command(async)]
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
#[tauri::command(async)]
pub fn plugins_use_template(app: AppHandle, state: State<'_, AppState>, goal_id: String, name: String) -> Result<Vec<PluginVersion>, String> {
    let text = aroh_plugins::template(&name).ok_or_else(|| format!("no template named {name}"))?;
    let out = with_core(&state, |c| {
        save(c, &goal_id, text)?;
        versions(c, &goal_id)
    })?;
    after_write(&app);
    Ok(out)
}

/// Lists every live plugin of the user across goals, one per goal and name (the active
/// version, else the newest), for the Plugins group of the coach library.
#[tauri::command]
pub fn plugins_all(state: State<'_, AppState>) -> Result<Vec<PluginSummary>, String> {
    with_core(&state, |c| {
        let (rows, goals) = everything(c)?;
        Ok(summaries(&rows, &goals))
    })
}

/// Reads live plugin `plugin_id` for its read-only page: summary, raw bundle, validated tree
/// and trackers, or the validation error. Errors when the plugin or its goal is gone.
#[tauri::command]
pub fn plugins_get(state: State<'_, AppState>, plugin_id: String) -> Result<PluginDetail, String> {
    with_core(&state, |c| {
        let (rows, goals) = everything(c)?;
        detail(&rows, &goals, &plugin_id).ok_or_else(|| invalid("plugin: it was deleted or its goal is gone"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aroh_domain::rows::{Autonomy, CadenceFreq, GoalStatus};

    fn goal(id: &str, title: &str) -> Goal {
        Goal {
            id: id.into(),
            user_id: "u".into(),
            category_id: None,
            title: title.into(),
            context: None,
            deadline: None,
            status: GoalStatus::Active,
            autonomy: Autonomy::Balanced,
            created_at: "2026-11-20T00:00:00Z".parse().unwrap(),
            reminders: true,
            use_calendars: true,
            ordered: false,
            run_limit_minutes: None,
            updated_at: String::new(),
            deleted: false,
        }
    }

    fn plugin(id: &str, goal: Option<&str>, name: &str, version: i64, enabled: bool, bundle: &str) -> Plugin {
        Plugin {
            id: id.into(),
            user_id: "u".into(),
            goal_id: goal.map(Into::into),
            name: name.into(),
            version,
            bundle_toml: bundle.into(),
            enabled,
            created_run_id: None,
            updated_at: String::new(),
            deleted: false,
        }
    }

    #[test]
    fn b81_lists_one_plugin_per_goal_and_name_active_else_newest() {
        let fitness = aroh_plugins::template("fitness").unwrap();
        let learning = aroh_plugins::template("learning").unwrap();
        let goals = vec![goal("g1", "Run a 5k"), goal("g2", "Machine learning")];
        let rows = vec![
            plugin("a1", Some("g1"), "fitness", 1, true, fitness),
            plugin("a2", Some("g1"), "fitness", 2, false, fitness),
            plugin("b1", Some("g2"), "learning", 1, false, learning),
            plugin("b2", Some("g2"), "learning", 2, false, "not toml ["),
            plugin("c1", Some("gone"), "fitness", 1, true, fitness),
            plugin("d1", None, "habits", 1, true, aroh_plugins::template("habits").unwrap()),
        ];
        let listed: Vec<(String, bool, Option<String>)> = summaries(&rows, &goals).into_iter().map(|s| (s.id, s.active, s.goal_title)).collect();
        assert_eq!(
            listed,
            vec![
                ("b2".into(), false, Some("Machine learning".into())),
                ("a1".into(), true, Some("Run a 5k".into())),
                ("d1".into(), false, None),
            ],
            "active v1 wins over a newer off v2; newest when none is on; a gone goal's plugin is left out; library last"
        );
        let mut deleted = rows.clone();
        deleted[0].deleted = true;
        assert_eq!(summaries(&deleted, &goals)[1].id, "a2", "a deleted version is never listed");
    }

    #[test]
    fn b81_detail_carries_bundle_trackers_with_cadence_or_the_error() {
        let finance = aroh_plugins::template("finance").unwrap();
        let goals = vec![goal("g1", "Save for a bike")];
        let rows = vec![plugin("p1", Some("g1"), "finance", 1, true, finance), plugin("p2", Some("g1"), "finance", 2, false, "not toml [")];
        let page = detail(&rows, &goals, "p1").unwrap();
        assert_eq!((page.summary.active, page.summary.goal_title.as_deref(), page.bundle_toml.as_str()), (true, Some("Save for a bike"), finance));
        assert!(page.tree.is_some() && page.error.is_none());
        let weekly = page.trackers.iter().find_map(|t| t.cadence.as_ref()).expect("the finance template has a cadence");
        assert_eq!(weekly.freq, CadenceFreq::Weekly);
        let broken = detail(&rows, &goals, "p2").unwrap();
        assert!(broken.tree.is_none() && broken.trackers.is_empty() && broken.error.is_some());
        assert_eq!(broken.summary.title, "finance", "an invalid bundle is titled by its name");
        assert!(detail(&rows, &goals, "nope").is_none());
        assert!(detail(&rows, &[], "p1").is_none(), "a plugin of a gone goal does not open");
        let json = serde_json::to_value(&page).unwrap();
        assert_eq!(json["goal_title"], "Save for a bike", "the summary is flattened into the page");
    }
}
