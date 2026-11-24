//! Aroh plugins: a plugin is one TOML bundle of tracker schemas, a view layout of the
//! approved component kinds bound to those trackers, and optional personas and tools.
//! No plugin code. This crate parses and strictly validates bundles, versions them and
//! ships the built-in templates. No I/O.

pub mod bind;
pub mod contract;
pub mod versions;
mod view;

use aroh_domain::rows::{Cadence, Record, Tracker, TrackerField, TrackerRule};
use serde_json::{json, Value as Json};
use std::collections::{BTreeMap, BTreeSet};
use toml::{Table, Value};

/// The bundle format version every bundle declares as `aroh_plugin`.
/// Only 1 exists.
/// The row `version` is the revision, a separate axis.
pub const FORMAT: i64 = 1;

/// Largest bundle in bytes, the same cap the `plugins` row puts on `bundle_toml`.
/// Larger text is rejected before parsing.
/// Equal to the domain's TOML limit.
pub const MAX_BYTES: usize = 64 * 1024;

/// Caps on a bundle's lists and texts, so a generated bundle stays small.
/// Checked by the validator.
/// Each error names the list that overflowed.
pub const MAX_TRACKERS: usize = 12;
pub const MAX_PERSONAS: usize = 8;
pub const MAX_TOOLS: usize = 16;
pub const MAX_SECTIONS: usize = 12;
pub const MAX_ITEMS: usize = 24;
pub const MAX_LIST: usize = 50;
pub const MAX_TEXT: usize = 500;

const PLACEHOLDER_ID: &str = "00000000-0000-7000-8000-000000000000";

/// The built-in templates by name: the bundles an owner can reuse and the
/// plugin-builder persona starts from.
/// Each one validates (tested).
pub const TEMPLATES: &[(&str, &str)] = &[
    ("fitness", include_str!("../templates/fitness.toml")),
    ("music", include_str!("../templates/music.toml")),
    ("learning", include_str!("../templates/learning.toml")),
    ("finance", include_str!("../templates/finance.toml")),
    ("habits", include_str!("../templates/habits.toml")),
];

/// Returns the built-in template called `name`, if any.
/// The text is a complete bundle.
/// Linear scan.
pub fn template(name: &str) -> Option<&'static str> {
    TEMPLATES.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
}

/// The bundle vocabulary as text for a model that writes bundles: every kind
/// as its TypeScript shape, the named types, and every binding source with
/// its keys and the type it gives. Generated from the validator's own tables.
pub fn reference() -> String {
    let mut out = String::from("Item kinds (TypeScript shape; a data prop takes a binding table instead of a literal):\n");
    for k in contract::KINDS {
        out.push_str(&contract::kind_ts(k));
        out.push('\n');
    }
    out.push_str("Named types:\n");
    for n in contract::NAMED {
        out.push_str(&format!("{} = {}\n", n.name, contract::ts(&n.ty)));
    }
    out.push_str("Binding sources, written as { bind = \"<source>\", ... }, with the keys each takes and the type it gives:\n");
    for s in bind::SOURCES {
        out.push_str(&format!("{}: keys {}; gives {}\n", s.name, bind::keys(s.reads).join(", "), contract::ts(&s.gives)));
    }
    out.push_str(&format!("`actions` takes when = {} and an optional limit; a streak without tracker reads the goal's actions.\n", bind::WINDOWS.join(" | ")));
    out
}

/// A bundle that fails validation: the file, the field path inside it (for
/// example `sections[0].items[1].points.tracker`) and why.
/// Displays as `file: field: reason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub file: String,
    pub field: String,
    pub reason: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}: {}", self.file, self.field, self.reason)
    }
}

impl std::error::Error for Error {}

/// One tracker a bundle declares: a tracker row's schema, plus a target per
/// number field key for the `target` binding. Matched to the goal's tracker
/// rows by `name`.
#[derive(Debug, Clone, PartialEq)]
pub struct BundleTracker {
    pub name: String,
    pub fields: Vec<TrackerField>,
    pub cadence: Option<Cadence>,
    pub rules: Vec<TrackerRule>,
    pub targets: BTreeMap<String, f64>,
}

