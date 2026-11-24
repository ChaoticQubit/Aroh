use aroh_domain::rows::Plugin;
use aroh_plugins::bind::SOURCES;
use aroh_plugins::contract::KINDS;
use aroh_plugins::versions::{active, enable, history, next_version};
use aroh_plugins::{parse, template, Error, TEMPLATES};
use serde_json::Value;
use std::collections::BTreeSet;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/aroh/src/lib/plugins/fixtures");
const ALL_KINDS: &str = include_str!("../../../apps/aroh/src/lib/plugins/fixtures/all-kinds.toml");

const MINIMAL: &str = r#"
aroh_plugin = 1
name = "water"
title = "Water"

[[trackers]]
name = "Water"

[[trackers.fields]]
key = "ml"
label = "Water"
kind = "number"
unit = "ml"
target = 2000

[[sections]]
title = "Today"

[[sections.items]]
kind = "counter"
label = "Water"
unit = "ml"
value = { bind = "today_total", tracker = "Water", field = "ml" }
target = { bind = "target", tracker = "Water", field = "ml" }
"#;

fn fails(text: &str) -> Error {
    parse("water.toml", text).expect_err("the bundle is invalid")
}

fn edit(from: &str, to: &str) -> String {
    assert!(MINIMAL.contains(from), "{from}");
    MINIMAL.replacen(from, to, 1)
}

fn assert_field(e: &Error, field: &str, reason: &str) {
    assert_eq!(e.file, "water.toml");
    assert_eq!(e.field, field, "{e}");
    assert!(e.reason.contains(reason), "{e}");
}

/// Compares `tree` with the pinned JSON file `name` in the app's fixtures,
/// rewriting the file instead when `AROH_PIN=1`.
/// The app's tests read the same files.
fn pinned(name: &str, tree: &Value) {
    let path = format!("{FIXTURES}/{name}");
    let text = serde_json::to_string_pretty(tree).unwrap() + "\n";
    if std::env::var("AROH_PIN").as_deref() == Ok("1") {
        std::fs::write(&path, &text).unwrap();
    }
    assert_eq!(std::fs::read_to_string(&path).expect("pinned tree exists"), text, "rerun with AROH_PIN=1 to repin {name}");
}

fn kinds_in(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            if let Some(Value::String(k)) = m.get("kind") {
                out.insert(k.clone());
            }
            m.values().for_each(|x| kinds_in(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| kinds_in(x, out)),
        _ => {}
    }
}

fn sources_in(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            if let Some(Value::String(b)) = m.get("bind") {
                out.insert(b.clone());
            }
            m.values().for_each(|x| sources_in(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| sources_in(x, out)),
        _ => {}
    }
}

#[test]
fn a_minimal_bundle_parses_into_a_tree() {
    let b = parse("water.toml", MINIMAL).unwrap();
    assert_eq!((b.name.as_str(), b.title.as_str()), ("water", "Water"));
    assert_eq!(b.trackers[0].targets["ml"], 2000.0);
    assert_eq!(b.tree["format"], 1);
    assert_eq!(b.tree["trackers"][0]["fields"][0]["target"], 2000.0);
    let item = &b.tree["sections"][0]["items"][0];
    assert_eq!(item["kind"], "counter");
    assert_eq!(item["value"]["bind"], "today_total");
    assert_eq!(item["target"]["field"], "ml");
}

