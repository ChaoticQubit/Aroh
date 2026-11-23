//! Every component kind of `apps/aroh/src/lib/ui/contract.ts` with its props and types.
//! The drift test renders this table to TypeScript and compares it with the contract.

/// A prop type, shaped like the TypeScript type it stands for.
/// `Named` refers to an entry of `NAMED`; `Null` is `T | null`;
/// `Spec` is a nested `ComponentSpec`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ty {
    Str,
    Num,
    Bool,
    Named(&'static str),
    StrLit(&'static [&'static str]),
    NumLit(&'static [i64]),
    Arr(&'static Ty),
    Obj(&'static [Prop]),
    Null(&'static Ty),
    Spec,
}

/// One prop of a kind or object type: its name, whether it may be left
/// out (`name?:` in TypeScript), and its type.
/// Built with `req` and `opt`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prop {
    pub name: &'static str,
    pub optional: bool,
    pub ty: Ty,
}

/// One component kind: the `kind` tag and its props in contract order.
/// Plugin items are checked against it.
/// Listed in `KINDS`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Kind {
    pub name: &'static str,
    pub props: &'static [Prop],
}

/// One named type of the contract, such as `Unit` or `ActionView`.
/// `Ty::Named` refers to it by name.
/// Listed in `NAMED`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NamedType {
    pub name: &'static str,
    pub ty: Ty,
}

const fn req(name: &'static str, ty: Ty) -> Prop {
    Prop { name, optional: false, ty }
}

const fn opt(name: &'static str, ty: Ty) -> Prop {
    Prop { name, optional: true, ty }
}

const UNIT: Ty = Ty::Named("Unit");
const SERIES: Ty = Ty::Named("SeriesSlot");
const LABEL: Prop = req("label", Ty::Str);
const SLOTS: &[i64] = &[1, 2, 3, 4, 5, 6];

/// A tracker series: `Point[]`.
/// Chart `points` take it; the `series` binding gives it.
/// Compared by its TypeScript text.
pub const POINTS: Ty = Ty::Arr(&Ty::Named("Point"));

/// Heatmap days: `{ date: string; level: 0 | 1 | 2 | 3 }[]`.
/// The `streak` binding gives it.
/// Compared by its TypeScript text.
pub const DAYS: Ty = Ty::Arr(&Ty::Obj(&[req("date", Ty::Str), req("level", Ty::NumLit(&[0, 1, 2, 3]))]));

/// Log list entries: `{ at: string; text: string; value?: number; unit?: Unit }[]`.
/// The `entries` binding gives it.
/// Compared by its TypeScript text.
pub const LOG_ENTRIES: Ty = Ty::Arr(&Ty::Obj(&[req("at", Ty::Str), req("text", Ty::Str), opt("value", Ty::Num), opt("unit", UNIT)]));

/// Planned or past actions: `ActionView[]`.
/// The `actions` binding gives it.
/// Compared by its TypeScript text.
pub const ACTIONS: Ty = Ty::Arr(&Ty::Named("ActionView"));

/// The named types of the contract, in the order the drift test checks them.
/// Every `Ty::Named` used in `KINDS` is listed here.
/// Looked up with `named`.
pub const NAMED: &[NamedType] = &[
    NamedType { name: "Unit", ty: Ty::StrLit(&["kg", "lb", "cm", "kcal", "g", "ml", "L", "min", "reps", "steps", "usd", "count"]) },
    NamedType { name: "CategorySlot", ty: Ty::NumLit(SLOTS) },
    NamedType { name: "SeriesSlot", ty: Ty::NumLit(SLOTS) },
    NamedType {
        name: "Ring",
        ty: Ty::Obj(&[LABEL, req("unit", UNIT), req("value", Ty::Num), req("target", Ty::Num), req("series", SERIES)]),
    },
    NamedType { name: "Point", ty: Ty::Obj(&[req("at", Ty::Str), req("value", Ty::Num)]) },
    NamedType {
        name: "ActionView",
        ty: Ty::Obj(&[
            req("id", Ty::Str),
            req("title", Ty::Str),
            req("start", Ty::Str),
            req("end", Ty::Str),
            req("status", Ty::StrLit(&["planned", "done", "missed", "skipped"])),
            req("locked", Ty::Bool),
            req("category", Ty::Named("CategorySlot")),
            req("goalTitle", Ty::Str),
        ]),
    },
    NamedType {
        name: "ChangeView",
        ty: Ty::Obj(&[
            req("op", Ty::StrLit(&["move", "resize", "create", "delete"])),
            req("title", Ty::Str),
            req("before", Ty::Null(&Ty::Str)),
            req("after", Ty::Null(&Ty::Str)),
        ]),
    },
];

