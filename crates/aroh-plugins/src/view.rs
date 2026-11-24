//! View validation: sections of items, each item a contract kind whose props are
//! literals of the contract type or bindings. Emits the tree as JSON.

use crate::contract::{kind, named, Prop, Ty, BOUND_ONLY, KINDS};
use crate::{bind, Cx, Error, BundleTracker, MAX_ITEMS, MAX_LIST, MAX_SECTIONS, MAX_TEXT};
use serde_json::{Map, Value as Json};
use std::collections::BTreeSet;
use toml::{Table, Value};

fn is_binding(v: &Value) -> bool {
    v.as_table().is_some_and(|t| t.contains_key("bind"))
}

fn props(cx: &Cx, path: &str, t: &Table, props: &[Prop], trackers: &[BundleTracker], out: &mut Map<String, Json>) -> Result<(), Error> {
    for p in props {
        let here = Cx::at(path, p.name);
        match t.get(p.name) {
            Some(v) => {
                out.insert(p.name.into(), value(cx, &here, v, &p.ty, trackers)?);
            }
            None if p.optional => {}
            None if matches!(p.ty, Ty::Null(_)) => {
                out.insert(p.name.into(), Json::Null);
            }
            None => return Err(cx.err(here, "missing")),
        }
    }
    Ok(())
}

fn value(cx: &Cx, path: &str, v: &Value, ty: &Ty, trackers: &[BundleTracker]) -> Result<Json, Error> {
    if let Some(t) = v.as_table().filter(|_| is_binding(v)) {
        return bind::check(cx, path, t, ty, trackers);
    }
    let wrong = |what: &str| cx.err(path, format!("expected {what}"));
    match ty {
        Ty::Str => {
            let s = v.as_str().ok_or_else(|| wrong("a string"))?;
            cx.check_text(path, s, 0, MAX_TEXT)?;
            Ok(Json::String(s.into()))
        }
        Ty::Num => {
            let n = v.as_float().or_else(|| v.as_integer().map(|i| i as f64)).filter(|n| n.is_finite());
            let n = n.ok_or_else(|| wrong("a finite number"))?;
            Ok(v.as_integer().map_or_else(|| serde_json::json!(n), |i| serde_json::json!(i)))
        }
        Ty::Bool => v.as_bool().map(Json::Bool).ok_or_else(|| wrong("true or false")),
        Ty::Named(n) => value(cx, path, v, named(n), trackers),
        Ty::StrLit(values) => match v.as_str() {
            Some(s) if values.contains(&s) => Ok(Json::String(s.into())),
            _ => Err(wrong(&format!("one of {}", values.join(", ")))),
        },
        Ty::NumLit(values) => match v.as_integer() {
            Some(i) if values.contains(&i) => Ok(serde_json::json!(i)),
            _ => Err(wrong(&format!("one of {values:?}"))),
        },
        Ty::Arr(inner) => {
            let items = v.as_array().ok_or_else(|| wrong("a list"))?;
            if items.len() > MAX_LIST {
                return Err(cx.err(path, format!("at most {MAX_LIST} entries")));
            }
            let out = items.iter().enumerate().map(|(i, x)| value(cx, &Cx::index(path, i), x, inner, trackers));
            Ok(Json::Array(out.collect::<Result<_, _>>()?))
        }
        Ty::Obj(fields) => {
            let t = v.as_table().ok_or_else(|| wrong("a table"))?;
            let names: Vec<&str> = fields.iter().map(|p| p.name).collect();
            cx.closed(t, path, &names)?;
            let mut out = Map::new();
            props(cx, path, t, fields, trackers, &mut out)?;
            Ok(Json::Object(out))
        }
        Ty::Null(inner) => value(cx, path, v, inner, trackers),
        Ty::Spec => item(cx, path, v, trackers, true),
    }
}

/// Checks one view item at `path`: a table naming a contract `kind` with
/// exactly that kind's props, each a literal of its type or a fitting
/// binding. A `nested` item (a question card input) cannot be a question card.
pub(crate) fn item(cx: &Cx, path: &str, v: &Value, trackers: &[BundleTracker], nested: bool) -> Result<Json, Error> {
    let t = v.as_table().ok_or_else(|| cx.err(path, "expected a table"))?;
    let name = cx.text(t, path, "kind", 1, 40)?;
    let Some(k) = kind(&name) else {
        let known: Vec<&str> = KINDS.iter().map(|k| k.name).collect();
        return Err(cx.err(Cx::at(path, "kind"), format!("unknown kind `{name}`, expected one of {}", known.join(", "))));
    };
    if nested && k.name == "question_card" {
        return Err(cx.err(Cx::at(path, "kind"), "a question card cannot ask through another question card"));
    }
    let mut names = vec!["kind"];
    names.extend(k.props.iter().map(|p| p.name));
    cx.closed(t, path, &names)?;
    for (kind_name, prop) in BOUND_ONLY {
        let literal = t.get(*prop).is_some_and(|v| !is_binding(v) && v.as_array().is_none_or(|a| !a.is_empty()));
        if *kind_name == k.name && literal {
            return Err(cx.err(Cx::at(path, prop), "takes only a binding or an empty list, never a literal"));
        }
    }
    let mut out = Map::new();
    out.insert("kind".into(), Json::String(k.name.into()));
    props(cx, path, t, k.props, trackers, &mut out)?;
    Ok(Json::Object(out))
}

/// Checks the `sections` list: 1 to 12 sections with unique titles, each
/// with 1 to 24 items. Returns the sections as JSON, `{title, items}` each,
/// with bindings kept as they are.
pub(crate) fn sections(cx: &Cx, v: Option<&Value>, trackers: &[BundleTracker]) -> Result<Json, Error> {
    let list = cx.list(v, "sections", 1, MAX_SECTIONS)?;
    let mut titles = BTreeSet::new();
    let mut out = Vec::with_capacity(list.len());
    for (i, s) in list.iter().enumerate() {
        let path = Cx::index("sections", i);
        let t = s.as_table().ok_or_else(|| cx.err(&path, "expected a table"))?;
        cx.closed(t, &path, &["title", "items"])?;
        let title = cx.text(t, &path, "title", 1, 80)?;
        if !titles.insert(title.clone()) {
            return Err(cx.err(Cx::at(&path, "title"), format!("duplicate section title `{title}`")));
        }
        let items_path = Cx::at(&path, "items");
        let items = cx.list(t.get("items"), &items_path, 1, MAX_ITEMS)?;
        let items = items.iter().enumerate().map(|(j, x)| item(cx, &Cx::index(&items_path, j), x, trackers, false));
        out.push(serde_json::json!({"title": title, "items": items.collect::<Result<Vec<_>, _>>()?}));
    }
    Ok(Json::Array(out))
}
