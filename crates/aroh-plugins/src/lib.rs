use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Number, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

pub const MAX_FILE_BYTES: usize = 64 * 1024;
pub const MAX_COMPONENTS: usize = 32;
pub const MAX_COMPONENTS_PER_SLOT: usize = 8;
pub const MAX_TRACKERS: usize = 8;
pub const MAX_AGENTS: usize = 8;
pub const MAX_PLUGINS: usize = 64;
const MAX_TOOLS: usize = 8;
const MAX_CHIPS: usize = 8;
const MAX_PROMPT_BYTES: usize = 8192;
const AGENT_FILE: &str = "<agent>";

/// A rejected plugin or agent file: the file, the 1-based line when known,
/// the path inside the document (`.` for the root) and a message naming the problem.
/// Displays as `<file>:<line>: <path>: <message>`, without the line when unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginError {
    pub file: String,
    pub line: Option<usize>,
    pub path: String,
    pub message: String,
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "{}:{}: {}: {}", self.file, line, self.path, self.message),
            None => write!(f, "{}: {}: {}", self.file, self.path, self.message),
        }
    }
}

impl std::error::Error for PluginError {}

/// A named UI slot a plugin can place components into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Slot {
    #[serde(rename = "today.top")]
    TodayTop,
    #[serde(rename = "today.sidebar")]
    TodaySidebar,
    #[serde(rename = "goal.detail")]
    GoalDetail,
    #[serde(rename = "log.trackers")]
    LogTrackers,
}

impl Slot {
    pub const ALL: [Slot; 4] = [Slot::TodayTop, Slot::TodaySidebar, Slot::GoalDetail, Slot::LogTrackers];

    /// The slot name as written in plugin files and the render model.
    pub fn as_str(self) -> &'static str {
        match self {
            Slot::TodayTop => "today.top",
            Slot::TodaySidebar => "today.sidebar",
            Slot::GoalDetail => "goal.detail",
            Slot::LogTrackers => "log.trackers",
        }
    }
}

/// A component from the fixed component list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentKind {
    StatTile,
    ProgressBar,
    Checklist,
    ActionList,
    QuickChips,
    TextNote,
}

impl ComponentKind {
    fn as_str(self) -> &'static str {
        match self {
            ComponentKind::StatTile => "stat-tile",
            ComponentKind::ProgressBar => "progress-bar",
            ComponentKind::Checklist => "checklist",
            ComponentKind::ActionList => "action-list",
            ComponentKind::QuickChips => "quick-chips",
            ComponentKind::TextNote => "text-note",
        }
    }

    /// Required and optional prop names of this component, in render order.
    fn props(self) -> (&'static [&'static str], &'static [&'static str]) {
        match self {
            ComponentKind::StatTile => (&["label", "value"], &["unit"]),
            ComponentKind::ProgressBar => (&["label", "value", "max"], &[]),
            ComponentKind::Checklist => (&["label", "items"], &[]),
            ComponentKind::ActionList => (&["label", "items"], &["limit"]),
            ComponentKind::QuickChips => (&["tracker"], &["label"]),
            ComponentKind::TextNote => (&["text"], &[]),
        }
    }
}

/// A tool an agent may be granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    ReadGoals,
    ReadActions,
    ReadTrackers,
    LogTracker,
    ProposeActions,
    FetchPage,
    Notify,
}

/// How hard the host lets the agent think.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    #[default]
    Medium,
    High,
}

/// The `tracker_entries.kind` a tracker logs into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackerKind {
    Food,
    Water,
    Workout,
    Other,
}

/// An agent declaration, from a plugin's `agents` list or a standalone agent file.
/// The host fills every `{{name}}` variable and picks the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDecl {
    pub name: String,
    pub purpose: String,
    pub tools: Vec<Tool>,
    #[serde(default)]
    pub variables: Vec<String>,
    #[serde(default)]
    pub untrusted: Vec<String>,
    #[serde(default)]
    pub effort: Effort,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub system: Option<String>,
    pub prompt: String,
}

