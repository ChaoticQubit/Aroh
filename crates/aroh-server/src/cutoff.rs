//! A structured answer cut off at the output cap continues instead of failing (Q90, B104).
//! [`ContinueModel`] keeps every complete entry of the cut JSON, asks the same model for the
//! rest in a new turn (no assistant prefill), merges, and repeats up to [`MAX_ROUNDS`] calls
//! within the run budget. It fails with the plain [`TOO_LONG`] only when a round adds nothing.

use crate::db;
use crate::error::store_err;
use crate::ledger::PgLedger;
use async_trait::async_trait;
use serde_json::{Map, Value};
use std::sync::Arc;
use troupekit_agent::budget;
use troupekit_core::clock::Clock;
use troupekit_core::error::{ProviderError, ProviderErrorKind};
use troupekit_core::ids::RunId;
use troupekit_core::message::{ContentPart, Message, Role};
use troupekit_providers::{CompletionRequest, CompletionResponse, Delta, Model, Price, StopReason};
use uuid::Uuid;

/// The owner-facing reason a run fails when its answer stays too long.
/// The app shows it as is (engine.ts `PLAIN_REASONS`).
/// Never a JSON parse error.
pub const TOO_LONG: &str = "The plan was too long to finish";

/// Model calls one answer may take, the first one included.
/// Bounds a reply that keeps hitting the output cap.
/// Each further call also passes the run budget.
pub const MAX_ROUNDS: u32 = 4;

/// Decides whether one more continuation call costing up to `estimate`
/// millionths of a US dollar fits the run's budgets.
/// Returns false to stop, or a ledger failure.
#[async_trait]
pub trait RoundBudget: Send + Sync {
    async fn allows(&self, estimate: u64) -> Result<bool, ProviderError>;
}

/// The run budget of one run io: the owner's budget rows, the run's spend
/// so far from `usage` and the month ledgers, checked the way Troupekit
/// checks a call. Built per model handle in `io::connect`.
pub struct RunBudget {
    pub pool: db::PgPool,
    pub clock: Arc<dyn Clock>,
    pub price: Price,
    pub user: Uuid,
    pub goal: Option<Uuid>,
    pub run: Option<RunId>,
}

#[async_trait]
impl RoundBudget for RunBudget {
    async fn allows(&self, estimate: u64) -> Result<bool, ProviderError> {
        let ledger_err =
            |e: String| ProviderError::new(ProviderErrorKind::Server, format!("usage ledger: {e}"));
        let limits = crate::engine::budget_limits(&self.pool, self.user, self.goal)
            .await
            .map_err(|e| ledger_err(e.to_string()))?;
        let spent: i64 = match self.run {
            None => 0,
            Some(run) => db::query_scalar(
                "select coalesce(sum(cost_usd_micros), 0)::bigint from public.usage where user_id = $1 and run_id = $2 and not deleted",
            )
            .bind(self.user)
            .bind(run.0)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ledger_err(store_err(e).to_string()))?,
        };
        let spent = u64::try_from(spent).expect("usage sums are non-negative");
        let ledger = PgLedger::new(self.pool.clone(), self.user, self.clock.clone());
        let crossed = budget::check(&limits, &self.price, spent, estimate, &ledger)
            .await
            .map_err(|e| ledger_err(e.to_string()))?;
        Ok(crossed.is_none())
    }
}

/// A model that turns a structured answer stopped on `max_tokens` into a
/// whole one over several calls, returning one reply with the merged JSON,
/// the summed usage and an `end_turn` stop. Other replies pass through.
pub struct ContinueModel {
    inner: Box<dyn Model>,
    price: Price,
    budget: Arc<dyn RoundBudget>,
}

impl ContinueModel {
    /// Wraps `inner`, priced at `price`, checking each continuation call
    /// against `budget`. The id stays the inner model's.
    /// Returns the model.
    pub fn new(inner: Box<dyn Model>, price: Price, budget: Arc<dyn RoundBudget>) -> ContinueModel {
        ContinueModel {
            inner,
            price,
            budget,
        }
    }

