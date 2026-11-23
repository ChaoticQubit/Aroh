//! Row structs of every synced table, their registry entries and their validation.

use crate::schema::{Column, Kind, TableDef};
use crate::{hlc, invalid, Result};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc, Weekday};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// A row of a synced table: the sync columns every table shares plus validation.
/// `TABLE` is the row's registry entry, so generic code (repositories, sync)
/// can find its table name and columns from the type alone.
pub trait Record: Serialize + DeserializeOwned + Clone {
    const TABLE: &'static TableDef;
    fn id(&self) -> &str;
    fn user_id(&self) -> &str;
    fn updated_at(&self) -> &str;
    fn set_updated_at(&mut self, stamp: String);
    fn deleted(&self) -> bool;
    fn set_deleted(&mut self, deleted: bool);
    fn validate(&self) -> Result<()>;
}

/// Deserializes a wire row into `R` and validates it.
/// Input: a JSON object holding exactly the registry columns of `R`.
/// Output: `Ok` when the row is well formed, else `Invalid` naming the table.
pub fn check_wire<R: Record>(row: &Value) -> Result<()> {
    let parsed: R = serde_json::from_value(row.clone()).map_err(|e| invalid(R::TABLE.name, e.to_string()))?;
    parsed.validate()
}

macro_rules! synced {
    ($(#[$doc:meta])* $name:ident, $table:ident, $sql:literal { $($field:ident : $ty:ty => $kind:ident),* $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            pub id: String,
            pub user_id: String,
            $(pub $field: $ty,)*
            pub updated_at: String,
            pub deleted: bool,
        }

        #[doc = concat!("Registry entry of the `", $sql, "` table.")]
        pub const $table: TableDef = TableDef {
            name: $sql,
            columns: &[
                Column { name: "id", kind: Kind::Text },
                Column { name: "user_id", kind: Kind::Text },
                $(Column { name: stringify!($field), kind: Kind::$kind },)*
                Column { name: "updated_at", kind: Kind::Text },
                Column { name: "deleted", kind: Kind::Bool },
            ],
            check: check_wire::<$name>,
        };

        impl Record for $name {
            const TABLE: &'static TableDef = &$table;

            fn id(&self) -> &str {
                &self.id
            }

            fn user_id(&self) -> &str {
                &self.user_id
            }

            fn updated_at(&self) -> &str {
                &self.updated_at
            }

            fn set_updated_at(&mut self, stamp: String) {
                self.updated_at = stamp;
            }

            fn deleted(&self) -> bool {
                self.deleted
            }

            fn set_deleted(&mut self, deleted: bool) {
                self.deleted = deleted;
            }

            fn validate(&self) -> Result<()> {
                check_uuid(concat!($sql, ".id"), &self.id)?;
                check_uuid(concat!($sql, ".user_id"), &self.user_id)?;
                hlc::parse(&self.updated_at)?;
                self.check_fields()
            }
        }
    };
}

/// Goal lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Draft,
    Active,
    Paused,
    Done,
    Archived,
}

/// How freely agents may change a goal's plan (Q19).
/// Ask always proposes, Balanced auto-applies small changes with undo,
/// Auto applies every change with undo and a daily digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Autonomy {
    Ask,
    Balanced,
    Auto,
}

/// Where a profile fact came from: an owner edit pattern (Q37) or a goal answer (Q62).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreferenceOrigin {
    Edit,
    Answer,
}

/// Type of a link between two goals (Q7). For `Parent`, `from_goal_id` is the parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    Parent,
    DependsOn,
    Supports,
    Conflicts,
}

/// Kind of a goal file; `Link` is a URL with no stored bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Document,
    Image,
    Audio,
    Video,
    Link,
}

/// Progress of chunking and embedding a goal file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestStatus {
    Pending,
    Processing,
    Done,
    Failed,
}

/// Value type of one tracker field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Number,
    Text,
    Bool,
    Choice,
}

/// An Apple Health metric a tracker field imports on iPhone (Q21).
/// `WorkoutType` fills a text field with the activity name; every other metric fills a
/// number field (kg or the field's unit, minutes, kcal, steps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthMetric {
    BodyMass,
    WorkoutMinutes,
    WorkoutEnergy,
    WorkoutType,
    Steps,
    DietaryEnergy,
}

/// One field of a tracker's schema. `choices` is used only by `Choice` fields.
/// `health` names the Apple Health metric that fills the field, if any.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackerField {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub choices: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<HealthMetric>,
}

/// Check-in frequency of a tracker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CadenceFreq {
    Daily,
    Weekly,
}

/// When a tracker asks for a check-in: daily, or weekly on `weekdays`, at local `at` (`HH:MM`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cadence {
    pub freq: CadenceFreq,
    #[serde(default)]
    pub weekdays: Vec<Weekday>,
    pub at: String,
}

/// Test of a tracker rule over its window: the average is `Above` or
/// `Below` the value, the values stay `Flat` within the value, or at
/// least the value check-ins were `Missed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleOp {
    Above,
    Below,
    Flat,
    Missed,
}

/// A tracker rule (Q20) over the last `window_days` days: the average of
/// number field `field` above or below `value`, its values flat within
/// `value`, or `value` missed check-ins of the tracker's cadence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackerRule {
    pub field: String,
    pub op: RuleOp,
    pub value: f64,
    pub window_days: u32,
}

/// Where a tracker entry came from (Q21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntrySource {
    CheckIn,
    Log,
    Health,
}

/// State of a planned action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    Planned,
    Done,
    Missed,
    Skipped,
}

/// Size of a plan change; Balanced autonomy auto-applies only `Small`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalSize {
    Small,
    Big,
}

/// State of a proposal in the Inbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Proposed,
    Applied,
    Rejected,
    Undone,
}

/// Triage decision of a calendar rule (Q23-24).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleDecision {
    Keep,
    Ignore,
}

/// What a device all-day event does to its days (Q47): `Reminder` keeps the plan,
/// `Busy` blocks the day and moves its actions to other days, `Away` has the
/// replanner adapt each goal by category. Proposed by triage, confirmed by the owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayKind {
    Reminder,
    Busy,
    Away,
}

/// How a named place was added (Q56): answered from a place prompt, or found by map search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceSource {
    Prompt,
    Search,
}

/// Level a spend limit applies to (Q32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetScope {
    Call,
    Run,
    GoalMonth,
    UserMonth,
}

synced! {
    /// A goal category, seeded (Q6) or owner-defined. `color` is `#rrggbb`.
    Category, CATEGORIES, "categories" {
        name: String => Text,
        color: String => Text,
        seeded: bool => Bool,
    }
}