#[test]
fn errors_name_the_file_and_the_field() {
    assert_field(&fails(&edit("title = \"Water\"", "title = \"Water\"\ncolour = 1")), "colour", "unknown key");
    assert_field(&fails(&edit("aroh_plugin = 1", "aroh_plugin = 2")), "aroh_plugin", "must be 1");
    assert_field(&fails(&edit("name = \"water\"", "name = \"Water!\"")), "name", "slug");
    assert_field(&fails(&edit("kind = \"counter\"", "kind = \"gauge\"")), "sections[0].items[0].kind", "unknown kind `gauge`");
    assert_field(&fails(&edit("unit = \"ml\"\nvalue", "unit = \"ml\"\nshade = 2\nvalue")), "sections[0].items[0].shade", "unknown key");
    assert_field(&fails(&edit("unit = \"ml\"\nvalue", "unit = \"litre\"\nvalue")), "sections[0].items[0].unit", "one of kg");
    assert_field(&fails(&edit("label = \"Water\"\nunit = \"ml\"\nvalue", "label = 3\nunit = \"ml\"\nvalue")), "sections[0].items[0].label", "a string");
    assert_field(&fails(&edit("tracker = \"Water\", field = \"ml\" }\ntarget", "tracker = \"Wter\", field = \"ml\" }\ntarget")), "sections[0].items[0].value.tracker", "no tracker named `Wter`");
    assert_field(&fails(&edit("field = \"ml\" }\ntarget", "field = \"cups\" }\ntarget")), "sections[0].items[0].value.field", "no field `cups`");
    assert_field(&fails(&edit("bind = \"today_total\"", "bind = \"sum\"")), "sections[0].items[0].value.bind", "unknown source `sum`");
    assert_field(&fails(&edit("bind = \"today_total\"", "bind = \"series\"")), "sections[0].items[0].value", "gives Point[], this prop needs number");
    assert_field(&fails(&edit("field = \"ml\" }\ntarget", "field = \"ml\", when = \"next\" }\ntarget")), "sections[0].items[0].value.when", "unknown key");
    assert_field(&fails(&edit("target = 2000\n", "")), "sections[0].items[0].target.field", "declares no target");
    assert_field(&fails(&edit("kind = \"number\"\nunit = \"ml\"\ntarget = 2000", "kind = \"text\"")), "sections[0].items[0].value.field", "not a number field");
    assert_field(&fails(&edit("target = { bind", "colour = { bind")), "sections[0].items[0].colour", "unknown key");
    assert_field(&fails(&edit("title = \"Today\"", "title = \"\"")), "sections[0].title", "length 0");
    assert_field(&fails(&edit("unit = \"ml\"\ntarget = 2000", "unit = \"ml\"\ntarget = \"lots\"")), "trackers[0].fields[0].target", "finite number");
    assert_field(&fails(&edit("kind = \"number\"", "kind = \"colour\"")), "trackers[0].fields[0]", "unknown variant");
    assert_field(&fails("aroh_plugin = 1\nname = \"water\"\ntitle = [1"), "(toml)", "");
    assert_field(&fails(&edit("[[sections]]\ntitle = \"Today\"", "")), "sections", "expected a list");
}

#[test]
fn fields_declare_a_health_source_that_fits_their_kind() {
    use aroh_domain::rows::HealthMetric;
    let b = parse("fitness.toml", template("fitness").unwrap()).unwrap();
    let sources: Vec<(&str, &str, Option<HealthMetric>)> =
        b.trackers.iter().flat_map(|t| t.fields.iter().map(move |f| (t.name.as_str(), f.key.as_str(), f.health))).collect();
    assert_eq!(
        sources,
        [
            ("Weight", "weight", Some(HealthMetric::BodyMass)),
            ("Protein", "protein", None),
            ("Workouts", "minutes", Some(HealthMetric::WorkoutMinutes)),
            ("Workouts", "type", Some(HealthMetric::WorkoutType)),
            ("Steps", "steps", Some(HealthMetric::Steps)),
        ]
    );
    assert_field(&fails(&edit("unit = \"ml\"\ntarget = 2000", "unit = \"ml\"\nhealth = \"workout_type\"\ntarget = 2000")), "trackers[0]", "does not fit");
    assert_field(&fails(&edit("unit = \"ml\"\ntarget = 2000", "unit = \"ml\"\nhealth = \"heart_rate\"\ntarget = 2000")), "trackers[0].fields[0]", "unknown variant");
}

#[test]
fn literal_urls_are_refused_anywhere_in_the_bundle() {
    assert_field(&fails(&edit("title = \"Water\"", "title = \"See https://x.example\"")), "(bundle)", "line 4: literal URL");
    assert_field(&fails(&edit("label = \"Water\"\nkind", "label = \"www.water.example\"\nkind")), "(bundle)", "literal URL");
    assert_field(&fails(&edit("label = \"Water\"\nunit = \"ml\"\nvalue", "label = \"JavaScript:alert(1)\"\nunit = \"ml\"\nvalue")), "(bundle)", "literal URL");
    let persona = format!("{MINIMAL}\n[[personas]]\nname = \"coach\"\ntoml = \"\"\"\nname = \"coach\"\ninstructions = \"Read ftp://files.example\"\n\"\"\"\n");
    assert_field(&fails(&persona), "(bundle)", "literal URL");
}

