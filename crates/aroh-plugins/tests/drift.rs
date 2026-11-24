use aroh_plugins::bind::SOURCES;
use aroh_plugins::contract::{kind_ts, ts, KINDS, NAMED};

const CONTRACT: &str = include_str!("../../../apps/aroh/src/lib/ui/contract.ts");
const RESOLVER: &str = include_str!("../../../apps/aroh/src/lib/plugins/resolve.ts");

/// Collapses whitespace and drops the trailing `;` before a closing brace,
/// so one-line and multi-line TypeScript object types compare equal.
/// Returns the normalized text.
fn normalize(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.replace("; }", " }").trim().to_string()
}

/// The right-hand side of `export type <name> = ...;` in `src`, up to the
/// `;` outside every brace. Returns `None` when the type is missing.
/// Normalized.
fn exported(src: &str, name: &str) -> Option<String> {
    let head = format!("export type {name} =");
    let start = src.find(&head)? + head.len();
    let mut depth = 0i32;
    for (i, c) in src[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ';' if depth == 0 => return Some(normalize(&src[start..start + i])),
            _ => {}
        }
    }
    None
}

/// Splits a union type text at every `|` outside braces.
/// Empty parts (a leading `|`) are dropped.
/// Returns the members, normalized.
fn members(union: &str) -> Vec<String> {
    let (mut out, mut depth, mut part) = (Vec::new(), 0i32, String::new());
    for c in union.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if c == '|' && depth == 0 {
            out.push(normalize(&part));
            part.clear();
        } else {
            part.push(c);
        }
    }
    out.push(normalize(&part));
    out.into_iter().filter(|p| !p.is_empty()).collect()
}

#[test]
fn every_component_kind_matches_the_contract() {
    let union = exported(CONTRACT, "ComponentSpec").expect("contract.ts exports ComponentSpec");
    let theirs = members(&union);
    let ours: Vec<String> = KINDS.iter().map(kind_ts).collect();
    assert_eq!(theirs.len(), 24, "the contract has 24 kinds");
    assert_eq!(ours, theirs);
}

#[test]
fn every_named_type_matches_the_contract() {
    for named in NAMED {
        let theirs = exported(CONTRACT, named.name).unwrap_or_else(|| panic!("contract.ts exports {}", named.name));
        assert_eq!(ts(&named.ty), theirs, "{}", named.name);
    }
}

#[test]
fn the_resolver_knows_every_binding_source() {
    let start = RESOLVER.find("export const SOURCES = [").expect("resolve.ts exports SOURCES");
    let list = &RESOLVER[start..];
    let list = &list[list.find('[').unwrap() + 1..list.find(']').unwrap()];
    let theirs: Vec<&str> = list.split(',').map(|s| s.trim().trim_matches('\'')).filter(|s| !s.is_empty()).collect();
    let ours: Vec<&str> = SOURCES.iter().map(|s| s.name).collect();
    assert_eq!(ours, theirs);
}
