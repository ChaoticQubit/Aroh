use aroh_plugins::{parse_agent, parse_plugin, resolve, Effort, Plugin, PluginError};
use serde_json::json;

const WATER: &str = include_str!("../examples-yaml/water-tracker.yaml");
const COURSE: &str = include_str!("../examples-yaml/course-progress.yaml");

fn plugin(file: &str, yaml: &str) -> Plugin {
    parse_plugin(file, yaml).unwrap_or_else(|e| panic!("{e}"))
}

fn base(body: &str) -> String {
    format!("aroh_plugin: 1\nname: t-plugin\nversion: 0.1.0\ntitle: T\n{body}")
}

fn err(yaml: &str) -> PluginError {
    match parse_plugin("t.yaml", yaml) {
        Ok(_) => panic!("expected an error for:\n{yaml}"),
        Err(e) => e,
    }
}

fn assert_err(e: &PluginError, needle: &str, line: Option<usize>) {
    assert!(e.message.contains(needle), "message `{}` lacks `{needle}`", e);
    if line.is_some() {
        assert_eq!(e.line, line, "wrong line in `{e}`");
    }
}

#[test]
fn water_example_resolves_to_spec_shape() {
    let model = resolve(&[plugin("water-tracker.yaml", WATER)]).unwrap();
    let prompt = "I have had {{today_total}} ml of water today. My target is {{daily_target}} ml.\nIf I am behind for the time of day, write one short, friendly nudge. Otherwise reply OK.\n";
    let expected = json!({
        "plugins": [{ "name": "water-tracker", "version": "0.1.0", "title": "Water tracker" }],
        "slots": {
            "today.top": [],
            "goal.detail": [],
            "today.sidebar": [
                { "key": "water-tracker/today.sidebar/0", "plugin": "water-tracker", "component": "stat-tile",
                  "props": { "label": "Water today", "value": { "query": "water.today_total" }, "unit": "ml" } },
                { "key": "water-tracker/today.sidebar/1", "plugin": "water-tracker", "component": "quick-chips",
                  "props": { "tracker": "water", "label": "Add water" } }
            ],
            "log.trackers": [
                { "key": "water-tracker/log.trackers/0", "plugin": "water-tracker", "component": "progress-bar",
                  "props": { "label": "Water vs target", "value": { "query": "water.today_total" }, "max": 2500 } },
                { "key": "water-tracker/log.trackers/1", "plugin": "water-tracker", "component": "quick-chips",
                  "props": { "tracker": "water" } }
            ]
        },
        "trackers": [{ "plugin": "water-tracker", "id": "water", "kind": "water", "unit": "ml",
                       "chips": [250, 500, 750], "daily_target": 2500 }],
        "agents": [{ "plugin": "water-tracker", "name": "hydration-coach",
                     "purpose": "Nudge me when I am behind on water.", "tools": ["read_trackers", "notify"],
                     "variables": ["today_total", "daily_target"], "untrusted": [], "effort": "medium",
                     "output": null, "system": null, "prompt": prompt }]
    });
    assert_eq!(serde_json::to_value(&model).unwrap(), expected);
}

#[test]
fn both_examples_resolve_sorted_with_defaults() {
    let early = base("order: 10\nslots:\n  today.top:\n    - component: text-note\n      text: first\n");
    let model = resolve(&[
        plugin("water-tracker.yaml", WATER),
        plugin("course-progress.yaml", COURSE),
        plugin("t.yaml", &early),
    ])
    .unwrap();
    let v = serde_json::to_value(&model).unwrap();
    let names: Vec<_> = v["plugins"].as_array().unwrap().iter().map(|p| p["name"].clone()).collect();
    assert_eq!(names, [json!("t-plugin"), json!("water-tracker"), json!("course-progress")]);
    let top = v["slots"]["today.top"].as_array().unwrap();
    assert_eq!(top[0]["plugin"], "t-plugin");
    assert_eq!(top[1]["props"], json!({ "label": "Today's sessions", "items": { "query": "today.actions" }, "limit": 5 }));
    assert_eq!(v["slots"]["goal.detail"].as_array().unwrap().len(), 3);
    assert_eq!(v["agents"][1]["untrusted"], json!(["course_url"]));
}

#[test]
fn action_list_limit_defaults_to_five() {
    let y = base("slots:\n  today.top:\n    - component: action-list\n      label: L\n      items: today.actions\n");
    let v = serde_json::to_value(resolve(&[plugin("t.yaml", &y)]).unwrap()).unwrap();
    assert_eq!(v["slots"]["today.top"][0]["props"]["limit"], 5);
}

#[test]
fn yaml_syntax_error() {
    let e = err("aroh_plugin: 1\nname: [oops\n");
    assert!(e.line.is_some(), "{e}");
}

#[test]
fn unknown_key() {
    let e = err(&base("colour: red\n"));
    assert_err(&e, "colour", Some(5));
    assert!(e.to_string().starts_with("t.yaml:5: "), "{e}");
}

#[test]
fn unknown_prop_for_component() {
    let e = err(&base("slots:\n  today.top:\n    - component: text-note\n      text: hi\n      items: today.actions\n"));
    assert_err(&e, "items", Some(9));
}

#[test]
fn missing_key() {
    assert_err(&err("aroh_plugin: 1\nname: t-plugin\nversion: 0.1.0\n"), "title", None);
    let e = err(&base("slots:\n  today.top:\n    - component: stat-tile\n      label: Lonely\n"));
    assert_err(&e, "value", None);
    assert!(e.path.starts_with("slots.today.top[0]"), "{e}");
}