    async fn call(
        &self,
        req: &CompletionRequest,
        on_delta: &mut Option<&mut (dyn for<'d> FnMut(&'d Delta) + Send)>,
    ) -> Result<CompletionResponse, ProviderError> {
        match on_delta {
            Some(f) => self.inner.stream(req, &mut **f).await,
            None => self.inner.complete(req).await,
        }
    }

    async fn answer(
        &self,
        req: &CompletionRequest,
        mut on_delta: Option<&mut (dyn for<'d> FnMut(&'d Delta) + Send)>,
    ) -> Result<CompletionResponse, ProviderError> {
        let first = self.call(req, &mut on_delta).await?;
        let Some(schema) = &req.output_schema else {
            return Ok(first);
        };
        if first.stop != StopReason::MaxTokens || first.message.tool_calls().next().is_some() {
            return Ok(first);
        }
        let required = required_fields(schema);
        let mut usage = first.usage;
        let mut plan = Partial::default();
        if plan.absorb(salvage(&first.message.final_text())) == 0 {
            return Err(plain(TOO_LONG.into()));
        }
        let mut ended = false;
        for _ in 1..MAX_ROUNDS {
            if plan.complete(&required) {
                break;
            }
            let next = continuation(req, &plan, &required);
            if !self
                .budget
                .allows(budget::estimate(&next, &self.price))
                .await?
            {
                return Err(plain(format!(
                    "{TOO_LONG} within the budget. Raise it in Settings and run again"
                )));
            }
            let reply = self.call(&next, &mut on_delta).await?;
            usage += reply.usage;
            let added = plan.absorb(salvage(&reply.message.final_text()));
            if reply.stop != StopReason::MaxTokens {
                plan.open = None;
                ended = true;
                break;
            }
            if added == 0 && !plan.complete(&required) {
                return Err(plain(TOO_LONG.into()));
            }
        }
        if !ended && !plan.complete(&required) {
            return Err(plain(TOO_LONG.into()));
        }
        Ok(CompletionResponse {
            message: merged_message(first.message, plan.fields),
            usage,
            stop: StopReason::EndTurn,
        })
    }
}

#[async_trait]
impl Model for ContinueModel {
    fn id(&self) -> &str {
        self.inner.id()
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        self.answer(req, None).await
    }

    async fn stream(
        &self,
        req: &CompletionRequest,
        on_delta: &mut (dyn for<'d> FnMut(&'d Delta) + Send),
    ) -> Result<CompletionResponse, ProviderError> {
        self.answer(req, Some(on_delta)).await
    }
}

fn plain(message: String) -> ProviderError {
    ProviderError::new(ProviderErrorKind::BadRequest, message)
}