/// A tracker type declaration. Absent `chips` render as an empty list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackerDecl {
    pub id: String,
    pub kind: TrackerKind,
    pub unit: String,
    #[serde(default)]
    pub chips: Vec<Number>,
    #[serde(default)]
    pub daily_target: Option<Number>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentDoc {
    component: ComponentKind,
    label: Option<String>,
    value: Option<String>,
    unit: Option<String>,
    max: Option<serde_yaml::Value>,
    items: Option<String>,
    limit: Option<i64>,
    tracker: Option<String>,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginDoc {
    aroh_plugin: u32,
    name: String,
    version: String,
    title: String,
    description: Option<String>,
    order: Option<u32>,
    #[serde(default)]
    trackers: Vec<TrackerDecl>,
    #[serde(default)]
    slots: BTreeMap<Slot, Vec<ComponentDoc>>,
    #[serde(default)]
    agents: Vec<AgentDecl>,
}

#[derive(Debug, Clone)]
struct Instance {
    component: ComponentKind,
    props: Map<String, Value>,
    tracker: Option<String>,
}

/// One validated plugin file, ready for `resolve`.
#[derive(Debug, Clone)]
pub struct Plugin {
    file_name: String,
    source: String,
    info: PluginInfo,
    order: u32,
    trackers: Vec<TrackerDecl>,
    agents: Vec<AgentDecl>,
    slots: BTreeMap<Slot, Vec<Instance>>,
}

impl Plugin {
    /// The plugin `name`.
    pub fn name(&self) -> &str {
        &self.info.name
    }

    /// The plugin name, version and title.
    pub fn info(&self) -> &PluginInfo {
        &self.info
    }

    /// The YAML text this plugin was parsed from.
    pub fn yaml(&self) -> &str {
        &self.source
    }

    /// The agents this plugin declares, in file order.
    pub fn agents(&self) -> &[AgentDecl] {
        &self.agents
    }
}

/// Name, version and title of a loaded plugin.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub title: String,
}

/// One component instance placed in a slot.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderInstance {
    pub key: String,
    pub plugin: String,
    pub component: ComponentKind,
    pub props: Map<String, Value>,
}

/// A tracker with the plugin that declared it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderTracker {
    pub plugin: String,
    #[serde(flatten)]
    pub tracker: TrackerDecl,
}

/// An agent with the plugin that declared it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderAgent {
    pub plugin: String,
    #[serde(flatten)]
    pub agent: AgentDecl,
}

/// The JSON render model the UI reads. Every slot key is present.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderModel {
    pub plugins: Vec<PluginInfo>,
    pub slots: BTreeMap<String, Vec<RenderInstance>>,
    pub trackers: Vec<RenderTracker>,
    pub agents: Vec<RenderAgent>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum QueryType {
    Number,
    List,
}

impl QueryType {
    fn as_str(self) -> &'static str {
        match self {
            QueryType::Number => "number",
            QueryType::List => "list",
        }
    }
}

const QUERIES: [(&str, QueryType); 7] = [
    ("water.today_total", QueryType::Number),
    ("food.today_total", QueryType::Number),
    ("workout.today_total", QueryType::Number),
    ("goal.progress", QueryType::Number),
    ("goal.actions_left", QueryType::Number),
    ("goal.next_actions", QueryType::List),
    ("today.actions", QueryType::List),
];

struct Ctx<'a> {
    file: &'a str,
    src: &'a str,
}

impl Ctx<'_> {
    fn err(&self, path: &str, needle: Option<&str>, message: String) -> PluginError {
        let line = needle.and_then(|n| self.src.lines().position(|l| l.contains(n)).map(|i| i + 1));
        PluginError { file: self.file.to_string(), line, path: path.to_string(), message }
    }

    fn yaml(&self, e: serde_yaml::Error) -> PluginError {
        let mut msg = e.to_string();
        let line = e.location().map(|l| l.line());
        if let Some(loc) = e.location() {
            let suffix = format!(" at line {} column {}", loc.line(), loc.column());
            if let Some(i) = msg.find(&suffix) {
                msg.replace_range(i..i + suffix.len(), "");
            }
        }
        let (path, message) = match msg.split_once(": ") {
            Some((p, m)) if !p.is_empty() && !p.contains(' ') => (p.to_string(), m.to_string()),
            _ => (".".to_string(), msg),
        };
        PluginError { file: self.file.to_string(), line, path, message }
    }

    fn text(&self, path: &str, s: &str, max_chars: usize) -> Result<(), PluginError> {
        let n = s.chars().count();
        if n == 0 || n > max_chars {
            return Err(self.err(path, Some(s).filter(|s| !s.is_empty()), format!("must be 1 to {max_chars} characters, got {n}")));
        }
        Ok(())
    }

    fn bytes(&self, path: &str, s: &str, max: usize) -> Result<(), PluginError> {
        if s.is_empty() || s.len() > max {
            return Err(self.err(path, None, format!("must be 1 to {max} bytes, got {}", s.len())));
        }
        Ok(())
    }

    fn query(&self, path: &str, name: &str, want: QueryType, slot: Slot) -> Result<Value, PluginError> {
        let Some(&(_, got)) = QUERIES.iter().find(|(q, _)| *q == name) else {
            let known: Vec<&str> = QUERIES.iter().map(|(q, _)| *q).collect();
            return Err(self.err(path, Some(name), format!("unknown query `{name}`; expected one of {}", known.join(", "))));
        };
        if name.starts_with("goal.") && slot != Slot::GoalDetail {
            return Err(self.err(path, Some(name), format!("query `{name}` only works in the goal.detail slot, not {}", slot.as_str())));
        }
        if got != want {
            return Err(self.err(path, Some(name), format!("query `{name}` is a {} query, but this prop needs a {} query", got.as_str(), want.as_str())));
        }
        Ok(json!({ "query": name }))
    }
}