#[test]
fn trackers_sections_and_nesting_are_checked() {
    let twice = format!("{MINIMAL}\n[[trackers]]\nname = \"Water\"\n[[trackers.fields]]\nkey = \"ml\"\nlabel = \"Water\"\nkind = \"number\"\n");
    assert_field(&fails(&twice), "trackers[1].name", "duplicate tracker name");
    let section = "\n[[sections]]\ntitle = \"Today\"\n[[sections.items]]\nkind = \"check_off\"\nlabel = \"Done\"\ndone = false\n";
    assert_field(&fails(&format!("{MINIMAL}{section}")), "sections[1].title", "duplicate section title");
    let bad_cadence = edit("target = 2000\n", "target = 2000\n[trackers.cadence]\nfreq = \"weekly\"\nat = \"07:00\"\n");
    assert_field(&fails(&bad_cadence), "trackers[0]", "weekdays");
    let nested = format!(
        "{MINIMAL}\n[[sections.items]]\nkind = \"question_card\"\npersona = \"A\"\nquestion = \"Q\"\n[sections.items.input]\nkind = \"question_card\"\npersona = \"B\"\nquestion = \"R\"\n"
    );
    assert_field(&fails(&nested), "sections[0].items[1].input.kind", "cannot ask through");
    let photo = format!("{MINIMAL}\n[[sections.items]]\nkind = \"photo_timeline\"\nlabel = \"P\"\n[[sections.items.photos]]\nat = \"2026-11-21\"\nsrc = \"https://evil.example/x.png\"\n");
    assert_field(&fails(&photo), "sections[0].items[1].photos", "only a binding");
    let ring = format!("{MINIMAL}\n[[sections.items]]\nkind = \"progress_ring\"\nlabel = \"R\"\n[[sections.items.rings]]\nlabel = \"A\"\nunit = \"count\"\nvalue = 1\ntarget = 2\nseries = 7\n");
    assert_field(&fails(&ring), "sections[0].items[1].rings[0].series", "one of [1, 2, 3, 4, 5, 6]");
    let missing = edit("label = \"Water\"\nunit = \"ml\"\nvalue", "unit = \"ml\"\nvalue");
    assert_field(&fails(&missing), "sections[0].items[0].label", "missing");
}

#[test]
fn limits_bound_a_generated_bundle() {
    let big = format!("{MINIMAL}\ndescription_pad = \"{}\"", "x".repeat(70_000));
    assert_field(&fails(&big), "(bundle)", "at most 65536");
    let item = "\n[[sections.items]]\nkind = \"check_off\"\nlabel = \"Done\"\ndone = false\n";
    assert_field(&fails(&format!("{MINIMAL}{}", item.repeat(24))), "sections[0].items", "25 entries, expected 1 to 24");
    let options: Vec<String> = (0..51).map(|i| format!("\"o{i}\"")).collect();
    let choice = format!("{MINIMAL}\n[[sections.items]]\nkind = \"choice\"\nlabel = \"C\"\noptions = [{}]\nmulti = false\nvalue = []\n", options.join(", "));
    assert_field(&fails(&choice), "sections[0].items[1].options", "at most 50");
    let tools = parse("water.toml", &format!("tools = [\"web_search\", \"mail\"]\n{MINIMAL}")).unwrap();
    assert_field(&tools.check_tools("water.toml", &["web_search"]).unwrap_err(), "tools[1]", "unknown tool `mail`");
    assert!(tools.check_tools("water.toml", &["web_search", "mail"]).is_ok());
}

#[test]
fn the_all_kinds_fixture_uses_every_kind_and_every_source() {
    let b = parse("all-kinds.toml", ALL_KINDS).unwrap();
    let mut kinds = BTreeSet::new();
    kinds_in(&b.tree["sections"], &mut kinds);
    let all: BTreeSet<String> = KINDS.iter().map(|k| k.name.to_string()).collect();
    assert_eq!(kinds, all);
    let mut sources = BTreeSet::new();
    sources_in(&b.tree["sections"], &mut sources);
    let every: BTreeSet<String> = SOURCES.iter().map(|s| s.name.to_string()).collect();
    assert_eq!(sources, every);
    assert_eq!(b.personas[0].name, "coach");
    assert_eq!(b.tree["sections"][0]["items"][1]["value"], Value::Null, "a left-out nullable prop is null");
    pinned("all-kinds.tree.json", &b.tree);
}