synced! {
    /// A goal: one category, free-form context, optional deadline (`YYYY-MM-DD`),
    /// the autonomy level agents follow when changing its plan, and its agent
    /// controls: local reminders, calendar reads and a run time limit in minutes
    /// (`None` uses the Settings default).
    Goal, GOALS, "goals" {
        category_id: Option<String> => Text,
        title: String => Text,
        context: Option<String> => Text,
        deadline: Option<String> => Text,
        status: GoalStatus => Text,
        autonomy: Autonomy => Text,
        created_at: DateTime<Utc> => Time,
        reminders: bool => Bool,
        use_calendars: bool => Bool,
        run_limit_minutes: Option<i64> => Int,
    }
}

synced! {
    /// A typed link between two goals, proposed by the orchestrator and
    /// `confirmed` by the owner (Q7).
    GoalLink, GOAL_LINKS, "goal_links" {
        from_goal_id: String => Text,
        to_goal_id: String => Text,
        kind: LinkKind => Text,
        confirmed: bool => Bool,
    }
}

synced! {
    /// A file in a goal's virtual folder (Q10): bytes in Storage under
    /// `goals/<goal_id>/`, or a fetched `https` link. `extracted_text` is the
    /// text the device read from the file, at most the first 50,000 characters.
    GoalFile, GOAL_FILES, "goal_files" {
        goal_id: String => Text,
        name: String => Text,
        kind: FileKind => Text,
        storage_path: Option<String> => Text,
        url: Option<String> => Text,
        mime: Option<String> => Text,
        size_bytes: i64 => Int,
        ingest_status: IngestStatus => Text,
        extracted_text: Option<String> => Text,
    }
}

synced! {
    /// A persona TOML, scoped to one goal or, with no `goal_id`, to the user library (Q9).
    Persona, PERSONAS, "personas" {
        goal_id: Option<String> => Text,
        name: String => Text,
        toml: String => Text,
    }
}

synced! {
    /// A plugin bundle TOML with its version and enabled flag (Q16).
    Plugin, PLUGINS, "plugins" {
        goal_id: Option<String> => Text,
        name: String => Text,
        version: i64 => Int,
        bundle_toml: String => Text,
        enabled: bool => Bool,
    }
}

synced! {
    /// A tracker: field schema, optional check-in cadence and adaptation rules (Q21).
    Tracker, TRACKERS, "trackers" {
        goal_id: String => Text,
        plugin_id: Option<String> => Text,
        name: String => Text,
        fields: Vec<TrackerField> => Json,
        cadence: Option<Cadence> => Json,
        rules: Vec<TrackerRule> => Json,
    }
}

synced! {
    /// One tracker entry: values keyed by tracker field key, its source and raw text.
    TrackerEntry, TRACKER_ENTRIES, "tracker_entries" {
        tracker_id: String => Text,
        data: Map<String, Value> => Json,
        source: EntrySource => Text,
        raw_text: Option<String> => Text,
        logged_at: DateTime<Utc> => Time,
    }
}

synced! {
    /// A recurring action (Q34): RRULE line, local start `YYYY-MM-DDTHH:MM:SS`
    /// in IANA `timezone`, and the length of each occurrence.
    ActionSeries, ACTION_SERIES, "action_series" {
        goal_id: String => Text,
        title: String => Text,
        notes: Option<String> => Text,
        rrule: String => Text,
        dtstart_local: String => Text,
        timezone: String => Text,
        duration_minutes: i64 => Int,
    }
}

synced! {
    /// A planned action. Occurrences of a series carry `series_id` and their
    /// original `occurrence` instant; `locked` actions are never moved without asking.
    Action, ACTIONS, "actions" {
        goal_id: String => Text,
        series_id: Option<String> => Text,
        occurrence: Option<DateTime<Utc>> => Time,
        title: String => Text,
        notes: Option<String> => Text,
        start_at: DateTime<Utc> => Time,
        end_at: DateTime<Utc> => Time,
        status: ActionStatus => Text,
        locked: bool => Bool,
        done_at: Option<DateTime<Utc>> => Time,
    }
}

synced! {
    /// A plan change for the Inbox: diff, size, state and the data to undo it.
    Proposal, PROPOSALS, "proposals" {
        goal_id: String => Text,
        run_id: Option<String> => Text,
        summary: String => Text,
        diff: Value => Json,
        size: ProposalSize => Text,
        status: ProposalStatus => Text,
        undo: Option<Value> => Json,
        created_at: DateTime<Utc> => Time,
    }
}

synced! {
    /// A user profile fact with its source and date (Q37, Q62): learned from
    /// edits (`origin` edit, confirmed by the owner) or from goal answers
    /// (`origin` answer, `topic` and source `goal_id`). A `blocked` row is a
    /// "never learn this" topic, never shown or read as a fact.
    Preference, PREFERENCES, "preferences" {
        fact: String => Text,
        source: String => Text,
        noted_at: DateTime<Utc> => Time,
        confirmed: bool => Bool,
        topic: Option<String> => Text,
        goal_id: Option<String> => Text,
        origin: PreferenceOrigin => Text,
        blocked: bool => Bool,
    }
}

synced! {
    /// A kept device calendar event, synced as busy time with its triage group.
    BusyBlock, BUSY_BLOCKS, "busy_blocks" {
        calendar_id: String => Text,
        event_id: String => Text,
        title: String => Text,
        start_at: DateTime<Utc> => Time,
        end_at: DateTime<Utc> => Time,
        all_day: bool => Bool,
        group_name: Option<String> => Text,
        day_kind: Option<DayKind> => Text,
    }
}

synced! {
    /// A calendar triage rule: keep or ignore events matching a calendar,
    /// a recurring series or a title pattern.
    CalendarRule, CALENDAR_RULES, "calendar_rules" {
        calendar_id: Option<String> => Text,
        series_key: Option<String> => Text,
        pattern: Option<String> => Text,
        group_name: String => Text,
        decision: RuleDecision => Text,
        day_kind: Option<DayKind> => Text,
    }
}

synced! {
    /// A model provider instance chosen in Settings. The key lives only in the
    /// server-side `provider_credentials` table, keyed by this row's id.
    ProviderSetting, PROVIDER_SETTINGS, "provider_settings" {
        label: String => Text,
        vendor: String => Text,
        model: String => Text,
        base_url: Option<String> => Text,
        is_default: bool => Bool,
    }
}

synced! {
    /// A web search instance; lower `priority` is tried first (Q14).
    SearchSetting, SEARCH_SETTINGS, "search_settings" {
        label: String => Text,
        provider: String => Text,
        base_url: Option<String> => Text,
        priority: i64 => Int,
        enabled: bool => Bool,
    }
}

synced! {
    /// A spend limit in micro USD at one level; `goal_id` is set only for `goal_month`.
    Budget, BUDGETS, "budgets" {
        scope: BudgetScope => Text,
        goal_id: Option<String> => Text,
        limit_usd_micros: i64 => Int,
    }
}