fn join(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

/// True when `s` is a lowercase letter followed by lowercase letters, digits or `sep`,
/// with a total length in `min..=max`.
fn ident(s: &str, sep: u8, min: usize, max: usize) -> bool {
    let b = s.as_bytes();
    (min..=max).contains(&b.len())
        && b[0].is_ascii_lowercase()
        && b[1..].iter().all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == sep)
}

fn plugin_name(s: &str) -> bool {
    ident(s, b'-', 2, 40)
}

fn variable_name(s: &str) -> bool {
    ident(s, b'_', 1, 32)
}

fn semver(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            (1..=9).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_digit()) && (p.len() == 1 || !p.starts_with('0'))
        })
}

fn positive(n: &Number) -> bool {
    n.as_f64().is_some_and(|f| f > 0.0)
}

/// The text between each `{{` and its `}}`, or the byte offset of an unclosed `{{`.
fn placeholders(s: &str) -> Result<Vec<&str>, usize> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find("{{") {
        let after = &rest[i + 2..];
        let Some(j) = after.find("}}") else {
            return Err(s.len() - rest.len() + i);
        };
        out.push(&after[..j]);
        rest = &after[j + 2..];
    }
    Ok(out)
}

fn check_agent(cx: &Ctx, a: &AgentDecl, prefix: &str) -> Result<(), PluginError> {
    let p = |k: &str| join(prefix, k);
    if !plugin_name(&a.name) {
        return Err(cx.err(&p("name"), Some(&a.name), format!("bad agent name `{}`; expected ^[a-z][a-z0-9-]{{1,39}}$", a.name)));
    }
    cx.text(&p("purpose"), &a.purpose, 200)?;
    if a.tools.len() > MAX_TOOLS {
        return Err(cx.err(&p("tools"), None, format!("at most {MAX_TOOLS} tools, got {}", a.tools.len())));
    }
    let mut tools = HashSet::new();
    for t in &a.tools {
        if !tools.insert(t) {
            return Err(cx.err(&p("tools"), None, format!("tool `{}` is listed twice", serde_json::to_value(t).unwrap_or_default().as_str().unwrap_or("?"))));
        }
    }
    let mut vars = HashSet::new();
    for (i, v) in a.variables.iter().enumerate() {
        let path = format!("{}[{i}]", p("variables"));
        if !variable_name(v) {
            return Err(cx.err(&path, Some(v), format!("bad variable name `{v}`; expected ^[a-z][a-z0-9_]{{0,31}}$")));
        }
        if !vars.insert(v.as_str()) {
            return Err(cx.err(&path, Some(v), format!("variable `{v}` is listed twice")));
        }
    }
    let mut untrusted = HashSet::new();
    for (i, v) in a.untrusted.iter().enumerate() {
        let path = format!("{}[{i}]", p("untrusted"));
        if !vars.contains(v.as_str()) {
            return Err(cx.err(&path, Some(v), format!("untrusted variable `{v}` is not declared in variables")));
        }
        if !untrusted.insert(v.as_str()) {
            return Err(cx.err(&path, Some(v), format!("untrusted variable `{v}` is listed twice")));
        }
    }
    if let Some(o) = &a.output {
        if !ident(o, b'_', 2, 40) {
            return Err(cx.err(&p("output"), Some(o), format!("bad output schema name `{o}`; expected ^[a-z][a-z0-9_]{{1,39}}$")));
        }
    }
    let mut used = HashSet::new();
    let texts = [("system", a.system.as_deref()), ("prompt", Some(a.prompt.as_str()))];
    for (key, text) in texts {
        let Some(text) = text else { continue };
        cx.bytes(&p(key), text, MAX_PROMPT_BYTES)?;
        let names = placeholders(text).map_err(|_| cx.err(&p(key), Some("{{"), "unclosed `{{`".to_string()))?;
        for name in names {
            if !variable_name(name) {
                return Err(cx.err(&p(key), Some(&format!("{{{{{name}}}}}")), format!("bad placeholder `{{{{{name}}}}}`; expected {{{{name}}}} with a declared variable")));
            }
            if !vars.contains(name) {
                return Err(cx.err(&p(key), Some(&format!("{{{{{name}}}}}")), format!("variable `{name}` is used but not declared in variables")));
            }
            used.insert(name);
        }
    }
    for (i, v) in a.variables.iter().enumerate() {
        if !used.contains(v.as_str()) {
            return Err(cx.err(&format!("{}[{i}]", p("variables")), Some(v), format!("variable `{v}` is declared but not used in prompt or system")));
        }
    }
    Ok(())
}

