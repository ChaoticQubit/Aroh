//! Plugins pod commands: list, load, enable and disable YAML plugins, and compute the named queries they bind to.

use crate::AppState;
use aroh_plugins::{parse_plugin, resolve, Plugin, RenderModel};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::Path;
use tauri::{AppHandle, Manager, State};

const BUILT_IN: [(&str, &str); 2] = [
    ("water-tracker.yaml", include_str!("../../../plugins/water-tracker.yaml")),
    ("course-progress.yaml", include_str!("../../../plugins/course-progress.yaml")),
];
const MAX_FILE_BYTES: u64 = 64 * 1024;
const MAX_USER_FILES: usize = 64;

/// One plugin file as the settings screen shows it. `error` is set when the file failed to parse.
#[derive(Serialize)]
pub struct PluginListing {
    pub file: String,
    pub source: &'static str,
    pub name: Option<String>,
    pub version: Option<String>,
    pub title: Option<String>,
    pub enabled: bool,
    pub error: Option<String>,
}

struct Loaded {
    file: String,
    source: &'static str,
    parsed: Result<Plugin, String>,
}

fn read_user_files(dir: &Path) -> Vec<(String, Result<String, String>)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "yaml" || x == "yml"))
        .collect();
    files.sort();
    files.truncate(MAX_USER_FILES);
    files
        .into_iter()
        .map(|path| {
            let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let text = match std::fs::metadata(&path) {
                Ok(m) if !m.is_file() => Err("not a regular file".to_string()),
                Ok(m) if m.len() > MAX_FILE_BYTES => Err(format!("{file}: file is larger than 64 KiB")),
                Ok(_) => std::fs::read_to_string(&path).map_err(|e| format!("{file}: {e}")),
                Err(e) => Err(format!("{file}: {e}")),
            };
            (file, text)
        })
        .collect()
}

fn load_all(app: &AppHandle) -> Vec<Loaded> {
    let mut out: Vec<Loaded> = BUILT_IN
        .iter()
        .map(|(file, yaml)| Loaded {
            file: file.to_string(),
            source: "builtin",
            parsed: parse_plugin(file, yaml).map_err(|e| e.to_string()),
        })
        .collect();
    if let Ok(dir) = app.path().app_data_dir() {
        for (file, text) in read_user_files(&dir.join("plugins")) {
            let parsed = text.and_then(|yaml| parse_plugin(&file, &yaml).map_err(|e| e.to_string()));
            out.push(Loaded { file, source: "user", parsed });
        }
    }
    out
}

fn is_enabled(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    let row: Option<bool> = conn
        .query_row("SELECT enabled FROM plugins WHERE name = ?1 AND deleted = 0", params![name], |r| r.get(0))
        .optional()?;
    Ok(row.unwrap_or(true))
}