/// One persona a bundle carries: a slug name and its persona TOML text.
/// The server checks the text with the persona parser before storing it.
/// Stored as a goal persona.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundlePersona {
    pub name: String,
    pub toml: String,
}

/// A validated bundle: its parts, and `tree`, the JSON the app resolves:
/// `{format, name, title, description, trackers, tools, personas, sections}`
/// with bindings kept as `{bind, ...}` objects.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub trackers: Vec<BundleTracker>,
    pub personas: Vec<BundlePersona>,
    pub tools: Vec<String>,
    pub tree: Json,
}

impl Bundle {
    /// Checks that every tool the bundle names is in `allowed`, the tools the
    /// host can give its personas. Errors name `file` and `tools[i]`.
    /// Hosts call it before storing a bundle.
    pub fn check_tools(&self, file: &str, allowed: &[&str]) -> Result<(), Error> {
        let cx = Cx { file };
        for (i, tool) in self.tools.iter().enumerate() {
            if !allowed.contains(&tool.as_str()) {
                return Err(cx.err(Cx::index("tools", i), format!("unknown tool `{tool}`, expected one of {}", allowed.join(", "))));
            }
        }
        Ok(())
    }
}

/// The tracker rows to create so every tracker `bundle` declares exists for
/// `goal` of `user`: one per bundle tracker with no live row of that name in
/// `existing`, with an id derived from goal and name so every engine agrees.
pub fn missing_trackers(bundle: &Bundle, existing: &[Tracker], user: &str, goal: &str) -> Vec<Tracker> {
    bundle
        .trackers
        .iter()
        .filter(|t| !existing.iter().any(|e| !e.deleted && e.goal_id == goal && e.name == t.name))
        .map(|t| Tracker {
            id: aroh_domain::derived_id(&format!("plugin-tracker:{goal}"), &t.name, 0),
            user_id: user.into(),
            goal_id: goal.into(),
            plugin_id: None,
            name: t.name.clone(),
            fields: t.fields.clone(),
            cadence: t.cadence.clone(),
            rules: t.rules.clone(),
            updated_at: String::new(),
            deleted: false,
        })
        .collect()
}

/// Validation context: the file name every error carries, plus the small
/// readers that turn a missing or mistyped key into an error naming the
/// field path.
pub(crate) struct Cx<'a> {
    file: &'a str,
}

impl Cx<'_> {
    fn err(&self, field: impl AsRef<str>, reason: impl Into<String>) -> Error {
        let field = field.as_ref();
        Error { file: self.file.into(), field: if field.is_empty() { "(bundle)".into() } else { field.into() }, reason: reason.into() }
    }

    fn at(path: &str, key: &str) -> String {
        if path.is_empty() {
            key.into()
        } else {
            format!("{path}.{key}")
        }
    }

    fn index(path: &str, i: usize) -> String {
        format!("{path}[{i}]")
    }

    fn closed(&self, t: &Table, path: &str, allowed: &[&str]) -> Result<(), Error> {
        match t.keys().find(|k| !allowed.contains(&k.as_str())) {
            Some(k) => Err(self.err(Cx::at(path, k), format!("unknown key, expected one of {}", allowed.join(", ")))),
            None => Ok(()),
        }
    }

    fn check_text(&self, path: &str, s: &str, min: usize, max: usize) -> Result<(), Error> {
        let n = s.chars().count();
        if n < min || n > max {
            return Err(self.err(path, format!("length {n} outside {min} to {max}")));
        }
        if s.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\t')) {
            return Err(self.err(path, "control character"));
        }
        Ok(())
    }

    fn text(&self, t: &Table, path: &str, key: &str, min: usize, max: usize) -> Result<String, Error> {
        let here = Cx::at(path, key);
        let s = t.get(key).ok_or_else(|| self.err(&here, "missing"))?.as_str().ok_or_else(|| self.err(&here, "expected a string"))?;
        self.check_text(&here, s, min, max)?;
        Ok(s.into())
    }

    fn slug(&self, t: &Table, path: &str, key: &str, max: usize) -> Result<String, Error> {
        let s = self.text(t, path, key, 2, max)?;
        let ok = s.starts_with(|c: char| c.is_ascii_lowercase()) && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
        if !ok {
            return Err(self.err(Cx::at(path, key), "a slug: a-z first, then a-z, 0-9, - and _"));
        }
        Ok(s)
    }

    fn list<'v>(&self, v: Option<&'v Value>, path: &str, min: usize, max: usize) -> Result<&'v [Value], Error> {
        let list = match v {
            None if min == 0 => return Ok(&[]),
            None => return Err(self.err(path, "missing")),
            Some(v) => v.as_array().ok_or_else(|| self.err(path, "expected a list"))?,
        };
        if list.len() < min || list.len() > max {
            return Err(self.err(path, format!("{} entries, expected {min} to {max}", list.len())));
        }
        Ok(list)
    }

    fn serde<T: serde::de::DeserializeOwned>(&self, path: &str, v: Json) -> Result<T, Error> {
        serde_json::from_value(v).map_err(|e| self.err(path, e.to_string()))
    }
}