fn check_tracker(cx: &Ctx, t: &TrackerDecl, prefix: &str) -> Result<(), PluginError> {
    if !ident(&t.id, b'_', 2, 32) {
        return Err(cx.err(&join(prefix, "id"), Some(&t.id), format!("bad tracker id `{}`; expected ^[a-z][a-z0-9_]{{1,31}}$", t.id)));
    }
    cx.text(&join(prefix, "unit"), &t.unit, 16)?;
    if t.chips.len() > MAX_CHIPS {
        return Err(cx.err(&join(prefix, "chips"), None, format!("at most {MAX_CHIPS} chips, got {}", t.chips.len())));
    }
    if let Some(i) = t.chips.iter().position(|c| !positive(c)) {
        return Err(cx.err(&format!("{}[{i}]", join(prefix, "chips")), None, format!("chip `{}` must be a positive number", t.chips[i])));
    }
    if let Some(d) = t.daily_target.as_ref().filter(|d| !positive(d)) {
        return Err(cx.err(&join(prefix, "daily_target"), None, format!("daily_target `{d}` must be a positive number")));
    }
    Ok(())
}

fn check_component(cx: &Ctx, c: ComponentDoc, slot: Slot, prefix: &str) -> Result<Instance, PluginError> {
    let kind = c.component;
    let present = [
        ("label", c.label.is_some()),
        ("value", c.value.is_some()),
        ("unit", c.unit.is_some()),
        ("max", c.max.is_some()),
        ("items", c.items.is_some()),
        ("limit", c.limit.is_some()),
        ("tracker", c.tracker.is_some()),
        ("text", c.text.is_some()),
    ];
    let (required, optional) = kind.props();
    for (key, is_set) in present {
        if is_set && !required.contains(&key) && !optional.contains(&key) {
            return Err(cx.err(&join(prefix, key), Some(&format!("{key}:")), format!("unknown prop `{key}` for {}; expected {}", kind.as_str(), [required, optional].concat().join(", "))));
        }
    }
    for (key, is_set) in present {
        if !is_set && required.contains(&key) {
            return Err(cx.err(prefix, None, format!("missing prop `{key}` for {}", kind.as_str())));
        }
    }
    let mut props = Map::new();
    if let Some(s) = &c.label {
        cx.text(&join(prefix, "label"), s, 80)?;
        props.insert("label".into(), json!(s));
    }
    if let Some(s) = &c.value {
        props.insert("value".into(), cx.query(&join(prefix, "value"), s, QueryType::Number, slot)?);
    }
    if let Some(s) = &c.unit {
        cx.text(&join(prefix, "unit"), s, 80)?;
        props.insert("unit".into(), json!(s));
    }
    if let Some(m) = &c.max {
        let path = join(prefix, "max");
        let v = match m {
            serde_yaml::Value::String(q) => cx.query(&path, q, QueryType::Number, slot)?,
            serde_yaml::Value::Number(n) => {
                let n = n.as_u64().map(Number::from).or_else(|| n.as_i64().map(Number::from)).or_else(|| n.as_f64().and_then(Number::from_f64));
                match n.filter(positive) {
                    Some(n) => Value::Number(n),
                    None => return Err(cx.err(&path, Some("max:"), "max must be a number greater than 0".into())),
                }
            }
            _ => return Err(cx.err(&path, Some("max:"), "invalid type for max: expected a number or a number query".into())),
        };
        props.insert("max".into(), v);
    }
    if let Some(s) = &c.items {
        props.insert("items".into(), cx.query(&join(prefix, "items"), s, QueryType::List, slot)?);
    }
    if kind == ComponentKind::ActionList {
        let limit = c.limit.unwrap_or(5);
        if !(1..=20).contains(&limit) {
            return Err(cx.err(&join(prefix, "limit"), Some("limit:"), format!("limit must be 1 to 20, got {limit}")));
        }
        props.insert("limit".into(), json!(limit));
    }
    if let Some(t) = &c.tracker {
        if !ident(t, b'_', 2, 32) {
            return Err(cx.err(&join(prefix, "tracker"), Some(t), format!("bad tracker id `{t}`")));
        }
        props.insert("tracker".into(), json!(t));
    }
    if let Some(s) = &c.text {
        cx.text(&join(prefix, "text"), s, 500)?;
        props.insert("text".into(), json!(s));
    }
    Ok(Instance { component: kind, props, tracker: c.tracker })
}