synced! {
    /// Token and cost usage of one model call, written by the server engine.
    Usage, USAGE, "usage" {
        goal_id: Option<String> => Text,
        run_id: Option<String> => Text,
        vendor: String => Text,
        model: String => Text,
        input_tokens: i64 => Int,
        output_tokens: i64 => Int,
        cost_usd_micros: i64 => Int,
        occurred_at: DateTime<Utc> => Time,
    }
}

synced! {
    /// A place the owner named (Q56): center in microdegrees, arrival radius in meters.
    /// Only named places sync; raw visits and unnamed candidates stay on the phone.
    Place, PLACES, "places" {
        name: String => Text,
        address: Option<String> => Text,
        lat_e6: i64 => Int,
        lon_e6: i64 => Int,
        radius_m: i64 => Int,
        source: PlaceSource => Text,
    }
}

synced! {
    /// A link from a named place to a goal and, for an approved routine, to its series.
    PlaceLink, PLACE_LINKS, "place_links" {
        place_id: String => Text,
        goal_id: String => Text,
        series_id: Option<String> => Text,
    }
}

const TOML_MAX: usize = 64 * 1024;
const NOTES_MAX: usize = 4000;

/// Checks that `s` is a lowercase hyphenated uuid, as every row id is.
/// Errors with `Invalid` naming `field`.
pub fn check_uuid(field: &'static str, s: &str) -> Result<()> {
    match uuid::Uuid::parse_str(s) {
        Ok(u) if u.hyphenated().to_string() == s => Ok(()),
        _ => Err(invalid(field, "not a lowercase hyphenated uuid")),
    }
}

fn check_opt_uuid(field: &'static str, s: &Option<String>) -> Result<()> {
    s.as_deref().map_or(Ok(()), |s| check_uuid(field, s))
}

fn check_text(field: &'static str, s: &str, min: usize, max: usize) -> Result<()> {
    let n = s.chars().count();
    if n < min || n > max {
        return Err(invalid(field, format!("length {n} outside {min}..={max}")));
    }
    if s.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')) {
        return Err(invalid(field, "control character"));
    }
    Ok(())
}

fn check_opt_text(field: &'static str, s: &Option<String>, max: usize) -> Result<()> {
    s.as_deref().map_or(Ok(()), |s| check_text(field, s, 1, max))
}

fn check_slug(field: &'static str, s: &str, max: usize) -> Result<()> {
    check_text(field, s, 1, max)?;
    if !s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-') {
        return Err(invalid(field, "only a-z, 0-9, _ and -"));
    }
    Ok(())
}

fn check_non_negative(field: &'static str, n: i64) -> Result<()> {
    if n < 0 {
        return Err(invalid(field, "negative"));
    }
    Ok(())
}

/// Checks a service URL: `https://` with a host, or `http://` only for
/// `localhost` or `127.0.0.1` (a self-hosted engine or a local model).
/// Errors with `Invalid` naming `field` otherwise.
pub fn check_url(field: &'static str, url: &str) -> Result<()> {
    check_text(field, url, 1, 2048)?;
    if url.chars().any(char::is_whitespace) {
        return Err(invalid(field, "whitespace in url"));
    }
    let rest = if let Some(rest) = url.strip_prefix("https://") {
        rest
    } else if let Some(rest) = url.strip_prefix("http://") {
        let host = rest.split(['/', ':', '?', '#']).next().unwrap_or("");
        if host != "localhost" && host != "127.0.0.1" {
            return Err(invalid(field, "http is allowed only for localhost"));
        }
        rest
    } else {
        return Err(invalid(field, "must start with https://"));
    };
    let host = rest.split(['/', ':', '?', '#']).next().unwrap_or("");
    if host.is_empty() || host.contains('@') {
        return Err(invalid(field, "missing host"));
    }
    Ok(())
}

/// Checks a public API key such as a Supabase anon key: 20 to 4096 visible
/// ASCII characters, no spaces. Errors with `Invalid` naming `field`.
/// Never used for provider keys, which the app does not keep.
pub fn check_public_key(field: &'static str, key: &str) -> Result<()> {
    if !(20..=4096).contains(&key.len()) || !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(invalid(field, "20 to 4096 visible characters"));
    }
    Ok(())
}

fn check_span(field: &'static str, start: &DateTime<Utc>, end: &DateTime<Utc>) -> Result<()> {
    if end <= start {
        return Err(invalid(field, "end must be after start"));
    }
    Ok(())
}

impl Category {
    fn check_fields(&self) -> Result<()> {
        check_text("categories.name", &self.name, 1, 60)?;
        let hex = self.color.strip_prefix('#').unwrap_or("");
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid("categories.color", "must be #rrggbb"));
        }
        Ok(())
    }
}

impl Goal {
    fn check_fields(&self) -> Result<()> {
        check_opt_uuid("goals.category_id", &self.category_id)?;
        check_text("goals.title", &self.title, 1, 200)?;
        check_opt_text("goals.context", &self.context, 20_000)?;
        if let Some(deadline) = &self.deadline {
            NaiveDate::parse_from_str(deadline, "%Y-%m-%d")
                .map_err(|_| invalid("goals.deadline", "must be YYYY-MM-DD"))?;
        }
        if let Some(minutes) = self.run_limit_minutes {
            if !(1..=120).contains(&minutes) {
                return Err(invalid("goals.run_limit_minutes", "must be 1..=120"));
            }
        }
        Ok(())
    }
}

impl GoalLink {
    fn check_fields(&self) -> Result<()> {
        check_uuid("goal_links.from_goal_id", &self.from_goal_id)?;
        check_uuid("goal_links.to_goal_id", &self.to_goal_id)?;
        if self.from_goal_id == self.to_goal_id {
            return Err(invalid("goal_links.to_goal_id", "a goal cannot link to itself"));
        }
        Ok(())
    }
}

/// Most characters of a goal file's extracted text that are stored and synced.
pub const FILE_TEXT_LIMIT: usize = 50_000;

impl GoalFile {
    fn check_fields(&self) -> Result<()> {
        check_uuid("goal_files.goal_id", &self.goal_id)?;
        check_text("goal_files.name", &self.name, 1, 255)?;
        check_opt_text("goal_files.mime", &self.mime, 255)?;
        check_non_negative("goal_files.size_bytes", self.size_bytes)?;
        check_opt_text("goal_files.extracted_text", &self.extracted_text, FILE_TEXT_LIMIT)?;
        match (self.kind, &self.storage_path, &self.url) {
            (FileKind::Link, None, Some(url)) => {
                if !url.starts_with("https://") {
                    return Err(invalid("goal_files.url", "links must be https"));
                }
                check_url("goal_files.url", url)
            }
            (FileKind::Link, _, _) => Err(invalid("goal_files.url", "a link has a url and no storage_path")),
            (_, Some(path), None) => {
                check_text("goal_files.storage_path", path, 1, 1024)?;
                let prefix = format!("goals/{}/", self.goal_id);
                let rest = path.strip_prefix(&prefix).unwrap_or("");
                if rest.is_empty() || rest.split('/').any(|p| p.is_empty() || p == "." || p == "..") || rest.contains('\\') {
                    return Err(invalid("goal_files.storage_path", "must be a file under goals/<goal_id>/"));
                }
                Ok(())
            }
            _ => Err(invalid("goal_files.storage_path", "a stored file has a storage_path and no url")),
        }
    }
}