/// Lists every built-in and user plugin file with its enabled flag and any parse error.
#[tauri::command]
pub fn plugins_list(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<PluginListing>, String> {
    let loaded = load_all(&app);
    let core = state.core.lock().map_err(|_| "database lock poisoned".to_string())?;
    loaded
        .into_iter()
        .map(|l| {
            let (info, error) = match &l.parsed {
                Ok(p) => (Some(p.info().clone()), None),
                Err(e) => (None, Some(e.clone())),
            };
            let enabled = match &info {
                Some(i) => is_enabled(&core.conn, &i.name).map_err(|e| e.to_string())?,
                None => false,
            };
            Ok(PluginListing {
                file: l.file,
                source: l.source,
                name: info.as_ref().map(|i| i.name.clone()),
                version: info.as_ref().map(|i| i.version.clone()),
                title: info.map(|i| i.title),
                enabled,
                error,
            })
        })
        .collect()
}

/// Loads all enabled, valid plugins and returns the resolved render model. Files that fail to parse are skipped.
#[tauri::command]
pub fn plugins_model(app: AppHandle, state: State<'_, AppState>) -> Result<RenderModel, String> {
    let loaded = load_all(&app);
    let core = state.core.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut enabled = Vec::new();
    for plugin in loaded.into_iter().filter_map(|l| l.parsed.ok()) {
        if is_enabled(&core.conn, plugin.name()).map_err(|e| e.to_string())? {
            enabled.push(plugin);
        }
    }
    resolve(&enabled).map_err(|e| e.to_string())
}

/// Enables or disables the plugin called `name`, storing the flag in the synced `plugins` table.
#[tauri::command]
pub fn plugins_set_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    enabled: bool,
) -> Result<(), String> {
    let yaml = load_all(&app)
        .into_iter()
        .find_map(|l| l.parsed.ok().filter(|p| p.name() == name).map(|p| p.yaml().to_string()))
        .ok_or_else(|| format!("no valid plugin named `{name}`"))?;
    let mut core = state.core.lock().map_err(|_| "database lock poisoned".to_string())?;
    let stamp = core.clock.tick();
    let changed = core
        .conn
        .execute(
            "UPDATE plugins SET enabled = ?1, updated_at = ?2 WHERE name = ?3 AND deleted = 0",
            params![enabled, stamp, name],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        core.conn
            .execute(
                "INSERT INTO plugins (id, name, yaml, enabled, updated_at, deleted) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
                params![aroh_core::new_id(), name, yaml, enabled, stamp],
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn number(conn: &Connection, sql: &str, args: impl rusqlite::Params) -> rusqlite::Result<f64> {
    conn.query_row(sql, args, |r| r.get::<_, Option<f64>>(0)).map(|v| v.unwrap_or(0.0))
}

fn actions(conn: &Connection, sql: &str, args: impl rusqlite::Params) -> rusqlite::Result<Value> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(args, |r| {
        Ok(json!({
            "id": r.get::<_, String>(0)?,
            "title": r.get::<_, String>(1)?,
            "start_at": r.get::<_, Option<String>>(2)?,
            "status": r.get::<_, String>(3)?,
        }))
    })?;
    Ok(Value::Array(rows.collect::<rusqlite::Result<_>>()?))
}

fn compute_data(conn: &Connection, goal_id: Option<&str>) -> rusqlite::Result<Map<String, Value>> {
    const TODAY: &str = "date(logged_at, 'localtime') = date('now', 'localtime') AND deleted = 0";
    let mut data = Map::new();
    data.insert("water.today_total".into(), json!(number(conn, &format!("SELECT SUM(amount) FROM tracker_entries WHERE kind = 'water' AND {TODAY}"), [])?));
    data.insert("food.today_total".into(), json!(number(conn, &format!("SELECT COUNT(*) FROM tracker_entries WHERE kind = 'food' AND {TODAY}"), [])?));
    data.insert("workout.today_total".into(), json!(number(conn, &format!("SELECT SUM(amount) FROM tracker_entries WHERE kind = 'workout' AND {TODAY}"), [])?));
    data.insert(
        "today.actions".into(),
        actions(
            conn,
            "SELECT id, title, start_at, status FROM actions WHERE deleted = 0 AND date(start_at, 'localtime') = date('now', 'localtime') ORDER BY start_at",
            [],
        )?,
    );
    if let Some(goal) = goal_id {
        let total = number(conn, "SELECT COUNT(*) FROM actions WHERE goal_id = ?1 AND deleted = 0", [goal])?;
        let done = number(conn, "SELECT COUNT(*) FROM actions WHERE goal_id = ?1 AND deleted = 0 AND status = 'done'", [goal])?;
        let left = number(conn, "SELECT COUNT(*) FROM actions WHERE goal_id = ?1 AND deleted = 0 AND status = 'planned'", [goal])?;
        data.insert("goal.progress".into(), json!(if total > 0.0 { done / total } else { 0.0 }));
        data.insert("goal.actions_left".into(), json!(left));
        data.insert(
            "goal.next_actions".into(),
            actions(
                conn,
                "SELECT id, title, start_at, status FROM actions WHERE goal_id = ?1 AND deleted = 0 AND status = 'planned' ORDER BY start_at, seq LIMIT 20",
                [goal],
            )?,
        );
    }
    Ok(data)
}

/// Computes the values of every named query. `goal.*` queries are present only when `goal_id` is given.
#[tauri::command]
pub fn plugins_data(state: State<'_, AppState>, goal_id: Option<String>) -> Result<Map<String, Value>, String> {
    let core = state.core.lock().map_err(|_| "database lock poisoned".to_string())?;
    compute_data(&core.conn, goal_id.as_deref()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_plugins_resolve_into_their_slots() {
        let plugins: Vec<Plugin> = BUILT_IN.iter().map(|(f, y)| parse_plugin(f, y).expect("built-in parses")).collect();
        let model = resolve(&plugins).expect("built-ins resolve");
        assert!(model.slots["today.sidebar"].iter().any(|i| i.plugin == "water-tracker"));
        assert!(model.slots["goal.detail"].iter().any(|i| i.plugin == "course-progress"));
        assert!(model.slots["log.trackers"].iter().any(|i| i.plugin == "water-tracker"));
    }

    #[test]
    fn goal_progress_counts_done_actions() {
        let conn = aroh_core::db::open_in_memory().expect("db");
        conn.execute_batch(
            "INSERT INTO goals (id, title, kind, target_date, status, created_at, updated_at) VALUES ('g', 'C', 'course', '2026-02-01', 'active', 'x', 'x');
             INSERT INTO actions (id, goal_id, title, start_at, end_at, est_minutes, status, seq, updated_at) VALUES
               ('a1', 'g', 'One', '2026-01-01T09:00:00Z', '2026-01-01T10:00:00Z', 60, 'done', 1, 'x'),
               ('a2', 'g', 'Two', '2026-01-02T09:00:00Z', '2026-01-02T10:00:00Z', 60, 'planned', 2, 'x'),
               ('a3', 'g', 'Three', '2026-01-03T09:00:00Z', '2026-01-03T10:00:00Z', 60, 'planned', 3, 'x'),
               ('a4', 'g', 'Four', '2026-01-04T09:00:00Z', '2026-01-04T10:00:00Z', 60, 'planned', 4, 'x');",
        )
        .expect("seed");
        let data = compute_data(&conn, Some("g")).expect("data");
        assert_eq!(data["goal.progress"], json!(0.25));
        assert_eq!(data["goal.actions_left"], json!(3.0));
        assert_eq!(data["goal.next_actions"][0]["id"], json!("a2"));
        assert!(compute_data(&conn, None).expect("data").get("goal.progress").is_none());
    }
}