/// Parses one plugin file and runs every single-file check and limit.
/// `file_name` only labels errors. Cross-plugin checks happen in `resolve`.
pub fn parse_plugin(file_name: &str, yaml: &str) -> Result<Plugin, PluginError> {
    let cx = Ctx { file: file_name, src: yaml };
    if yaml.len() > MAX_FILE_BYTES {
        return Err(cx.err(".", None, format!("file is {} bytes; the limit is 64 KiB ({MAX_FILE_BYTES} bytes)", yaml.len())));
    }
    let doc: PluginDoc = serde_yaml::from_str(yaml).map_err(|e| cx.yaml(e))?;
    if doc.aroh_plugin != 1 {
        return Err(cx.err("aroh_plugin", Some("aroh_plugin"), format!("unsupported DSL version {}; only 1 exists", doc.aroh_plugin)));
    }
    if !plugin_name(&doc.name) {
        return Err(cx.err("name", Some(&doc.name), format!("bad plugin name `{}`; expected ^[a-z][a-z0-9-]{{1,39}}$", doc.name)));
    }
    if !semver(&doc.version) {
        return Err(cx.err("version", Some(&doc.version), format!("bad version `{}`; expected MAJOR.MINOR.PATCH", doc.version)));
    }
    cx.text("title", &doc.title, 80)?;
    if let Some(d) = &doc.description {
        if d.chars().count() > 500 {
            return Err(cx.err("description", None, "description must be at most 500 characters".into()));
        }
    }
    let order = doc.order.unwrap_or(500);
    if order > 1000 {
        return Err(cx.err("order", Some("order:"), format!("order must be 0 to 1000, got {order}")));
    }
    if doc.trackers.len() > MAX_TRACKERS {
        return Err(cx.err("trackers", None, format!("at most {MAX_TRACKERS} trackers, got {}", doc.trackers.len())));
    }
    for (i, t) in doc.trackers.iter().enumerate() {
        check_tracker(&cx, t, &format!("trackers[{i}]"))?;
    }
    let mut total = 0;
    let mut slots = BTreeMap::new();
    for (slot, list) in doc.slots {
        let path = format!("slots.{}", slot.as_str());
        if list.len() > MAX_COMPONENTS_PER_SLOT {
            return Err(cx.err(&path, Some(slot.as_str()), format!("at most {MAX_COMPONENTS_PER_SLOT} components per slot, got {}", list.len())));
        }
        total += list.len();
        if total > MAX_COMPONENTS {
            return Err(cx.err("slots", None, format!("at most {MAX_COMPONENTS} components per plugin")));
        }
        let mut out = Vec::with_capacity(list.len());
        for (i, c) in list.into_iter().enumerate() {
            out.push(check_component(&cx, c, slot, &format!("{path}[{i}]"))?);
        }
        slots.insert(slot, out);
    }
    if doc.agents.len() > MAX_AGENTS {
        return Err(cx.err("agents", None, format!("at most {MAX_AGENTS} agents, got {}", doc.agents.len())));
    }
    for (i, a) in doc.agents.iter().enumerate() {
        check_agent(&cx, a, &format!("agents[{i}]"))?;
    }
    Ok(Plugin {
        file_name: file_name.to_string(),
        source: yaml.to_string(),
        info: PluginInfo { name: doc.name, version: doc.version, title: doc.title },
        order,
        trackers: doc.trackers,
        agents: doc.agents,
        slots,
    })
}