impl Persona {
    fn check_fields(&self) -> Result<()> {
        check_opt_uuid("personas.goal_id", &self.goal_id)?;
        check_text("personas.name", &self.name, 1, 100)?;
        check_text("personas.toml", &self.toml, 1, TOML_MAX)
    }
}

impl Plugin {
    fn check_fields(&self) -> Result<()> {
        check_opt_uuid("plugins.goal_id", &self.goal_id)?;
        check_text("plugins.name", &self.name, 1, 100)?;
        if self.version < 1 {
            return Err(invalid("plugins.version", "versions start at 1"));
        }
        check_text("plugins.bundle_toml", &self.bundle_toml, 1, TOML_MAX)
    }
}

impl Tracker {
    fn check_fields(&self) -> Result<()> {
        check_uuid("trackers.goal_id", &self.goal_id)?;
        check_opt_uuid("trackers.plugin_id", &self.plugin_id)?;
        check_text("trackers.name", &self.name, 1, 100)?;
        if self.fields.is_empty() || self.fields.len() > 50 {
            return Err(invalid("trackers.fields", "between 1 and 50 fields"));
        }
        let mut keys = BTreeSet::new();
        for field in &self.fields {
            check_slug("trackers.fields.key", &field.key, 40)?;
            check_text("trackers.fields.label", &field.label, 1, 100)?;
            check_opt_text("trackers.fields.unit", &field.unit, 20)?;
            if !keys.insert(field.key.as_str()) {
                return Err(invalid("trackers.fields.key", format!("duplicate key {}", field.key)));
            }
            match field.kind {
                FieldKind::Choice if field.choices.is_empty() => {
                    return Err(invalid("trackers.fields.choices", "a choice field needs choices"));
                }
                FieldKind::Choice => {}
                _ if !field.choices.is_empty() => {
                    return Err(invalid("trackers.fields.choices", "only choice fields have choices"));
                }
                _ => {}
            }
            let fits = match field.health {
                None => true,
                Some(HealthMetric::WorkoutType) => field.kind == FieldKind::Text,
                Some(_) => field.kind == FieldKind::Number,
            };
            if !fits {
                return Err(invalid("trackers.fields.health", format!("{} does not fit a {:?} field", field.key, field.kind)));
            }
        }
        if let Some(cadence) = &self.cadence {
            NaiveTime::parse_from_str(&cadence.at, "%H:%M").map_err(|_| invalid("trackers.cadence.at", "must be HH:MM"))?;
            match cadence.freq {
                CadenceFreq::Daily if !cadence.weekdays.is_empty() => {
                    return Err(invalid("trackers.cadence.weekdays", "daily cadence has no weekdays"));
                }
                CadenceFreq::Weekly if cadence.weekdays.is_empty() => {
                    return Err(invalid("trackers.cadence.weekdays", "weekly cadence needs weekdays"));
                }
                _ => {}
            }
        }
        if self.rules.len() > 20 {
            return Err(invalid("trackers.rules", "at most 20 rules"));
        }
        for rule in &self.rules {
            let field = self.fields.iter().find(|f| f.key == rule.field);
            if rule.op == RuleOp::Missed {
                if field.is_none() {
                    return Err(invalid("trackers.rules.field", format!("{} is not a field", rule.field)));
                }
                if self.cadence.is_none() {
                    return Err(invalid("trackers.rules", "a missed check-in rule needs a cadence"));
                }
                if rule.value < 1.0 || rule.value.fract() != 0.0 {
                    return Err(invalid("trackers.rules", "missed check-ins are a whole count of at least 1"));
                }
            } else if field.map(|f| f.kind) != Some(FieldKind::Number) {
                return Err(invalid("trackers.rules.field", format!("{} is not a number field", rule.field)));
            }
            if !rule.value.is_finite() || !(1..=365).contains(&rule.window_days) {
                return Err(invalid("trackers.rules", "finite value and window of 1 to 365 days"));
            }
            if rule.op == RuleOp::Flat && rule.value < 0.0 {
                return Err(invalid("trackers.rules", "a flat rule's tolerance is not negative"));
            }
        }
        Ok(())
    }
}

impl TrackerEntry {
    fn check_fields(&self) -> Result<()> {
        check_uuid("tracker_entries.tracker_id", &self.tracker_id)?;
        if self.data.is_empty() || self.data.len() > 50 {
            return Err(invalid("tracker_entries.data", "between 1 and 50 values"));
        }
        for key in self.data.keys() {
            check_slug("tracker_entries.data", key, 40)?;
        }
        check_opt_text("tracker_entries.raw_text", &self.raw_text, NOTES_MAX)
    }
}

/// Checks that `entry` belongs to `tracker` and that every value names one of
/// its fields with a matching type (a choice value must be one of the choices).
/// Errors with `Invalid` naming the first bad value.
pub fn validate_entry(tracker: &Tracker, entry: &TrackerEntry) -> Result<()> {
    if entry.tracker_id != tracker.id {
        return Err(invalid("tracker_entries.tracker_id", "entry belongs to another tracker"));
    }
    for (key, value) in &entry.data {
        let Some(field) = tracker.fields.iter().find(|f| &f.key == key) else {
            return Err(invalid("tracker_entries.data", format!("{key} is not a tracker field")));
        };
        let ok = match field.kind {
            FieldKind::Number => value.as_f64().is_some_and(f64::is_finite),
            FieldKind::Text => value.as_str().is_some_and(|s| s.chars().count() <= NOTES_MAX),
            FieldKind::Bool => value.is_boolean(),
            FieldKind::Choice => value.as_str().is_some_and(|s| field.choices.iter().any(|c| c == s)),
        };
        if !ok {
            return Err(invalid("tracker_entries.data", format!("{key} has the wrong type")));
        }
    }
    Ok(())
}

impl ActionSeries {
    fn check_fields(&self) -> Result<()> {
        check_uuid("action_series.goal_id", &self.goal_id)?;
        check_text("action_series.title", &self.title, 1, 200)?;
        check_opt_text("action_series.notes", &self.notes, NOTES_MAX)?;
        if !(1..=1440).contains(&self.duration_minutes) {
            return Err(invalid("action_series.duration_minutes", "between 1 and 1440"));
        }
        crate::recur::rule_set(self).map(|_| ())
    }
}