fn tracker(cx: &Cx, path: &str, v: &Value) -> Result<BundleTracker, Error> {
    let t = v.as_table().ok_or_else(|| cx.err(path, "expected a table"))?;
    cx.closed(t, path, &["name", "fields", "cadence", "rules"])?;
    let name = cx.text(t, path, "name", 1, 100)?;
    let fields_path = Cx::at(path, "fields");
    let mut fields = Vec::new();
    let mut targets = BTreeMap::new();
    for (j, f) in cx.list(t.get("fields"), &fields_path, 1, 20)?.iter().enumerate() {
        let here = Cx::index(&fields_path, j);
        let mut table = f.as_table().ok_or_else(|| cx.err(&here, "expected a table"))?.clone();
        let target = table.remove("target");
        let field: TrackerField = cx.serde(&here, serde_json::to_value(&table).expect("toml tables serialize"))?;
        if let Some(target) = target {
            let n = target.as_float().or_else(|| target.as_integer().map(|i| i as f64)).filter(|n| n.is_finite());
            let n = n.ok_or_else(|| cx.err(Cx::at(&here, "target"), "expected a finite number"))?;
            if field.kind != aroh_domain::rows::FieldKind::Number {
                return Err(cx.err(Cx::at(&here, "target"), "only number fields have a target"));
            }
            targets.insert(field.key.clone(), n);
        }
        fields.push(field);
    }
    let cadence: Option<Cadence> = match t.get("cadence") {
        Some(c) => Some(cx.serde(&Cx::at(path, "cadence"), serde_json::to_value(c).expect("toml values serialize"))?),
        None => None,
    };
    let rules: Vec<TrackerRule> = match t.get("rules") {
        Some(r) => cx.serde(&Cx::at(path, "rules"), serde_json::to_value(r).expect("toml values serialize"))?,
        None => Vec::new(),
    };
    let row = Tracker {
        id: PLACEHOLDER_ID.into(),
        user_id: PLACEHOLDER_ID.into(),
        goal_id: PLACEHOLDER_ID.into(),
        plugin_id: None,
        name: name.clone(),
        fields: fields.clone(),
        cadence: cadence.clone(),
        rules: rules.clone(),
        updated_at: aroh_domain::seed::SEED_STAMP.into(),
        deleted: false,
    };
    row.validate().map_err(|e| cx.err(path, e.to_string()))?;
    Ok(BundleTracker { name, fields, cadence, rules, targets })
}

fn tracker_json(t: &BundleTracker) -> Json {
    let fields: Vec<Json> = t
        .fields
        .iter()
        .map(|f| {
            let mut v = serde_json::to_value(f).expect("fields serialize");
            if let Some(target) = t.targets.get(&f.key) {
                v["target"] = json!(target);
            }
            v
        })
        .collect();
    json!({"name": t.name, "fields": fields})
}