/// Every kind of the `ComponentSpec` union, in contract order.
/// A plugin item names one of these in `kind`.
/// The drift test fails when this table and `contract.ts` differ.
pub const KINDS: &[Kind] = &[
    Kind {
        name: "number_input",
        props: &[LABEL, req("unit", Ty::Str), req("value", Ty::Null(&Ty::Num)), req("step", Ty::Num), opt("min", Ty::Num), opt("max", Ty::Num)],
    },
    Kind { name: "rating", props: &[LABEL, req("min", Ty::Num), req("max", Ty::Num), req("value", Ty::Null(&Ty::Num))] },
    Kind { name: "choice", props: &[LABEL, req("options", Ty::Arr(&Ty::Str)), req("multi", Ty::Bool), req("value", Ty::Arr(&Ty::Str))] },
    Kind { name: "note", props: &[LABEL, req("placeholder", Ty::Str), req("value", Ty::Str)] },
    Kind { name: "check_off", props: &[LABEL, req("done", Ty::Bool)] },
    Kind { name: "counter", props: &[LABEL, req("unit", UNIT), req("value", Ty::Num), req("target", Ty::Num)] },
    Kind { name: "timer", props: &[LABEL, req("targetMinutes", Ty::Num)] },
    Kind { name: "capture", props: &[LABEL, req("media", Ty::StrLit(&["photo", "audio", "video"]))] },
    Kind { name: "line_chart", props: &[LABEL, req("unit", UNIT), req("points", POINTS), opt("target", Ty::Num), opt("series", SERIES)] },
    Kind { name: "bar_chart", props: &[LABEL, req("unit", UNIT), req("points", POINTS), opt("target", Ty::Num), opt("series", SERIES)] },
    Kind { name: "progress_ring", props: &[LABEL, req("rings", Ty::Arr(&Ty::Named("Ring")))] },
    Kind { name: "target_vs_actual", props: &[LABEL, req("unit", UNIT), req("target", Ty::Num), req("actual", Ty::Num)] },
    Kind { name: "streak_heatmap", props: &[LABEL, req("days", DAYS), opt("series", SERIES)] },
    Kind { name: "stat_tile", props: &[LABEL, req("unit", UNIT), req("value", Ty::Num), opt("delta", Ty::Num)] },
    Kind { name: "log_list", props: &[LABEL, req("entries", LOG_ENTRIES)] },
    Kind { name: "photo_timeline", props: &[LABEL, req("photos", Ty::Arr(&Ty::Obj(&[req("at", Ty::Str), req("src", Ty::Str)])))] },
    Kind { name: "action_list", props: &[LABEL, req("actions", ACTIONS)] },
    Kind { name: "week_strip", props: &[LABEL, req("actions", ACTIONS), req("startDay", Ty::Str)] },
    Kind {
        name: "milestone_timeline",
        props: &[LABEL, req("milestones", Ty::Arr(&Ty::Obj(&[req("title", Ty::Str), req("due", Ty::Str), req("done", Ty::Bool)])))],
    },
    Kind {
        name: "module_checklist",
        props: &[LABEL, req("modules", Ty::Arr(&Ty::Obj(&[req("title", Ty::Str), req("minutes", Ty::Num), req("done", Ty::Bool)])))],
    },
    Kind { name: "countdown", props: &[LABEL, req("deadline", Ty::Str)] },
    Kind { name: "insight_card", props: &[req("persona", Ty::Str), req("title", Ty::Str), req("body", Ty::Str)] },
    Kind { name: "proposal_card", props: &[req("persona", Ty::Str), req("summary", Ty::Str), req("changes", Ty::Arr(&Ty::Named("ChangeView")))] },
    Kind { name: "question_card", props: &[req("persona", Ty::Str), req("question", Ty::Str), req("input", Ty::Spec)] },
];

/// Props that take only a binding or an empty list: photo sources are URLs
/// and a generated bundle must not carry one.
/// Pairs of kind and prop.
pub const BOUND_ONLY: &[(&str, &str)] = &[("photo_timeline", "photos")];

/// Returns the kind called `name`, if the contract has it.
/// Linear scan over 24 entries.
/// Used by the validator.
pub fn kind(name: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.name == name)
}

/// Returns the named type called `name`.
/// Panics when a `Ty::Named` in this table has no entry, which the drift
/// test rules out.
pub fn named(name: &str) -> &'static Ty {
    &NAMED.iter().find(|n| n.name == name).unwrap_or_else(|| panic!("named type {name} is listed")).ty
}

fn props_ts(props: &[Prop]) -> String {
    props
        .iter()
        .map(|p| format!("{}{}: {}", p.name, if p.optional { "?" } else { "" }, ts(&p.ty)))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Renders `ty` as the TypeScript text the contract writes for it, for
/// example `{ at: string; value: number }[]` or `number | null`.
/// Two types fit each other exactly when their texts are equal.
pub fn ts(ty: &Ty) -> String {
    match ty {
        Ty::Str => "string".into(),
        Ty::Num => "number".into(),
        Ty::Bool => "boolean".into(),
        Ty::Named(n) => (*n).into(),
        Ty::StrLit(values) => values.iter().map(|v| format!("'{v}'")).collect::<Vec<_>>().join(" | "),
        Ty::NumLit(values) => values.iter().map(i64::to_string).collect::<Vec<_>>().join(" | "),
        Ty::Arr(inner) => match inner {
            Ty::StrLit(_) | Ty::NumLit(_) | Ty::Null(_) => format!("({})[]", ts(inner)),
            _ => format!("{}[]", ts(inner)),
        },
        Ty::Obj(props) => format!("{{ {} }}", props_ts(props)),
        Ty::Null(inner) => format!("{} | null", ts(inner)),
        Ty::Spec => "ComponentSpec".into(),
    }
}

/// Renders one kind as its member of the `ComponentSpec` union, for
/// example `{ kind: 'timer'; label: string; targetMinutes: number }`.
/// Compared with `contract.ts` by the drift test.
pub fn kind_ts(kind: &Kind) -> String {
    format!("{{ kind: '{}'; {} }}", kind.name, props_ts(kind.props))
}