impl Action {
    fn check_fields(&self) -> Result<()> {
        check_uuid("actions.goal_id", &self.goal_id)?;
        check_opt_uuid("actions.series_id", &self.series_id)?;
        if self.series_id.is_some() != self.occurrence.is_some() {
            return Err(invalid("actions.occurrence", "set together with series_id"));
        }
        check_text("actions.title", &self.title, 1, 200)?;
        check_opt_text("actions.notes", &self.notes, NOTES_MAX)?;
        check_span("actions.end_at", &self.start_at, &self.end_at)?;
        if self.done_at.is_some() && self.status != ActionStatus::Done {
            return Err(invalid("actions.done_at", "only done actions have done_at"));
        }
        Ok(())
    }
}

impl Proposal {
    fn check_fields(&self) -> Result<()> {
        check_uuid("proposals.goal_id", &self.goal_id)?;
        check_opt_uuid("proposals.run_id", &self.run_id)?;
        check_text("proposals.summary", &self.summary, 1, 2000)?;
        if !(self.diff.is_object() || self.diff.is_array()) {
            return Err(invalid("proposals.diff", "must be a JSON object or array"));
        }
        Ok(())
    }
}

impl Preference {
    fn check_fields(&self) -> Result<()> {
        check_text("preferences.fact", &self.fact, 1, 1000)?;
        check_text("preferences.source", &self.source, 1, 60)?;
        check_opt_text("preferences.topic", &self.topic, 60)?;
        check_opt_uuid("preferences.goal_id", &self.goal_id)?;
        if self.blocked && self.topic.is_none() {
            return Err(invalid("preferences.topic", "a blocked preference names its topic"));
        }
        Ok(())
    }
}

impl BusyBlock {
    fn check_fields(&self) -> Result<()> {
        check_text("busy_blocks.calendar_id", &self.calendar_id, 1, 255)?;
        check_text("busy_blocks.event_id", &self.event_id, 1, 255)?;
        check_text("busy_blocks.title", &self.title, 0, 500)?;
        check_span("busy_blocks.end_at", &self.start_at, &self.end_at)?;
        check_opt_text("busy_blocks.group_name", &self.group_name, 100)
    }
}

impl CalendarRule {
    fn check_fields(&self) -> Result<()> {
        check_opt_text("calendar_rules.calendar_id", &self.calendar_id, 255)?;
        check_opt_text("calendar_rules.series_key", &self.series_key, 255)?;
        check_opt_text("calendar_rules.pattern", &self.pattern, 200)?;
        if self.calendar_id.is_none() && self.series_key.is_none() && self.pattern.is_none() {
            return Err(invalid("calendar_rules.pattern", "a rule needs a calendar, series or pattern"));
        }
        check_text("calendar_rules.group_name", &self.group_name, 1, 100)
    }
}

impl ProviderSetting {
    fn check_fields(&self) -> Result<()> {
        check_text("provider_settings.label", &self.label, 1, 60)?;
        check_slug("provider_settings.vendor", &self.vendor, 40)?;
        check_text("provider_settings.model", &self.model, 1, 200)?;
        self.base_url.as_deref().map_or(Ok(()), |u| check_url("provider_settings.base_url", u))
    }
}

impl SearchSetting {
    fn check_fields(&self) -> Result<()> {
        check_text("search_settings.label", &self.label, 1, 60)?;
        check_slug("search_settings.provider", &self.provider, 40)?;
        if !(0..=1000).contains(&self.priority) {
            return Err(invalid("search_settings.priority", "between 0 and 1000"));
        }
        self.base_url.as_deref().map_or(Ok(()), |u| check_url("search_settings.base_url", u))
    }
}

impl Budget {
    fn check_fields(&self) -> Result<()> {
        check_opt_uuid("budgets.goal_id", &self.goal_id)?;
        if (self.scope == BudgetScope::GoalMonth) != self.goal_id.is_some() {
            return Err(invalid("budgets.goal_id", "set exactly for goal_month budgets"));
        }
        if self.limit_usd_micros <= 0 {
            return Err(invalid("budgets.limit_usd_micros", "must be positive"));
        }
        Ok(())
    }
}

impl Usage {
    fn check_fields(&self) -> Result<()> {
        check_opt_uuid("usage.goal_id", &self.goal_id)?;
        check_opt_uuid("usage.run_id", &self.run_id)?;
        check_slug("usage.vendor", &self.vendor, 40)?;
        check_text("usage.model", &self.model, 1, 200)?;
        check_non_negative("usage.input_tokens", self.input_tokens)?;
        check_non_negative("usage.output_tokens", self.output_tokens)?;
        check_non_negative("usage.cost_usd_micros", self.cost_usd_micros)
    }
}

/// Smallest and largest arrival radius of a place, in meters.
pub const PLACE_RADIUS_M: std::ops::RangeInclusive<i64> = 50..=1000;

/// Checks a latitude and longitude in microdegrees: within plus or minus 90 and 180 degrees.
/// Errors with `Invalid` naming `field`.
pub fn check_coordinate(field: &'static str, lat_e6: i64, lon_e6: i64) -> Result<()> {
    if !(-90_000_000..=90_000_000).contains(&lat_e6) || !(-180_000_000..=180_000_000).contains(&lon_e6) {
        return Err(invalid(field, "outside the globe"));
    }
    Ok(())
}

impl Place {
    fn check_fields(&self) -> Result<()> {
        check_text("places.name", &self.name, 1, 100)?;
        check_opt_text("places.address", &self.address, 300)?;
        check_coordinate("places.lat_e6", self.lat_e6, self.lon_e6)?;
        if !PLACE_RADIUS_M.contains(&self.radius_m) {
            return Err(invalid("places.radius_m", "between 50 and 1000"));
        }
        Ok(())
    }
}

impl PlaceLink {
    fn check_fields(&self) -> Result<()> {
        check_uuid("place_links.place_id", &self.place_id)?;
        check_uuid("place_links.goal_id", &self.goal_id)?;
        check_opt_uuid("place_links.series_id", &self.series_id)
    }
}

/// Valid sample rows of every synced table, for tests in this crate and in
/// aroh-store (feature `samples`). All rows belong to [`samples::USER`] and
/// goal [`samples::goal_id`].
#[cfg(any(test, feature = "samples"))]
pub mod samples {
    use super::*;
    use chrono::TimeZone;

    pub const USER: &str = "0190f2a0-0000-7000-8000-00000000000a";
    pub const STAMP: &str = "1763683200000-0000-00000000000000aa";