/// Parses and validates bundle `text`, read from `file`: format, names,
/// limits, closed maps, trackers (by the domain's tracker rules), personas,
/// tools and every view item and binding. Returns the bundle or the first error.
pub fn parse(file: &str, text: &str) -> Result<Bundle, Error> {
    assert!(!file.is_empty(), "errors name a file");
    let cx = Cx { file };
    if text.len() > MAX_BYTES {
        return Err(cx.err("", format!("{} bytes, at most {MAX_BYTES}", text.len())));
    }
    let root: Table = toml::from_str(text).map_err(|e| cx.err("(toml)", e.to_string().trim().replace('\n', " ")))?;
    cx.closed(&root, "", &["aroh_plugin", "name", "title", "description", "tools", "trackers", "personas", "sections"])?;
    if root.get("aroh_plugin").and_then(Value::as_integer) != Some(FORMAT) {
        return Err(cx.err("aroh_plugin", format!("must be {FORMAT}")));
    }
    let name = cx.slug(&root, "", "name", 40)?;
    let title = cx.text(&root, "", "title", 1, 80)?;
    let description = match root.get("description") {
        Some(_) => Some(cx.text(&root, "", "description", 1, MAX_TEXT)?),
        None => None,
    };
    let mut tools = Vec::new();
    for (i, tool) in cx.list(root.get("tools"), "tools", 0, MAX_TOOLS)?.iter().enumerate() {
        let here = Cx::index("tools", i);
        let tool = tool.as_str().ok_or_else(|| cx.err(&here, "expected a string"))?;
        if tools.iter().any(|t| t == tool) {
            return Err(cx.err(&here, format!("duplicate tool `{tool}`")));
        }
        tools.push(tool.to_string());
    }
    let mut trackers: Vec<BundleTracker> = Vec::new();
    for (i, t) in cx.list(root.get("trackers"), "trackers", 0, MAX_TRACKERS)?.iter().enumerate() {
        let path = Cx::index("trackers", i);
        let tracker = tracker(&cx, &path, t)?;
        if trackers.iter().any(|x| x.name == tracker.name) {
            return Err(cx.err(Cx::at(&path, "name"), format!("duplicate tracker name `{}`", tracker.name)));
        }
        trackers.push(tracker);
    }
    let mut personas: Vec<BundlePersona> = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, p) in cx.list(root.get("personas"), "personas", 0, MAX_PERSONAS)?.iter().enumerate() {
        let path = Cx::index("personas", i);
        let t = p.as_table().ok_or_else(|| cx.err(&path, "expected a table"))?;
        cx.closed(t, &path, &["name", "toml"])?;
        let name = cx.slug(t, &path, "name", 60)?;
        if !seen.insert(name.clone()) {
            return Err(cx.err(Cx::at(&path, "name"), format!("duplicate persona `{name}`")));
        }
        personas.push(BundlePersona { name, toml: cx.text(t, &path, "toml", 1, MAX_BYTES)? });
    }
    let sections = view::sections(&cx, root.get("sections"), &trackers)?;
    let tree = json!({
        "format": FORMAT,
        "name": name,
        "title": title,
        "description": description,
        "trackers": trackers.iter().map(tracker_json).collect::<Vec<_>>(),
        "tools": tools,
        "personas": personas.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        "sections": sections,
    });
    if let Some(line) = url_line(text) {
        return Err(cx.err("(bundle)", format!("line {line}: literal URL not allowed")));
    }
    Ok(Bundle { name, title, description, trackers, personas, tools, tree })
}

/// URL markers a bundle may not carry anywhere, matched without case.
const URL_MARKERS: [&str; 3] = ["://", "www.", "javascript:"];

/// The first 1-based line of `text` holding a URL marker, or None. Bundles are
/// untrusted, so no text in one, persona files included, may point anywhere.
fn url_line(text: &str) -> Option<usize> {
    text.lines().position(|l| {
        let l = l.to_ascii_lowercase();
        URL_MARKERS.iter().any(|m| l.contains(m))
    }).map(|i| i + 1)
}