#[test]
fn every_template_validates_and_is_pinned() {
    assert_eq!(TEMPLATES.iter().map(|(n, _)| *n).collect::<Vec<_>>(), ["fitness", "music", "learning", "finance", "habits"]);
    for (name, text) in TEMPLATES {
        let b = parse(&format!("{name}.toml"), text).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(&b.name, name);
        pinned(&format!("{name}.tree.json"), &b.tree);
    }
    let fitness = parse("fitness.toml", template("fitness").unwrap()).unwrap();
    assert_eq!(fitness.trackers.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Weight", "Protein", "Workouts", "Steps"]);
    assert!(template("chess").is_none());
}

fn row(id: &str, goal: Option<&str>, version: i64, enabled: bool) -> Plugin {
    Plugin {
        id: id.into(),
        user_id: "u".into(),
        goal_id: goal.map(str::to_string),
        name: "fitness".into(),
        version,
        bundle_toml: String::new(),
        enabled,
        updated_at: String::new(),
        deleted: false,
    }
}

#[test]
fn versions_pick_the_highest_enabled_row() {
    let mut rows = vec![row("a", Some("g"), 1, true), row("b", Some("g"), 2, true), row("c", Some("g"), 3, false), row("d", Some("h"), 9, true)];
    assert_eq!(active(&rows, "g").unwrap().id, "b", "a half-synced flip still picks one");
    assert_eq!(history(&rows, "g").iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["c", "b", "a"]);
    assert_eq!(next_version(&rows, Some("g"), "fitness"), 4);
    assert_eq!(next_version(&rows, Some("g"), "piano"), 1);
    assert_eq!(next_version(&rows, None, "fitness"), 1);
    rows[1].deleted = true;
    assert_eq!(active(&rows, "g").unwrap().id, "a");
    assert_eq!(next_version(&rows, Some("g"), "fitness"), 4, "a deleted version number is never reused");
    assert!(active(&rows, "nope").is_none());
}

#[test]
fn enable_turns_one_version_on_and_the_rest_off() {
    let rows = vec![row("a", Some("g"), 1, true), row("b", Some("g"), 2, false), row("d", Some("h"), 1, true)];
    let writes = enable(&rows, "g", Some("b")).unwrap();
    assert_eq!(writes.iter().map(|p| (p.id.as_str(), p.enabled)).collect::<Vec<_>>(), [("b", true), ("a", false)], "target first");
    assert_eq!(enable(&rows, "g", Some("a")).unwrap().len(), 0, "already the only enabled one");
    let off = enable(&rows, "g", None).unwrap();
    assert_eq!(off.iter().map(|p| (p.id.as_str(), p.enabled)).collect::<Vec<_>>(), [("a", false)]);
    assert!(enable(&rows, "g", Some("d")).is_none(), "another goal's row");
    assert!(enable(&rows, "g", Some("zz")).is_none());
}

#[test]
fn missing_trackers_are_derived_once_per_goal_and_name() {
    let user = "0190f5c4-0000-7000-8000-000000000001";
    let goal = "0190f5c4-0000-7000-8000-000000000002";
    let b = parse("fitness.toml", template("fitness").unwrap()).unwrap();
    let all = aroh_plugins::missing_trackers(&b, &[], user, goal);
    assert_eq!(all.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Weight", "Protein", "Workouts", "Steps"]);
    assert_eq!(all[0].id, aroh_plugins::missing_trackers(&b, &[], user, goal)[0].id, "derived, so every engine agrees");
    assert!(all.iter().all(|t| t.goal_id == goal && t.user_id == user && !t.fields.is_empty()));
    let left = aroh_plugins::missing_trackers(&b, &all[..1], user, goal);
    assert_eq!(left.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Protein", "Workouts", "Steps"]);
    let mut gone = all[0].clone();
    gone.deleted = true;
    assert_eq!(aroh_plugins::missing_trackers(&b, &[gone], user, goal).len(), 4);
}

#[test]
fn the_reference_lists_every_kind_and_source() {
    let text = aroh_plugins::reference();
    for k in KINDS {
        assert!(text.contains(&format!("kind: '{}'", k.name)), "{}", k.name);
    }
    for s in SOURCES {
        assert!(text.contains(&format!("\n{}: keys bind", s.name)), "{}", s.name);
    }
}