    fn id(n: u32) -> String {
        format!("0190f2a0-0000-7000-8000-{n:012x}")
    }

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 23, hour, 0, 0).unwrap()
    }

    /// A valid goal.
    pub fn goal() -> Goal {
        Goal {
            id: goal_id(),
            user_id: USER.into(),
            category_id: Some(id(1)),
            title: "Six pack by Feb 1".into(),
            context: Some("Gym three times a week".into()),
            deadline: Some("2027-02-01".into()),
            status: GoalStatus::Active,
            autonomy: Autonomy::Balanced,
            created_at: at(8),
            reminders: true,
            use_calendars: true,
            run_limit_minutes: None,
            updated_at: STAMP.into(),
            deleted: false,
        }
    }

    /// A valid confirmed preference learned from edits.
    pub fn preference() -> Preference {
        Preference {
            id: id(13),
            user_id: USER.into(),
            fact: "Prefers morning workouts".into(),
            source: "edit-analysis".into(),
            noted_at: at(8),
            confirmed: true,
            topic: None,
            goal_id: None,
            origin: PreferenceOrigin::Edit,
            blocked: false,
            updated_at: STAMP.into(),
            deleted: false,
        }
    }

    /// The id of [`goal`], a valid uuid.
    pub fn goal_id() -> String {
        id(0x91)
    }

    /// A valid tracker with weight and meal fields, a daily cadence and one rule.
    pub fn tracker() -> Tracker {
        Tracker {
            id: id(7),
            user_id: USER.into(),
            goal_id: goal_id(),
            plugin_id: None,
            name: "Body".into(),
            fields: vec![
                TrackerField { key: "weight".into(), label: "Weight".into(), kind: FieldKind::Number, unit: Some("kg".into()), choices: vec![], health: None },
                TrackerField { key: "meal".into(), label: "Meal".into(), kind: FieldKind::Choice, unit: None, choices: vec!["clean".into(), "cheat".into()], health: None },
            ],
            cadence: Some(Cadence { freq: CadenceFreq::Daily, weekdays: vec![], at: "20:00".into() }),
            rules: vec![TrackerRule { field: "weight".into(), op: RuleOp::Above, value: 80.0, window_days: 7 }],
            updated_at: STAMP.into(),
            deleted: false,
        }
    }

    /// A valid daily 07:00 Europe/Berlin series of 45 minute actions.
    pub fn series() -> ActionSeries {
        ActionSeries {
            id: id(9),
            user_id: USER.into(),
            goal_id: goal_id(),
            title: "Core workout".into(),
            notes: None,
            rrule: "FREQ=DAILY".into(),
            dtstart_local: "2026-10-20T07:00:00".into(),
            timezone: "Europe/Berlin".into(),
            duration_minutes: 45,
            updated_at: STAMP.into(),
            deleted: false,
        }
    }

    /// A valid one-off action.
    pub fn action() -> Action {
        Action {
            id: id(10),
            user_id: USER.into(),
            goal_id: goal_id(),
            series_id: None,
            occurrence: None,
            title: "Buy a scale".into(),
            notes: None,
            start_at: at(9),
            end_at: at(10),
            status: ActionStatus::Planned,
            locked: false,
            done_at: None,
            updated_at: STAMP.into(),
            deleted: false,
        }
    }

    /// A valid named gym place with a 150 m radius.
    pub fn place() -> Place {
        Place {
            id: id(20),
            user_id: USER.into(),
            name: "Iron Works Gym".into(),
            address: Some("1200 Market St".into()),
            lat_e6: 37_776_000,
            lon_e6: -122_417_000,
            radius_m: 150,
            source: PlaceSource::Prompt,
            updated_at: STAMP.into(),
            deleted: false,
        }
    }

    /// One valid row of every synced table as wire JSON, in registry order.
    pub fn all() -> Vec<Value> {
        let user = || USER.to_string();
        let stamp = || STAMP.to_string();
        let rows = vec![
            to(Category { id: id(1), user_id: user(), name: "Health and Fitness".into(), color: "#4f7a5a".into(), seeded: true, updated_at: stamp(), deleted: false }),
            to(goal()),
            to(GoalLink { id: id(3), user_id: user(), from_goal_id: goal_id(), to_goal_id: id(0x92), kind: LinkKind::Supports, confirmed: false, updated_at: stamp(), deleted: false }),
            to(GoalFile { id: id(4), user_id: user(), goal_id: goal_id(), name: "plan.pdf".into(), kind: FileKind::Document, storage_path: Some(format!("goals/{}/plan.pdf", goal_id())), url: None, mime: Some("application/pdf".into()), size_bytes: 1024, ingest_status: IngestStatus::Pending, extracted_text: None, updated_at: stamp(), deleted: false }),
            to(Persona { id: id(5), user_id: user(), goal_id: Some(goal_id()), name: "coach".into(), toml: "role = \"coach\"".into(), updated_at: stamp(), deleted: false }),
            to(Plugin { id: id(6), user_id: user(), goal_id: Some(goal_id()), name: "fitness".into(), version: 1, bundle_toml: "name = \"fitness\"".into(), enabled: true, updated_at: stamp(), deleted: false }),
            to(tracker()),
            to(TrackerEntry { id: id(8), user_id: user(), tracker_id: id(7), data: serde_json::json!({"weight": 81.5, "meal": "clean"}).as_object().unwrap().clone(), source: EntrySource::CheckIn, raw_text: None, logged_at: at(20), updated_at: stamp(), deleted: false }),
            to(series()),
            to(action()),
            to(Proposal { id: id(11), user_id: user(), goal_id: goal_id(), run_id: Some(id(12)), summary: "Move Friday workout".into(), diff: serde_json::json!([{"action_id": id(10), "op": "move"}]), size: ProposalSize::Small, status: ProposalStatus::Proposed, undo: None, created_at: at(8), updated_at: stamp(), deleted: false }),
            to(preference()),
            to(BusyBlock { id: id(14), user_id: user(), calendar_id: "work".into(), event_id: "evt-1".into(), title: "Standup".into(), start_at: at(9), end_at: at(10), all_day: true, group_name: Some("Work".into()), day_kind: Some(DayKind::Busy), updated_at: stamp(), deleted: false }),
            to(CalendarRule { id: id(15), user_id: user(), calendar_id: Some("work".into()), series_key: None, pattern: None, group_name: "Work".into(), decision: RuleDecision::Keep, day_kind: None, updated_at: stamp(), deleted: false }),
            to(ProviderSetting { id: id(16), user_id: user(), label: "Claude".into(), vendor: "anthropic".into(), model: "claude-opus".into(), base_url: None, is_default: true, updated_at: stamp(), deleted: false }),
            to(SearchSetting { id: id(17), user_id: user(), label: "Local SearXNG".into(), provider: "searxng".into(), base_url: Some("http://localhost:8888".into()), priority: 0, enabled: true, updated_at: stamp(), deleted: false }),
            to(Budget { id: id(18), user_id: user(), scope: BudgetScope::GoalMonth, goal_id: Some(goal_id()), limit_usd_micros: 5_000_000, updated_at: stamp(), deleted: false }),
            to(Usage { id: id(19), user_id: user(), goal_id: Some(goal_id()), run_id: Some(id(12)), vendor: "anthropic".into(), model: "claude-opus".into(), input_tokens: 1200, output_tokens: 300, cost_usd_micros: 4200, occurred_at: at(8), updated_at: stamp(), deleted: false }),
            to(place()),
            to(PlaceLink { id: id(21), user_id: user(), place_id: id(20), goal_id: goal_id(), series_id: Some(id(9)), updated_at: stamp(), deleted: false }),
        ];
        assert_eq!(rows.len(), crate::schema::SYNCED.len());
        rows
    }

    fn to<R: Record>(row: R) -> Value {
        serde_json::to_value(row).expect("row serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::samples;
    use super::*;
    use crate::schema::SYNCED;

    fn field_err(row: impl Record) -> &'static str {
        match row.validate() {
            Err(crate::Error::Invalid { field, .. }) => field,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn every_sample_has_exactly_the_registry_columns_and_validates() {
        for (table, row) in SYNCED.iter().zip(samples::all()) {
            let keys: Vec<&str> = row.as_object().unwrap().keys().map(String::as_str).collect();
            let mut expected = table.names();
            expected.sort_unstable();
            let mut keys_sorted = keys.clone();
            keys_sorted.sort_unstable();
            assert_eq!(keys_sorted, expected, "{}", table.name);
            (table.check)(&row).unwrap_or_else(|e| panic!("{}: {e}", table.name));
        }
    }

    #[test]
    fn check_rejects_unknown_columns_and_bad_sync_columns() {
        let mut row = samples::all()[1].clone();
        row["server_seq"] = 3.into();
        assert!((GOALS.check)(&row).is_err());
        let mut row = samples::all()[1].clone();
        row["updated_at"] = "yesterday".into();
        assert_eq!((GOALS.check)(&row), Err(crate::Error::InvalidHlc("yesterday".into())));
        let mut row = samples::all()[1].clone();
        row["id"] = "0190F2A0-0000-7000-8000-000000000091".into();
        assert!((GOALS.check)(&row).is_err());
        let mut row = samples::all()[1].clone();
        row["autonomy"] = "reckless".into();
        assert!((GOALS.check)(&row).is_err());
    }

    #[test]
    fn goal_controls_and_profile_columns_are_checked() {
        let goal = samples::goal();
        assert!(goal.reminders && goal.use_calendars && goal.run_limit_minutes.is_none());
        for minutes in [1, 120] {
            let mut ok = samples::goal();
            ok.run_limit_minutes = Some(minutes);
            ok.validate().unwrap();
        }
        for minutes in [0, 121] {
            let mut bad = samples::goal();
            bad.run_limit_minutes = Some(minutes);
            assert_eq!(field_err(bad), "goals.run_limit_minutes");
        }

        let pref = samples::preference();
        assert_eq!((pref.origin, pref.blocked, pref.topic.as_deref()), (PreferenceOrigin::Edit, false, None));
        let mut learned = samples::preference();
        learned.origin = PreferenceOrigin::Answer;
        learned.topic = Some("reading time".into());
        learned.goal_id = Some(samples::goal_id());
        learned.validate().unwrap();
        let mut blocked = learned.clone();
        blocked.blocked = true;
        blocked.validate().unwrap();
        blocked.topic = None;
        assert_eq!(field_err(blocked), "preferences.topic");
        let mut bad = learned.clone();
        bad.goal_id = Some("not-a-uuid".into());
        assert_eq!(field_err(bad), "preferences.goal_id");
        let mut bad = learned;
        bad.topic = Some("x".repeat(61));
        assert_eq!(field_err(bad), "preferences.topic");
        let mut row = serde_json::to_value(samples::preference()).unwrap();
        row["origin"] = "guess".into();
        assert!((PREFERENCES.check)(&row).is_err());
    }

    #[test]
    fn field_rules_reject_bad_values() {
        let mut goal = samples::goal();
        goal.title = String::new();
        assert_eq!(field_err(goal), "goals.title");
        let mut goal = samples::goal();
        goal.deadline = Some("Feb 1".into());
        assert_eq!(field_err(goal), "goals.deadline");

        let mut action = samples::action();
        action.end_at = action.start_at;
        assert_eq!(field_err(action), "actions.end_at");
        let mut action = samples::action();
        action.series_id = Some(samples::goal_id());
        assert_eq!(field_err(action), "actions.occurrence");

        let mut tracker = samples::tracker();
        tracker.fields.push(tracker.fields[0].clone());
        assert_eq!(field_err(tracker), "trackers.fields.key");
        let mut tracker = samples::tracker();
        tracker.rules[0].field = "meal".into();
        assert_eq!(field_err(tracker), "trackers.rules.field");
        let mut tracker = samples::tracker();
        tracker.cadence = Some(Cadence { freq: CadenceFreq::Weekly, weekdays: vec![], at: "20:00".into() });
        assert_eq!(field_err(tracker), "trackers.cadence.weekdays");
        let mut tracker = samples::tracker();
        tracker.cadence = Some(Cadence { freq: CadenceFreq::Daily, weekdays: vec![], at: "25:00".into() });
        assert_eq!(field_err(tracker), "trackers.cadence.at");
    }

    #[test]
    fn health_source_round_trips_and_must_fit_the_field_kind() {
        let mut tracker = samples::tracker();
        let plain = serde_json::to_value(&tracker.fields[0]).unwrap();
        assert!(plain.get("health").is_none());
        tracker.fields[0].health = Some(HealthMetric::BodyMass);
        let wire = serde_json::to_value(&tracker.fields[0]).unwrap();
        assert_eq!(wire["health"], "body_mass");
        let back: TrackerField = serde_json::from_value(wire).unwrap();
        assert_eq!(back.health, Some(HealthMetric::BodyMass));
        tracker.validate().unwrap();

        let mut tracker = samples::tracker();
        tracker.fields[0].health = Some(HealthMetric::WorkoutType);
        assert_eq!(field_err(tracker), "trackers.fields.health");
        let mut tracker = samples::tracker();
        tracker.fields[1].health = Some(HealthMetric::Steps);
        assert_eq!(field_err(tracker), "trackers.fields.health");
        let mut tracker = samples::tracker();
        tracker.fields[1].kind = FieldKind::Text;
        tracker.fields[1].choices.clear();
        tracker.fields[1].health = Some(HealthMetric::WorkoutType);
        tracker.validate().unwrap();
    }

    #[test]
    fn flat_and_missed_rules_validate() {
        let mut tracker = samples::tracker();
        tracker.rules[0] = TrackerRule { field: "weight".into(), op: RuleOp::Flat, value: 0.5, window_days: 14 };
        tracker.validate().unwrap();
        tracker.rules[0].value = -0.5;
        assert_eq!(field_err(tracker.clone()), "trackers.rules");
        tracker.rules[0] = TrackerRule { field: "meal".into(), op: RuleOp::Flat, value: 0.5, window_days: 14 };
        assert_eq!(field_err(tracker.clone()), "trackers.rules.field");
        tracker.rules[0] = TrackerRule { field: "meal".into(), op: RuleOp::Missed, value: 3.0, window_days: 7 };
        tracker.validate().unwrap();
        tracker.rules[0].value = 2.5;
        assert_eq!(field_err(tracker.clone()), "trackers.rules");
        tracker.rules[0].value = 0.0;
        assert_eq!(field_err(tracker.clone()), "trackers.rules");
        tracker.rules[0].value = 3.0;
        tracker.rules[0].field = "nope".into();
        assert_eq!(field_err(tracker.clone()), "trackers.rules.field");
        tracker.rules[0].field = "meal".into();
        tracker.cadence = None;
        assert_eq!(field_err(tracker), "trackers.rules");
        let rule: TrackerRule =
            serde_json::from_value(serde_json::json!({"field": "weight", "op": "missed", "value": 3, "window_days": 7})).unwrap();
        assert_eq!(rule.op, RuleOp::Missed);
    }

    #[test]
    fn file_paths_links_and_urls_are_checked() {
        let file: GoalFile = serde_json::from_value(samples::all()[3].clone()).unwrap();
        for bad in ["goals/other/plan.pdf", "plan.pdf", "goals/GOAL/../x"] {
            let mut f = file.clone();
            f.storage_path = Some(bad.replace("GOAL", &f.goal_id));
            assert_eq!(field_err(f), "goal_files.storage_path", "{bad}");
        }
        let mut f = file.clone();
        f.storage_path = Some(format!("goals/{}/../../x", f.goal_id));
        assert_eq!(field_err(f), "goal_files.storage_path");
        let mut link = file.clone();
        link.kind = FileKind::Link;
        link.storage_path = None;
        link.url = Some("https://example.com/plan".into());
        link.validate().unwrap();
        link.url = Some("http://example.com/plan".into());
        assert_eq!(field_err(link), "goal_files.url");
        let mut read = file.clone();
        read.extracted_text = Some("é".repeat(FILE_TEXT_LIMIT));
        read.validate().unwrap();
        read.extracted_text = Some("é".repeat(FILE_TEXT_LIMIT + 1));
        assert_eq!(field_err(read.clone()), "goal_files.extracted_text");
        read.extracted_text = Some(String::new());
        assert_eq!(field_err(read), "goal_files.extracted_text");

        assert!(check_url("u", "https://api.example.com/v1").is_ok());
        assert!(check_url("u", "http://localhost:11434").is_ok());
        assert!(check_url("u", "http://127.0.0.1/v1").is_ok());
        for bad in ["http://example.com", "ftp://x", "https://", "https://user@host", "https://a b", "http://localhost.evil.com"] {
            assert!(check_url("u", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn links_budgets_and_rules_are_checked() {
        let mut link: GoalLink = serde_json::from_value(samples::all()[2].clone()).unwrap();
        link.to_goal_id = link.from_goal_id.clone();
        assert_eq!(field_err(link), "goal_links.to_goal_id");
        let mut budget: Budget = serde_json::from_value(samples::all()[16].clone()).unwrap();
        budget.scope = BudgetScope::UserMonth;
        assert_eq!(field_err(budget.clone()), "budgets.goal_id");
        budget.goal_id = None;
        budget.validate().unwrap();
        budget.limit_usd_micros = 0;
        assert_eq!(field_err(budget), "budgets.limit_usd_micros");
        let mut rule: CalendarRule = serde_json::from_value(samples::all()[13].clone()).unwrap();
        rule.calendar_id = None;
        assert_eq!(field_err(rule), "calendar_rules.pattern");
        let mut usage: Usage = serde_json::from_value(samples::all()[17].clone()).unwrap();
        usage.output_tokens = -1;
        assert_eq!(field_err(usage), "usage.output_tokens");
    }

    #[test]
    fn places_check_coordinates_radius_and_links() {
        let mut place = samples::place();
        place.validate().unwrap();
        place.lat_e6 = 90_000_001;
        assert_eq!(field_err(place), "places.lat_e6");
        let mut place = samples::place();
        place.radius_m = 10;
        assert_eq!(field_err(place), "places.radius_m");
        let mut place = samples::place();
        place.name = String::new();
        assert_eq!(field_err(place), "places.name");
        let mut link: PlaceLink = serde_json::from_value(samples::all()[19].clone()).unwrap();
        link.validate().unwrap();
        link.goal_id = "gym".into();
        assert_eq!(field_err(link), "place_links.goal_id");
        assert_eq!(serde_json::to_value(PlaceSource::Search).unwrap(), "search");
    }

    #[test]
    fn day_kinds_round_trip_as_text() {
        for (kind, text) in [(DayKind::Reminder, "reminder"), (DayKind::Busy, "busy"), (DayKind::Away, "away")] {
            assert_eq!(serde_json::to_value(kind).unwrap(), text);
            assert_eq!(serde_json::from_value::<DayKind>(text.into()).unwrap(), kind);
        }
        assert!(serde_json::from_value::<DayKind>("holiday".into()).is_err());
        let mut block = samples::all()[12].clone();
        assert_eq!(block["day_kind"], "busy");
        (BUSY_BLOCKS.check)(&block).unwrap();
        block["day_kind"] = serde_json::Value::Null;
        (BUSY_BLOCKS.check)(&block).unwrap();
        block["day_kind"] = "holiday".into();
        assert!((BUSY_BLOCKS.check)(&block).is_err());
        let mut rule = samples::all()[13].clone();
        rule["day_kind"] = "away".into();
        (CALENDAR_RULES.check)(&rule).unwrap();
    }

    #[test]
    fn entries_must_match_their_tracker() {
        let tracker = samples::tracker();
        let entry: TrackerEntry = serde_json::from_value(samples::all()[7].clone()).unwrap();
        validate_entry(&tracker, &entry).unwrap();
        for bad in [serde_json::json!({"weight": "heavy"}), serde_json::json!({"meal": "pizza"}), serde_json::json!({"mood": 3})] {
            let mut e = entry.clone();
            e.data = bad.as_object().unwrap().clone();
            assert!(validate_entry(&tracker, &e).is_err(), "{bad}");
        }
        let mut other = entry.clone();
        other.tracker_id = samples::goal_id();
        assert!(validate_entry(&tracker, &other).is_err());
    }
}