#[test]
fn wrong_type() {
    assert_err(&err(&base("order: high\n")), "invalid type", Some(5));
    assert_err(&err(&base("slots:\n  today.top:\n    - component: progress-bar\n      label: P\n      value: water.today_total\n      max: [1]\n")), "max", Some(10));
}

#[test]
fn unknown_slot() {
    let e = err(&base("slots:\n  today.bottom:\n    - component: text-note\n      text: hi\n"));
    assert_err(&e, "today.bottom", Some(6));
}

#[test]
fn unknown_component() {
    let e = err(&base("slots:\n  today.top:\n    - component: pie-chart\n"));
    assert_err(&e, "pie-chart", Some(7));
    assert!(e.path.starts_with("slots.today.top[0]"), "{e}");
}

#[test]
fn unknown_query() {
    let e = err(&base("slots:\n  today.top:\n    - component: stat-tile\n      label: S\n      value: water.yesterday\n"));
    assert_err(&e, "water.yesterday", Some(9));
    assert_eq!(e.path, "slots.today.top[0].value");
}

#[test]
fn query_type_mismatch() {
    let e = err(&base("slots:\n  today.top:\n    - component: stat-tile\n      label: S\n      value: today.actions\n"));
    assert_err(&e, "today.actions", Some(9));
    let e = err(&base("slots:\n  goal.detail:\n    - component: checklist\n      label: C\n      items: goal.progress\n"));
    assert_err(&e, "goal.progress", Some(9));
}

#[test]
fn goal_query_outside_goal_detail() {
    let e = err(&base("slots:\n  today.sidebar:\n    - component: stat-tile\n      label: S\n      value: goal.actions_left\n"));
    assert_err(&e, "goal.detail", Some(9));
}

#[test]
fn unknown_tracker_in_quick_chips() {
    let y = base("slots:\n  today.top:\n    - component: quick-chips\n      tracker: nope_tracker\n");
    let e = resolve(&[plugin("t.yaml", &y)]).unwrap_err();
    assert_err(&e, "nope_tracker", Some(8));
    let cross = base("slots:\n  today.top:\n    - component: quick-chips\n      tracker: water\n");
    resolve(&[plugin("t.yaml", &cross), plugin("water-tracker.yaml", WATER)]).unwrap();
}

#[test]
fn unknown_tool() {
    let e = err(&base("agents:\n  - name: a-gent\n    purpose: P\n    tools: [read_goals, hack_disk]\n    prompt: hi\n"));
    assert_err(&e, "hack_disk", Some(8));
}

#[test]
fn undeclared_prompt_variable() {
    let e = err(&base("agents:\n  - name: a-gent\n    purpose: P\n    tools: []\n    prompt: hi {{secret_thing}}\n"));
    assert_err(&e, "secret_thing", Some(9));
}

#[test]
fn unused_prompt_variable() {
    let e = err(&base("agents:\n  - name: a-gent\n    purpose: P\n    tools: []\n    variables: [used_var, extra_var]\n    prompt: hi {{used_var}}\n"));
    assert_err(&e, "extra_var", Some(9));
}

#[test]
fn bad_name_or_version() {
    let e = err("aroh_plugin: 1\nname: Bad_Name\nversion: 0.1.0\ntitle: T\n");
    assert_err(&e, "Bad_Name", Some(2));
    let e = err("aroh_plugin: 1\nname: t-plugin\nversion: '1.0'\ntitle: T\n");
    assert_err(&e, "1.0", Some(3));
}

#[test]
fn file_size_limit() {
    let big = base(&format!("description: {}\n", "x".repeat(64 * 1024)));
    assert_err(&err(&big), "64 KiB", None);
}

#[test]
fn component_per_slot_limit() {
    let note = "    - component: text-note\n      text: hi\n";
    let e = err(&base(&format!("slots:\n  today.top:\n{}", note.repeat(9))));
    assert_err(&e, "at most 8", None);
}

#[test]
fn duplicates_across_plugins() {
    let e = resolve(&[plugin("a.yaml", WATER), plugin("b.yaml", WATER)]).unwrap_err();
    assert_err(&e, "duplicate plugin name `water-tracker`", Some(2));
    let tracker = base("trackers:\n  - id: water\n    kind: water\n    unit: ml\n");
    let e = resolve(&[plugin("w.yaml", WATER), plugin("t.yaml", &tracker)]).unwrap_err();
    assert_err(&e, "duplicate tracker id `water`", None);
    let agent = base("agents:\n  - name: hydration-coach\n    purpose: P\n    tools: []\n    prompt: hi\n");
    let e = resolve(&[plugin("w.yaml", WATER), plugin("t.yaml", &agent)]).unwrap_err();
    assert_err(&e, "duplicate agent name `hydration-coach`", None);
}

#[test]
fn standalone_agent() {
    let a = parse_agent("name: planner\npurpose: Plan.\ntools: [read_goals]\nvariables: [page]\nuntrusted: [page]\nsystem: Data {{page}}\nprompt: Go.\n").unwrap();
    assert_eq!(a.effort, Effort::Medium);
    assert_eq!(a.untrusted, ["page"]);
    let e = parse_agent("name: planner\npurpose: Plan.\ntools: []\nvariables: [page]\nuntrusted: [other_var]\nprompt: '{{page}}'\n").unwrap_err();
    assert_err(&e, "other_var", Some(5));
    assert!(parse_agent("name: planner\npurpose: Plan.\ntools: []\nprompt: Go.\nmodel: opus\n").is_err());
}
