//! Version and enable rules over `plugins` rows: one row per version, and a goal's
//! active plugin is its enabled row with the highest version.

use aroh_domain::rows::Plugin;

fn belongs(p: &Plugin, goal: &str) -> bool {
    !p.deleted && p.goal_id.as_deref() == Some(goal)
}

/// The active plugin of `goal`: its enabled, live row with the highest
/// version, ties broken by the larger id. `None` means the derived view.
/// Deterministic, so a half-synced enable flip still renders one plugin.
pub fn active<'a>(rows: &'a [Plugin], goal: &str) -> Option<&'a Plugin> {
    rows.iter().filter(|p| belongs(p, goal)).filter(|p| p.enabled).max_by(|a, b| a.version.cmp(&b.version).then_with(|| a.id.cmp(&b.id)))
}

/// The versions of `goal`, newest first, deleted rows left out.
/// What the version picker lists.
/// Returns references into `rows`.
pub fn history<'a>(rows: &'a [Plugin], goal: &str) -> Vec<&'a Plugin> {
    let mut out: Vec<&Plugin> = rows.iter().filter(|p| belongs(p, goal)).collect();
    out.sort_by(|a, b| b.version.cmp(&a.version).then_with(|| b.id.cmp(&a.id)));
    out
}

/// The version a new row of plugin `name` for `goal` gets: one more than
/// the highest live or deleted version of that goal and name, else 1.
/// `goal` `None` is the user library.
pub fn next_version(rows: &[Plugin], goal: Option<&str>, name: &str) -> i64 {
    let top = rows.iter().filter(|p| p.goal_id.as_deref() == goal && p.name == name).map(|p| p.version).max().unwrap_or(0);
    assert!(top >= 0, "versions start at 1");
    top + 1
}

/// The rows to write so that `id` becomes the only enabled plugin of
/// `goal`, the target first; `id` `None` turns every plugin of the goal off.
/// Returns `None` when `id` is not a live row of the goal.
pub fn enable(rows: &[Plugin], goal: &str, id: Option<&str>) -> Option<Vec<Plugin>> {
    let mut out = Vec::new();
    if let Some(id) = id {
        let target = rows.iter().filter(|p| belongs(p, goal)).find(|p| p.id == id)?;
        if !target.enabled {
            out.push(Plugin { enabled: true, ..target.clone() });
        }
    }
    out.extend(rows.iter().filter(|p| belongs(p, goal)).filter(|p| p.enabled && Some(p.id.as_str()) != id).map(|p| Plugin { enabled: false, ..p.clone() }));
    Some(out)
}