fn required_fields(schema: &Value) -> Vec<String> {
    schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// The answer kept so far: every complete top-level field, plus `open`,
/// the list field the last cut stopped in, holding its complete entries.
#[derive(Debug, Default, PartialEq)]
struct Partial {
    fields: Map<String, Value>,
    open: Option<String>,
}

/// What one reply holds: its complete top-level fields, and the list field
/// it was cut in with that list's complete entries.
#[derive(Debug, Default, PartialEq)]
struct Salvage {
    fields: Map<String, Value>,
    open: Option<(String, Vec<Value>)>,
}

impl Partial {
    /// Merges one reply into the answer: the open list gains the entries it
    /// lacks, an absent field is taken, a field already complete is kept.
    /// Returns how many fields and entries were added.
    fn absorb(&mut self, reply: Salvage) -> usize {
        let mut added = 0;
        for (key, value) in reply.fields {
            if self.open.as_deref() == Some(key.as_str()) {
                added += self.extend(&key, value.as_array().cloned().unwrap_or_default());
                self.open = None;
            } else if !self.fields.contains_key(&key) {
                self.fields.insert(key, value);
                added += 1;
            }
        }
        if let Some((key, entries)) = reply.open {
            if self.open.as_deref() == Some(key.as_str()) {
                added += self.extend(&key, entries);
            } else if self.open.is_none() && !self.fields.contains_key(&key) {
                added += entries.len();
                self.fields.insert(key.clone(), Value::Array(entries));
                self.open = Some(key);
            }
        }
        added
    }

    fn extend(&mut self, key: &str, entries: Vec<Value>) -> usize {
        let list = self
            .fields
            .get_mut(key)
            .and_then(Value::as_array_mut)
            .expect("the open field is a kept list");
        let before = list.len();
        for entry in entries {
            if !list.contains(&entry) {
                list.push(entry);
            }
        }
        list.len() - before
    }

    fn complete(&self, required: &[String]) -> bool {
        self.open.is_none() && required.iter().all(|r| self.fields.contains_key(r))
    }
}

/// The request for the rest: the original conversation, an assistant turn
/// with the kept JSON, and a user turn asking only for what is missing, so
/// the last turn is the user's (Opus 5.5 rejects assistant prefill).
fn continuation(req: &CompletionRequest, plan: &Partial, required: &[String]) -> CompletionRequest {
    let mut next = req.clone();
    next.messages.push(Message::text(
        Role::Assistant,
        Value::Object(plan.fields.clone()).to_string(),
    ));
    let done: Vec<&str> = plan
        .fields
        .keys()
        .map(String::as_str)
        .filter(|k| Some(*k) != plan.open.as_deref())
        .collect();
    let missing: Vec<&str> = required
        .iter()
        .map(String::as_str)
        .filter(|r| !plan.fields.contains_key(*r))
        .collect();
    let mut ask = String::from("Your answer stopped at the output limit. The JSON above is everything kept so far. Reply with the whole JSON shape again, holding only what is still missing.");
    if let Some(open) = &plan.open {
        ask.push_str(&format!(" The list `{open}` was cut off after its last entry above: give only the entries that come after it, never one already above."));
    }
    if !missing.is_empty() {
        ask.push_str(&format!(" Still missing: {}.", missing.join(", ")));
    }
    if !done.is_empty() {
        ask.push_str(&format!(" Fields already complete above ({}) are kept as they are: give an empty list for a list and a short value for the rest.", done.join(", ")));
    }
    next.messages.push(Message::text(Role::User, ask));
    next
}

fn merged_message(first: Message, fields: Map<String, Value>) -> Message {
    let keep = first
        .parts
        .iter()
        .rposition(|p| matches!(p, ContentPart::Hosted { .. }))
        .map_or(0, |i| i + 1);
    let mut parts: Vec<ContentPart> = first.parts.into_iter().take(keep).collect();
    parts.push(ContentPart::Text {
        text: Value::Object(fields).to_string(),
    });
    Message::new(Role::Assistant, parts).expect("hosted and text parts form an assistant message")
}

/// Reads what a possibly cut JSON object answer holds: every top-level
/// field whose value is complete, and for the field the text ends in, when
/// it is a list, its complete entries. A leading code fence is skipped.
fn salvage(text: &str) -> Salvage {
    let mut out = Salvage::default();
    let mut t = text.trim_start();
    if let Some(rest) = t.strip_prefix("```") {
        t = rest.strip_prefix("json").unwrap_or(rest).trim_start();
    }
    let b = t.as_bytes();
    if b.first() != Some(&b'{') {
        return out;
    }
    let mut i = skip_ws(b, 1);
    loop {
        if b.get(i) == Some(&b',') {
            i = skip_ws(b, i + 1);
        }
        if b.get(i) != Some(&b'"') {
            return out;
        }
        let Some(key_end) = string_end(b, i) else {
            return out;
        };
        let Ok(key) = serde_json::from_str::<String>(&t[i..key_end]) else {
            return out;
        };
        i = skip_ws(b, key_end);
        if b.get(i) != Some(&b':') {
            return out;
        }
        i = skip_ws(b, i + 1);
        match value_end(b, i) {
            Some(end) => {
                let Ok(value) = serde_json::from_str::<Value>(&t[i..end]) else {
                    return out;
                };
                out.fields.insert(key, value);
                i = skip_ws(b, end);
            }
            None => {
                if b.get(i) == Some(&b'[') {
                    out.open = Some((key, complete_entries(t, i)));
                }
                return out;
            }
        }
    }
}

fn complete_entries(t: &str, start: usize) -> Vec<Value> {
    let b = t.as_bytes();
    assert_eq!(b[start], b'[');
    let mut entries = Vec::new();
    let mut i = skip_ws(b, start + 1);
    while let Some(&c) = b.get(i) {
        if c == b',' {
            i = skip_ws(b, i + 1);
            continue;
        }
        let Some(end) = value_end(b, i) else { break };
        let Ok(entry) = serde_json::from_str::<Value>(&t[i..end]) else {
            break;
        };
        entries.push(entry);
        i = skip_ws(b, end);
    }
    entries
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while b.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    i
}

fn string_end(b: &[u8], start: usize) -> Option<usize> {
    assert_eq!(b[start], b'"');
    let mut i = start + 1;
    while let Some(&c) = b.get(i) {
        match c {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// The index just past the JSON value starting at `start`, or None when
/// the text ends inside it. A scalar is complete only once a delimiter
/// follows, since a cut `12` may have been `125`.
fn value_end(b: &[u8], start: usize) -> Option<usize> {
    match *b.get(start)? {
        b'"' => string_end(b, start),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut i = start;
            while let Some(&c) = b.get(i) {
                match c {
                    b'"' => {
                        i = string_end(b, i)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i + 1);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            None
        }
        _ => {
            let mut i = start;
            while let Some(&c) = b.get(i) {
                if matches!(c, b',' | b'}' | b']') || c.is_ascii_whitespace() {
                    return Some(i);
                }
                i += 1;
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{body_text, sse_text, sse_text_stop, FnTransport, Reply};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use troupekit_agent::{
        AgentRun, EvalRegistry, Limits, Persona, RunState, SimpleIo, StepOutcome,
    };
    use troupekit_core::secret::Secret;
    use troupekit_providers::Catalog;
    use troupekit_tools::ToolRegistry;

    const SELECTOR: &str = "anthropic/claude-opus-5-5";
    const MORE: &str = "stopped at the output limit";

    struct Allow(bool);

    #[async_trait]
    impl RoundBudget for Allow {
        async fn allows(&self, _estimate: u64) -> Result<bool, ProviderError> {
            Ok(self.0)
        }
    }

    fn item(n: u32) -> Value {
        json!({"title": format!("Chapter {n}"), "start": format!("2026-12-0{n}T19:00:00"), "minutes": 45, "rrule": null, "notes": format!("Read chapter {n}, note the {{key}} \"ideas\"")})
    }

    fn whole(items: &[Value]) -> String {
        let items: Vec<String> = items.iter().map(Value::to_string).collect();
        format!(
            r#"{{"summary": "Read the book", "timezone": "Europe/Berlin", "items": [{}], "trackers": [], "daily_minutes": 60, "needed_minutes": 180, "ordered": true, "ordered_why": "Chapters build on each other"}}"#,
            items.join(", ")
        )
    }

    fn cut_after(items: &[Value]) -> String {
        let text = whole(&[items, &[item(9)]].concat());
        let at = text
            .find("Read chapter 9")
            .expect("the cut item is in the text")
            + 27;
        text[..at].to_string()
    }

    async fn run_planner(
        handler: impl Fn(&str, usize) -> Reply + Send + Sync + 'static,
        allow: bool,
    ) -> (StepOutcome, Arc<FnTransport>) {
        let calls = AtomicUsize::new(0);
        let transport = FnTransport::new(move |req| {
            handler(&body_text(req), calls.fetch_add(1, Ordering::SeqCst))
        });
        let catalog = Catalog::load(crate::io::CATALOG).unwrap();
        let price = catalog.price(SELECTOR).unwrap();
        let inner = catalog
            .connect_with(
                SELECTOR,
                Some(Secret::new("sk-ant-test-key-0000".into()).unwrap()),
                transport.clone(),
            )
            .unwrap();
        let model = Arc::new(ContinueModel::new(inner, price, Arc::new(Allow(allow))));
        let mut evals = EvalRegistry::new();
        for name in ["within_availability", "notes_filled", "starts_after_now"] {
            evals
                .register(name, Arc::new(|_: &Value, _: &[Value]| Ok(())))
                .unwrap();
        }
        let mut io = SimpleIo::new(ToolRegistry::new()).with_model("default", model, price);
        io.evals = evals;
        let persona = Persona::parse(
            "practice-planner.toml",
            include_str!("../personas/practice-planner.toml"),
        )
        .unwrap();
        let mut run = AgentRun::new(
            &persona,
            vec![Message::text(Role::User, "Plan reading the book")],
            Limits::default(),
            &io,
        )
        .await
        .unwrap();
        let mut outcome = StepOutcome::Continue;
        for _ in 0..20 {
            outcome = run.step(&io).await;
            if !matches!(outcome, StepOutcome::Continue) {
                break;
            }
        }
        assert!(
            !matches!(run.state(), RunState::CallModel | RunState::Check { .. }),
            "the run settled"
        );
        (outcome, transport)
    }

    #[test]
    fn salvage_keeps_complete_entries_of_a_cut_list() {
        let s = salvage(&cut_after(&[item(1), item(2)]));
        assert_eq!(s.fields["summary"], "Read the book");
        assert_eq!(s.fields["timezone"], "Europe/Berlin");
        assert_eq!(s.open, Some(("items".to_string(), vec![item(1), item(2)])));
        let fenced = salvage(&format!("```json\n{}\n```", whole(&[item(1)])));
        assert_eq!(fenced.open, None);
        assert_eq!(fenced.fields["items"], json!([item(1)]));
        assert_eq!(salvage(r#"{"summary": "Rea"#), Salvage::default());
        assert_eq!(salvage(r#"{"daily_minutes": 12"#), Salvage::default());
    }

    #[tokio::test]
    async fn a_cut_plan_continues_to_a_whole_plan_in_two_rounds() {
        let first = cut_after(&[item(1), item(2)]);
        let (outcome, transport) = run_planner(
            move |body, _| {
                if body.contains(MORE) {
                    sse_text(&whole(&[item(3), item(4)]))
                } else {
                    sse_text_stop(&first, "max_tokens")
                }
            },
            true,
        )
        .await;
        let StepOutcome::Done(output) = outcome else {
            panic!("the run ends with a plan: {outcome:?}")
        };
        assert_eq!(output["items"], json!([item(1), item(2), item(3), item(4)]));
        assert_eq!(output["ordered"], true);
        let sent = transport.sent();
        assert_eq!(sent.len(), 2, "two rounds");
        let second: Value = serde_json::from_str(&body_text(&sent[1])).unwrap();
        assert_eq!(second["max_tokens"], 128000);
        let turns = second["messages"].as_array().unwrap();
        assert_eq!(
            turns.last().unwrap()["role"],
            "user",
            "no assistant prefill"
        );
        assert!(
            turns[turns.len() - 2].to_string().contains("Chapter 2"),
            "the kept items ride in the assistant turn"
        );
    }

    #[tokio::test]
    async fn a_round_that_adds_nothing_fails_plainly() {
        let first = cut_after(&[item(1)]);
        let (outcome, transport) =
            run_planner(move |_, _| sse_text_stop(&first, "max_tokens"), true).await;
        let StepOutcome::Failed(error) = outcome else {
            panic!("the run fails: {outcome:?}")
        };
        assert!(error.contains(TOO_LONG), "{error}");
        assert!(!error.contains("JSON"), "{error}");
        assert_eq!(transport.sent().len(), 2);
    }

    #[tokio::test]
    async fn the_round_limit_ends_an_answer_that_never_finishes() {
        let (outcome, transport) = run_planner(
            |_, call| {
                let kept: Vec<Value> = (1..=call as u32 + 1).map(item).collect();
                sse_text_stop(&cut_after(&kept), "max_tokens")
            },
            true,
        )
        .await;
        let StepOutcome::Failed(error) = outcome else {
            panic!("the run fails: {outcome:?}")
        };
        assert!(error.contains(TOO_LONG), "{error}");
        assert_eq!(transport.sent().len(), MAX_ROUNDS as usize);
    }

    #[tokio::test]
    async fn the_run_budget_stops_further_rounds() {
        let first = cut_after(&[item(1)]);
        let (outcome, transport) =
            run_planner(move |_, _| sse_text_stop(&first, "max_tokens"), false).await;
        let StepOutcome::Failed(error) = outcome else {
            panic!("the run fails: {outcome:?}")
        };
        assert!(error.contains("within the budget"), "{error}");
        assert_eq!(transport.sent().len(), 1);
    }
}