/// Parses and checks a standalone agent file, with the agent keys at the top level.
pub fn parse_agent(yaml: &str) -> Result<AgentDecl, PluginError> {
    let cx = Ctx { file: AGENT_FILE, src: yaml };
    if yaml.len() > MAX_FILE_BYTES {
        return Err(cx.err(".", None, format!("file is {} bytes; the limit is 64 KiB ({MAX_FILE_BYTES} bytes)", yaml.len())));
    }
    let agent: AgentDecl = serde_yaml::from_str(yaml).map_err(|e| cx.yaml(e))?;
    check_agent(&cx, &agent, "")?;
    Ok(agent)
}

/// Runs the cross-plugin checks (plugin count, duplicate plugin names, tracker ids
/// and agent names, quick-chips trackers) and builds the render model.
/// Plugins, slot instances, trackers and agents sort by plugin order, then name, then file position.
pub fn resolve(plugins: &[Plugin]) -> Result<RenderModel, PluginError> {
    if plugins.len() > MAX_PLUGINS {
        let extra = &plugins[MAX_PLUGINS];
        let cx = Ctx { file: &extra.file_name, src: &extra.source };
        return Err(cx.err(".", None, format!("{} plugins loaded; the limit is {MAX_PLUGINS}", plugins.len())));
    }
    let mut sorted: Vec<&Plugin> = plugins.iter().collect();
    sorted.sort_by(|a, b| (a.order, &a.info.name).cmp(&(b.order, &b.info.name)));
    let mut names: HashMap<&str, &str> = HashMap::new();
    let mut tracker_ids: HashMap<&str, &str> = HashMap::new();
    let mut agent_names: HashMap<&str, &str> = HashMap::new();
    for p in &sorted {
        let cx = Ctx { file: &p.file_name, src: &p.source };
        if let Some(other) = names.insert(&p.info.name, &p.file_name) {
            return Err(cx.err("name", Some(&p.info.name), format!("duplicate plugin name `{}`; also in {other}", p.info.name)));
        }
        for (i, t) in p.trackers.iter().enumerate() {
            if let Some(other) = tracker_ids.insert(&t.id, &p.file_name) {
                return Err(cx.err(&format!("trackers[{i}].id"), None, format!("duplicate tracker id `{}`; also in {other}", t.id)));
            }
        }
        for (i, a) in p.agents.iter().enumerate() {
            if let Some(other) = agent_names.insert(&a.name, &p.file_name) {
                return Err(cx.err(&format!("agents[{i}].name"), None, format!("duplicate agent name `{}`; also in {other}", a.name)));
            }
        }
    }
    let mut slots: BTreeMap<String, Vec<RenderInstance>> = Slot::ALL.iter().map(|s| (s.as_str().to_string(), Vec::new())).collect();
    for p in &sorted {
        let cx = Ctx { file: &p.file_name, src: &p.source };
        for (slot, list) in &p.slots {
            let out = slots.get_mut(slot.as_str()).expect("every slot is pre-filled");
            for (i, inst) in list.iter().enumerate() {
                if let Some(t) = inst.tracker.as_deref().filter(|t| !tracker_ids.contains_key(t)) {
                    let mut known: Vec<&str> = tracker_ids.keys().copied().collect();
                    known.sort_unstable();
                    return Err(cx.err(&format!("slots.{}[{i}].tracker", slot.as_str()), Some(t), format!("unknown tracker `{t}`; loaded trackers: {}", known.join(", "))));
                }
                out.push(RenderInstance {
                    key: format!("{}/{}/{i}", p.info.name, slot.as_str()),
                    plugin: p.info.name.clone(),
                    component: inst.component,
                    props: inst.props.clone(),
                });
            }
        }
    }
    assert_eq!(slots.len(), Slot::ALL.len());
    Ok(RenderModel {
        plugins: sorted.iter().map(|p| p.info.clone()).collect(),
        slots,
        trackers: sorted.iter().flat_map(|p| p.trackers.iter().map(|t| RenderTracker { plugin: p.info.name.clone(), tracker: t.clone() })).collect(),
        agents: sorted.iter().flat_map(|p| p.agents.iter().map(|a| RenderAgent { plugin: p.info.name.clone(), agent: a.clone() })).collect(),
    })
}
