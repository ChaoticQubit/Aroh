//! The binding vocabulary: each source, the keys it takes and the type it fills.
//! Values are computed by the app's resolver; this module only checks bindings.

use crate::contract::{ts, Ty, ACTIONS, DAYS, LOG_ENTRIES, POINTS};
use crate::{Cx, Error, BundleTracker, MAX_LIST};
use aroh_domain::rows::FieldKind;
use serde_json::Value as Json;
use toml::Table;

/// What a source reads: a number field of a tracker, a tracker, a tracker
/// or the goal's actions, an action window, or only the goal.
/// Decides which keys a binding of the source takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reads {
    Field,
    Target,
    Tracker,
    TrackerOrActions,
    Actions,
    Goal,
}

/// One binding source: its name, what it reads and the type of the value it
/// gives. A binding fits a prop when the texts of both types are equal.
/// Listed in `SOURCES`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Source {
    pub name: &'static str,
    pub reads: Reads,
    pub gives: Ty,
}

/// Every binding source, in the order `resolve.ts` lists them.
/// The drift test compares the names with the app's `SOURCES`.
/// Field sources need a number field.
pub const SOURCES: &[Source] = &[
    Source { name: "series", reads: Reads::Field, gives: POINTS },
    Source { name: "latest", reads: Reads::Field, gives: Ty::Num },
    Source { name: "today_total", reads: Reads::Field, gives: Ty::Num },
    Source { name: "change", reads: Reads::Field, gives: Ty::Num },
    Source { name: "target", reads: Reads::Target, gives: Ty::Num },
    Source { name: "entries", reads: Reads::Tracker, gives: LOG_ENTRIES },
    Source { name: "streak", reads: Reads::TrackerOrActions, gives: DAYS },
    Source { name: "actions", reads: Reads::Actions, gives: ACTIONS },
    Source { name: "week_done", reads: Reads::Goal, gives: Ty::Num },
    Source { name: "week_planned", reads: Reads::Goal, gives: Ty::Num },
    Source { name: "week_start", reads: Reads::Goal, gives: Ty::Str },
    Source { name: "deadline", reads: Reads::Goal, gives: Ty::Str },
];

/// Action windows of the `actions` source.
/// `next` is planned actions after now, `today` and `week` are by local date.
/// Checked against `when`.
pub const WINDOWS: &[&str] = &["next", "today", "week"];

/// Returns the source called `name`, if any.
/// Linear scan.
/// Used by the validator.
pub fn source(name: &str) -> Option<&'static Source> {
    SOURCES.iter().find(|s| s.name == name)
}

fn fits(expected: &Ty, gives: &Ty) -> bool {
    let want = ts(expected);
    want == ts(gives) || matches!(expected, Ty::Null(inner) if ts(inner) == ts(gives))
}

/// The keys a binding of a source reading `reads` may carry, `bind` first.
/// Every key but `limit` and a streak's `tracker` is required.
/// Shown to the plugin builder by `reference`.
pub fn keys(reads: Reads) -> &'static [&'static str] {
    match reads {
        Reads::Field | Reads::Target => &["bind", "tracker", "field"],
        Reads::Tracker | Reads::TrackerOrActions => &["bind", "tracker"],
        Reads::Actions => &["bind", "when", "limit"],
        Reads::Goal => &["bind"],
    }
}

/// Checks binding table `t` at `path` for a prop of type `expected`: a known
/// source, only its keys, a type that fits, and every tracker, field and
/// target it names declared in `trackers`. Returns the binding as JSON.
pub(crate) fn check(cx: &Cx, path: &str, t: &Table, expected: &Ty, trackers: &[BundleTracker]) -> Result<Json, Error> {
    let name = cx.text(t, path, "bind", 1, 40)?;
    let Some(src) = source(&name) else {
        let known: Vec<&str> = SOURCES.iter().map(|s| s.name).collect();
        return Err(cx.err(Cx::at(path, "bind"), format!("unknown source `{name}`, expected one of {}", known.join(", "))));
    };
    cx.closed(t, path, keys(src.reads))?;
    if !fits(expected, &src.gives) {
        return Err(cx.err(path, format!("`{name}` gives {}, this prop needs {}", ts(&src.gives), ts(expected))));
    }
    let tracker = match src.reads {
        Reads::Goal | Reads::Actions => None,
        Reads::TrackerOrActions if !t.contains_key("tracker") => None,
        _ => {
            let wanted = cx.text(t, path, "tracker", 1, 100)?;
            let found = trackers.iter().find(|x| x.name == wanted);
            Some(found.ok_or_else(|| cx.err(Cx::at(path, "tracker"), format!("no tracker named `{wanted}`")))?)
        }
    };
    if let (Some(tracker), Reads::Field | Reads::Target) = (tracker, src.reads) {
        let key = cx.text(t, path, "field", 1, 40)?;
        let field_path = Cx::at(path, "field");
        let field = tracker.fields.iter().find(|f| f.key == key).ok_or_else(|| cx.err(&field_path, format!("`{}` has no field `{key}`", tracker.name)))?;
        if field.kind != FieldKind::Number {
            return Err(cx.err(&field_path, format!("`{key}` is not a number field")));
        }
        if src.reads == Reads::Target && !tracker.targets.contains_key(&key) {
            return Err(cx.err(&field_path, format!("`{key}` declares no target")));
        }
    }
    if src.reads == Reads::Actions {
        let when = cx.text(t, path, "when", 1, 10)?;
        if !WINDOWS.contains(&when.as_str()) {
            return Err(cx.err(Cx::at(path, "when"), format!("`{when}` is not one of {}", WINDOWS.join(", "))));
        }
        if let Some(limit) = t.get("limit") {
            let n = limit.as_integer().filter(|n| (1..=MAX_LIST as i64).contains(n));
            n.ok_or_else(|| cx.err(Cx::at(path, "limit"), format!("a whole number from 1 to {MAX_LIST}")))?;
        }
    }
    Ok(serde_json::to_value(t).expect("toml tables serialize"))
}
