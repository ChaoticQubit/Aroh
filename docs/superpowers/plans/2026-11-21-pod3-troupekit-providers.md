# Pod 3: troupekit-providers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A `troupekit-providers` crate that lets any Troupekit agent call any chat model: four wire protocols as pure encode and decode functions, one decoder path for unary and streamed replies, an injected `Transport`, a TOML provider catalog with prices, `Catalog::connect("vendor/model")` returning a type-erased `Box<dyn Model>`, and retry, fallback and cost-meter middleware.

**Architecture:** rig 0.47 split (agent decision Q5, research in scratchpad `framework-refs.md` sections 1 and 11). `wire::encode` turns a `CompletionRequest` into JSON with no I/O. `wire::decode_body` (unary) and `wire::Decoder` (one per streamed reply) turn wire JSON into `Delta`s, and one `Fold` turns deltas into a `CompletionResponse`, so unary and streaming cannot drift. `Endpoint::request` adds URL and auth headers. `HttpModel` sends through `Arc<dyn Transport>` (reqwest by default, `ScriptedTransport` in tests). Vendors are data in `troupekit/catalog/providers.toml`: protocol, base URL, auth style, quirks, models and prices. Middleware are `Model` wrappers, AutoAgents `LLMLayer` style.

**Tech Stack:** Rust 2021, troupekit-core, serde, serde_json, thiserror 2, async-trait, tokio (time), toml 1, reqwest 0.13 (rustls, optional default feature).

## Global Constraints

- License `MIT OR Apache-2.0`. Crate `troupekit-providers`. Zero references to the host app anywhere in `troupekit/` (`troupekit/scripts/check-boundary.sh`).
- No code comments; docstrings only (`///`, `//!`), directly above a declaration, 3 to 5 lines. No em dashes. The TOML catalog and JSON goldens carry no comments either.
- `assert!` for internal invariants; input validation returns `Err`.
- One copy of every helper. Reuse `troupekit_core::{message::Message, event::Usage, secret::Secret, error::ProviderError, tool::ToolSpec}`; never redefine them. Error constructors `bad_request` and `decode_error` live once in `status.rs`.
- Never a real API key anywhere. Tests use `ScriptedTransport` or a loopback TCP server only.
- Tests run in Kiri from the repo root: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit` then `kiri run --rm troupekit-test-p3` (prefix `HOME=/Users/chaoticqubit` when `HOME` differs).
- Branch `feat/troupekit-providers` in its own worktree. Conventional commits, no `Co-Authored-By` trailer. Commit, never push. Touch only `troupekit/` and this plan.
- Workspace additions to `troupekit/Cargo.toml` stay in one group of three lines (`troupekit-providers`, `reqwest`, `toml`), matching pod 4's `reqwest`/`toml` lines so the lockfile holds one version.

## Decisions (pod 3, 2026-11-21)

| # | Topic | Decision | Reason |
|---|---|---|---|
| P1 | `ToolSpec` | Lives in `troupekit-core` (`tool.rs`), added as isolated commit `7f113e7`; pod 4 cherry-picked the same change. | Providers and tools both need it; neither may depend on the other. |
| P2 | Stream API | `Model::stream(req, on_delta: &mut (dyn for<'d> FnMut(&'d Delta) + Send)) -> Result<CompletionResponse>`. | Object safe, no `futures` dependency, and the caller still gets the folded response. |
| P3 | Deltas | Own `Delta` enum (`text_delta`, `tool_call_started`, `tool_call_args`, `usage`, `stop`), names aligned with core `EventBody`. | `EventBody` has no tool-argument fragments or usage; the agent pod maps `Delta::TextDelta` to `EventBody::TextDelta`. |
| P4 | One decoder path | Unary bodies decode to the same deltas a stream yields; both go through one `Fold`. Tests assert unary and streamed goldens fold to the same response per protocol. | rig "one fold, one constructor". |
| P5 | `CompletionRequest.model` | `Option<String>`; `HttpModel` rejects a value that differs from its connected model. | Keeps the requested field while a request can never be priced at another model's rates. |
| P6 | Key | `connect_with(selector, Option<Secret>, Arc<dyn Transport>)`; `connect(selector, Option<Secret>)` behind feature `reqwest`. | `Secret` cannot be blank and ollama/lmstudio need no key; offline tests need an injected transport. |
| P7 | reqwest | Default feature `reqwest`, rustls, `default-features = false`. | `cargo test --workspace` then compiles and tests it; hosts with their own transport turn it off. |
| P8 | Catalog file | `troupekit/catalog/providers.toml`, loaded with `Catalog::load(&str)`; library code never `include_str!`s it. | A path outside the crate breaks `cargo publish`; tests may read it. |
| P9 | Prices | `u64` usd micros per million tokens, `price_verified` per model, optional `long_context` tier, cost rounded up with `u128` math. Anthropic prices verified against the claude-api skill model table (cached 2026-11-03); every other vendor 0 with `price_verified = false`; local vendors 0 and verified. | Budgets (pod 5) must never under-count or read an unverified zero as free. |
| P10 | Security | `HttpRequest` Debug prints header names only; keys go in headers, never URLs; reqwest errors are stripped of URLs; a catalog vendor with a key over plain `http://` is rejected; model ids are restricted to `[A-Za-z0-9._:/-]` so they cannot inject into a URL. | Engineering priority 1. |
| P11 | Anthropic | `anthropic-version: 2026-10-21`; vendor quirk `temperature = false`; structured output via `output_config.format`; thinking blocks and unknown deltas are skipped; input tokens include cache read and write tokens. | Current Claude models reject sampling params on this version; all three catalog models think by default. |
| P12 | Gemini | Header `x-goog-api-key`; `parametersJsonSchema` and `responseJsonSchema`; missing function-call ids synthesized as `call_<n>` per reply; a tool result looks up its function name from the latest earlier tool call with that id, else `BadRequest`. | Gemini's `functionResponse` needs the name, core `ToolResult` has only the call id. |
| P13 | OpenAI quirks as data | `max_tokens_field`, `stream_usage`, `output_schema` (`json_schema`, `json_object`, `none`; `none` omits the schema, typed output is then validated by the agent), `temperature`. Values from rig `dialects.rs`. | Q5 "vendor dialects as data". |
| P14 | Retry | Retryable kinds only, `max_attempts` 1 to 10, exponential from `base_delay_ms` capped at `max_delay_ms`, server `retry_after_ms` honoured when at most `max_delay_ms` else the error is returned; injected `Sleep`. Streams retry and fall back only before the first delta. | Never replay half a stream to a UI. |
| P15 | Status map | 401/403 Auth, 408 Stalled, 429 RateLimited, other 4xx BadRequest, everything else Server; `retry-after-ms` then `retry-after` seconds on any status; reqwest timeouts Stalled, other transport errors Network; bad JSON Decode; a stream that ends without a stop reason Network (retryable). | Leader brief plus truncation is transient. |

---

## File Structure

| Path | Responsibility |
|---|---|
| `troupekit/Cargo.toml` | add `troupekit-providers`, `reqwest`, `toml` to `[workspace.dependencies]` |
| `troupekit/catalog/providers.toml` | provider catalog data |
| `troupekit/crates/troupekit-providers/Cargo.toml` | crate manifest, feature `reqwest` |
| `.../src/lib.rs` | modules and re-exports |
| `.../src/status.rs` | `bad_request`, `decode_error`, `kind_for_status`, `retry_after_ms`, `error_detail`, `status_error` |
| `.../src/request.rs` | `CompletionRequest`, `CompletionResponse`, `StopReason`, `Delta` |
| `.../src/fold.rs` | `Fold`: deltas to one `CompletionResponse` |
| `.../src/sse.rs` | `SseParser`, `SseEvent` |
| `.../src/transport.rs` | `HttpRequest`, `HttpResponse`, `Body`, `Transport`, `read_body`, `Scripted`, `ScriptedTransport` |
| `.../src/reqwest_transport.rs` | `ReqwestTransport` (feature `reqwest`) |
| `.../src/wire/mod.rs` | `Protocol`, `AuthStyle`, `Quirks`, `encode`, `decode_body`, `Decoder`, shared JSON helpers |
| `.../src/wire/anthropic.rs` | Anthropic Messages |
| `.../src/wire/openai.rs` | OpenAI Chat Completions and OpenAI-compatible |
| `.../src/wire/gemini.rs` | Gemini generateContent |
| `.../src/model.rs` | `Model`, `Endpoint`, `HttpModel` |
| `.../src/catalog.rs` | `Catalog`, `VendorSpec`, `ModelSpec`, `Price`, `LongContextPrice`, `CatalogError` |
| `.../src/middleware.rs` | `Sleep`, `TokioSleep`, `RetryPolicy`, `RetryModel`, `FallbackModel`, `MeteredModel` |
| `.../tests/common/mod.rs` | shared fixtures: rich request, goldens, scripted model helpers |
| `.../tests/golden/*` | request goldens (JSON) and reply fixtures (JSON, SSE) |
| `.../tests/*.rs` | one test file per task |
| `troupekit/README.md` | crate map line |

Every source and test file below is written exactly as shown. A `**File:**` line names the path of the block after it.

---
### Task 1: Crate scaffold, error helpers, request types and Fold

**Files:**
- Modify: `troupekit/Cargo.toml`
- Create: `troupekit/crates/troupekit-providers/Cargo.toml`, `src/lib.rs`, `src/status.rs`, `src/request.rs`, `src/fold.rs`, `tests/status.rs`, `tests/request.rs`, `tests/fold.rs`

**Interfaces:**
- `status::{bad_request(msg), decode_error(msg)} -> ProviderError`; `kind_for_status(u16) -> ProviderErrorKind`; `retry_after_ms(&[(String, String)]) -> Option<u64>`; `error_detail(&[u8]) -> String`; `status_error(u16, &[(String, String)], &[u8]) -> ProviderError`.
- `request::CompletionRequest { model: Option<String>, system: Option<String>, messages: Vec<Message>, tools: Vec<ToolSpec>, max_tokens: u32, temperature: Option<f64>, output_schema: Option<Value> }` with `user(text, max_tokens)`, `validate() -> Result<(), ProviderError>`, `system_text() -> Option<String>`.
- `request::CompletionResponse { message: Message, usage: Usage, stop: StopReason }`; `StopReason { EndTurn, ToolUse, MaxTokens, Refusal, Other(String) }`.
- `request::Delta { TextDelta { text }, ToolCallStarted { index: u32, id, name }, ToolCallArgs { index: u32, json }, Usage { input_tokens: Option<u64>, output_tokens: Option<u64> }, Stop { reason } }`.
- `fold::Fold { new(), push(&Delta) -> Result<(), ProviderError>, finish(self) -> Result<CompletionResponse, ProviderError> }`.

- [ ] **Step 1: Workspace dependencies**

Append to `[workspace.dependencies]` in `troupekit/Cargo.toml`, as one group. No `troupekit-providers` entry yet: cargo warns on an unused workspace dependency, so the first consumer (pod 5) adds it.

```toml
reqwest = { version = "0.13", default-features = false, features = ["rustls", "http2"] }
toml = "1"
```

- [ ] **Step 2: Crate manifest and module list**

**File:** `troupekit/crates/troupekit-providers/Cargo.toml`

```toml
[package]
name = "troupekit-providers"
description = "Model providers for the Troupekit agent framework: wire protocols, catalog, transports and middleware."
edition.workspace = true
version.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[features]
default = ["reqwest"]
reqwest = ["dep:reqwest"]

[dependencies]
troupekit-core.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
async-trait.workspace = true
tokio = { workspace = true, features = ["time"] }
toml.workspace = true
reqwest = { workspace = true, optional = true }

[dev-dependencies]
tokio = { workspace = true, features = ["rt-multi-thread", "macros", "time"] }
```

`src/lib.rs` starts with the docstring and the modules of this task; later tasks add one `pub mod` line each (Task 9 writes the final file):

```rust
//! Model providers for Troupekit: wire protocols, a provider catalog,
//! injected transports and middleware, behind one `Model` trait.

pub mod fold;
pub mod request;
pub mod status;
```

- [ ] **Step 3: Write the failing tests**

**File:** `troupekit/crates/troupekit-providers/tests/status.rs`

```rust
use troupekit_core::error::ProviderErrorKind;
use troupekit_providers::status::{error_detail, kind_for_status, retry_after_ms, status_error};

fn h(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn maps_statuses_to_kinds() {
    use ProviderErrorKind::*;
    let table = [
        (401, Auth), (403, Auth), (408, Stalled), (429, RateLimited), (400, BadRequest), (404, BadRequest),
        (422, BadRequest), (500, Server), (503, Server), (529, Server), (302, Server),
    ];
    for (status, kind) in table {
        assert_eq!(kind_for_status(status), kind, "status {status}");
    }
}

#[test]
fn reads_retry_after_headers() {
    assert_eq!(retry_after_ms(&h(&[("Retry-After", "3")])), Some(3000));
    assert_eq!(retry_after_ms(&h(&[("retry-after", "1.5")])), Some(1500));
    assert_eq!(retry_after_ms(&h(&[("retry-after", "3"), ("retry-after-ms", "250")])), Some(250));
    assert_eq!(retry_after_ms(&h(&[("retry-after", "Wed, 21 Oct 2026 07:28:00 GMT")])), None);
    assert_eq!(retry_after_ms(&h(&[("retry-after", "-1")])), None);
    assert_eq!(retry_after_ms(&[]), None);
}

#[test]
fn extracts_error_detail() {
    assert_eq!(error_detail(br#"{"error":{"type":"x","message":"bad key"}}"#), "bad key");
    assert_eq!(error_detail(br#"{"message":"quota"}"#), "quota");
    assert_eq!(error_detail(b"  plain text  "), "plain text");
    assert_eq!(error_detail(b""), "empty body");
    assert_eq!(error_detail("\u{e9}".repeat(400).as_bytes()).chars().count(), 300);
}

#[test]
fn builds_status_errors() {
    let e = status_error(429, &h(&[("retry-after", "2")]), br#"{"error":{"message":"slow down"}}"#);
    assert_eq!(e.kind, ProviderErrorKind::RateLimited);
    assert_eq!(e.retry_after_ms, Some(2000));
    assert_eq!(e.message, "HTTP 429: slow down");
    let e = status_error(401, &[], b"nope");
    assert_eq!((e.kind, e.retry_after_ms), (ProviderErrorKind::Auth, None));
}
```

**File:** `troupekit/crates/troupekit-providers/tests/request.rs`

```rust
use serde_json::json;
use troupekit_core::error::ProviderErrorKind;
use troupekit_core::message::{Message, Role};
use troupekit_core::tool::ToolSpec;
use troupekit_providers::request::{CompletionRequest, Delta, StopReason};

fn tool(name: &str) -> ToolSpec {
    ToolSpec { name: name.into(), description: "d".into(), schema: json!({"type":"object"}) }
}

#[test]
fn validates_requests() {
    assert!(CompletionRequest::user("hi", 10).validate().is_ok());
    let cases: Vec<Box<dyn Fn(&mut CompletionRequest)>> = vec![
        Box::new(|r| r.messages.clear()),
        Box::new(|r| r.max_tokens = 0),
        Box::new(|r| r.temperature = Some(2.5)),
        Box::new(|r| r.temperature = Some(f64::NAN)),
        Box::new(|r| r.tools = vec![tool(" ")]),
        Box::new(|r| r.tools = vec![tool("a"), tool("a")]),
    ];
    for (i, change) in cases.iter().enumerate() {
        let mut r = CompletionRequest::user("hi", 10);
        change(&mut r);
        assert_eq!(r.validate().unwrap_err().kind, ProviderErrorKind::BadRequest, "case {i}");
    }
}

#[test]
fn system_text_joins_field_and_system_messages() {
    let mut r = CompletionRequest::user("hi", 10);
    assert_eq!(r.system_text(), None);
    r.system = Some("Be brief.".into());
    r.messages.insert(0, Message::text(Role::System, "Use metric units."));
    assert_eq!(r.system_text().as_deref(), Some("Be brief.\n\nUse metric units."));
}

#[test]
fn deltas_and_stops_serialize_tagged() {
    assert_eq!(serde_json::to_value(Delta::TextDelta { text: "a".into() }).unwrap(), json!({"type":"text_delta","text":"a"}));
    assert_eq!(serde_json::to_value(StopReason::ToolUse).unwrap(), json!("tool_use"));
    assert_eq!(serde_json::to_value(StopReason::Other("pause_turn".into())).unwrap(), json!({"other":"pause_turn"}));
}
```

**File:** `troupekit/crates/troupekit-providers/tests/fold.rs`

```rust
use serde_json::json;
use troupekit_core::error::{ProviderError, ProviderErrorKind};
use troupekit_core::event::Usage;
use troupekit_core::message::{ContentPart, Role};
use troupekit_providers::fold::Fold;
use troupekit_providers::request::{CompletionResponse, Delta, StopReason};

fn fold(deltas: &[Delta]) -> Result<CompletionResponse, ProviderError> {
    let mut f = Fold::new();
    for d in deltas {
        f.push(d)?;
    }
    f.finish()
}

fn text(t: &str) -> Delta {
    Delta::TextDelta { text: t.into() }
}

fn stop(reason: StopReason) -> Delta {
    Delta::Stop { reason }
}

fn started(index: u32, id: &str, name: &str) -> Delta {
    Delta::ToolCallStarted { index, id: id.into(), name: name.into() }
}

fn args(index: u32, json: &str) -> Delta {
    Delta::ToolCallArgs { index, json: json.into() }
}

#[test]
fn joins_text_and_keeps_part_order() {
    let r = fold(&[
        text("Mon: "),
        text("run"),
        started(4, "c1", "calendar"),
        args(4, "{\"days\":"),
        args(4, "3}"),
        started(7, "c2", "now"),
        Delta::Usage { input_tokens: Some(5), output_tokens: Some(1) },
        Delta::Usage { input_tokens: None, output_tokens: Some(9) },
        stop(StopReason::EndTurn),
    ])
    .unwrap();
    assert_eq!(r.message.role, Role::Assistant);
    assert_eq!(r.message.parts, vec![
        ContentPart::Text { text: "Mon: run".into() },
        ContentPart::ToolCall { id: "c1".into(), name: "calendar".into(), arguments: json!({"days":3}) },
        ContentPart::ToolCall { id: "c2".into(), name: "now".into(), arguments: json!({}) },
    ]);
    assert_eq!(r.usage, Usage { input_tokens: 5, output_tokens: 9, cost_usd_micros: 0 });
    assert_eq!(r.stop, StopReason::ToolUse);
}

#[test]
fn text_after_a_tool_call_starts_a_new_part() {
    let r = fold(&[text("a"), started(0, "c", "n"), text("b"), stop(StopReason::ToolUse)]).unwrap();
    assert_eq!(r.message.parts.len(), 3);
    assert_eq!(r.message.text_content(), "a\nb");
}

#[test]
fn empty_reply_is_one_empty_text_part() {
    let r = fold(&[stop(StopReason::MaxTokens)]).unwrap();
    assert_eq!(r.message.parts, vec![ContentPart::Text { text: String::new() }]);
    assert_eq!(r.stop, StopReason::MaxTokens);
}

#[test]
fn rejects_broken_delta_sequences() {
    let kind = |d: &[Delta]| fold(d).unwrap_err().kind;
    assert_eq!(kind(&[text("x")]), ProviderErrorKind::Network);
    assert_eq!(kind(&[args(1, "{}"), stop(StopReason::EndTurn)]), ProviderErrorKind::Decode);
    assert_eq!(kind(&[started(0, "c", "n"), started(0, "c", "n"), stop(StopReason::EndTurn)]), ProviderErrorKind::Decode);
    assert_eq!(kind(&[started(0, "c", "n"), args(0, "{nope"), stop(StopReason::EndTurn)]), ProviderErrorKind::Decode);
    assert_eq!(kind(&[started(0, "", "n"), stop(StopReason::EndTurn)]), ProviderErrorKind::Decode);
}
```

- [ ] **Step 4: Run, expect failure**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit`
Expected: build fails, `unresolved import troupekit_providers::status` (modules empty).

- [ ] **Step 5: Implement**

**File:** `troupekit/crates/troupekit-providers/src/status.rs`

```rust
use serde_json::Value;
use troupekit_core::error::{ProviderError, ProviderErrorKind};

const MAX_DETAIL_CHARS: usize = 300;

/// Builds a `BadRequest` error with `message`: the request cannot be sent
/// as given. Used by request validation and every protocol encoder.
/// Returns the error.
pub fn bad_request(message: impl Into<String>) -> ProviderError {
    ProviderError::new(ProviderErrorKind::BadRequest, message)
}

/// Builds a `Decode` error with `message`: a reply did not have the shape
/// its protocol promises. Used by every decoder and the SSE parser.
/// Returns the error.
pub fn decode_error(message: impl Into<String>) -> ProviderError {
    ProviderError::new(ProviderErrorKind::Decode, message)
}

/// Maps an HTTP `status` to the kind of failure it means.
/// 401 and 403 are auth, 408 a stall, 429 a rate limit, other 4xx a bad
/// request, and every other status a server error.
pub fn kind_for_status(status: u16) -> ProviderErrorKind {
    match status {
        401 | 403 => ProviderErrorKind::Auth,
        408 => ProviderErrorKind::Stalled,
        429 => ProviderErrorKind::RateLimited,
        400..=499 => ProviderErrorKind::BadRequest,
        _ => ProviderErrorKind::Server,
    }
}

/// Reads the retry delay a provider sent, in milliseconds.
/// `retry-after-ms` wins over `retry-after`, which counts seconds and may
/// be fractional. Names match case-insensitively; dates give `None`.
pub fn retry_after_ms(headers: &[(String, String)]) -> Option<u64> {
    let find = |name: &str| headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.trim());
    if let Some(ms) = find("retry-after-ms").and_then(|v| v.parse::<u64>().ok()) {
        return Some(ms);
    }
    let secs: f64 = find("retry-after")?.parse().ok()?;
    if !secs.is_finite() || secs < 0.0 {
        return None;
    }
    Some((secs * 1000.0).ceil().min(u64::MAX as f64) as u64)
}

/// Pulls a short readable message out of an error reply `body`.
/// Prefers JSON `error.message`, then `message`, then the raw text, cut
/// to 300 characters. Returns `"empty body"` when there is nothing.
pub fn error_detail(body: &[u8]) -> String {
    if let Ok(v) = serde_json::from_slice::<Value>(body) {
        if let Some(m) = v.pointer("/error/message").or_else(|| v.get("message")).and_then(Value::as_str) {
            return cut(m);
        }
    }
    let text = String::from_utf8_lossy(body);
    let text = text.trim();
    if text.is_empty() {
        "empty body".into()
    } else {
        cut(text)
    }
}

/// Builds the error for a non-success HTTP reply from its `status`,
/// `headers` and `body`: kind from the status, message from the body,
/// retry delay from the headers.
pub fn status_error(status: u16, headers: &[(String, String)], body: &[u8]) -> ProviderError {
    let err = ProviderError::new(kind_for_status(status), format!("HTTP {status}: {}", error_detail(body)));
    match retry_after_ms(headers) {
        Some(ms) => err.with_retry_after(ms),
        None => err,
    }
}

fn cut(text: &str) -> String {
    text.chars().take(MAX_DETAIL_CHARS).collect()
}
```

**File:** `troupekit/crates/troupekit-providers/src/request.rs`

```rust
use crate::status::bad_request;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use troupekit_core::error::ProviderError;
use troupekit_core::event::Usage;
use troupekit_core::message::{Message, Role};
use troupekit_core::tool::ToolSpec;

/// One call to a chat model: the conversation, the tools it may call and
/// the output limits. `model` is the wire model id the caller expects,
/// or `None` for the model the handle was connected to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: Option<String>,
    pub system: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
    pub temperature: Option<f64>,
    pub output_schema: Option<Value>,
}

impl CompletionRequest {
    /// Builds a request holding one user message with `text`, capped at
    /// `max_tokens` output tokens. Every other field starts empty.
    /// Returns the new request.
    pub fn user(text: impl Into<String>, max_tokens: u32) -> Self {
        Self {
            model: None,
            system: None,
            messages: vec![Message::text(Role::User, text)],
            tools: Vec::new(),
            max_tokens,
            temperature: None,
            output_schema: None,
        }
    }

    /// Checks the request before any protocol encodes it.
    /// Returns `BadRequest` for no messages, a zero token cap, a temperature
    /// outside 0 to 2, or a blank or repeated tool name.
    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.messages.is_empty() {
            return Err(bad_request("request has no messages"));
        }
        if self.max_tokens == 0 {
            return Err(bad_request("max_tokens must be above zero"));
        }
        if let Some(t) = self.temperature {
            if !(0.0..=2.0).contains(&t) {
                return Err(bad_request("temperature must be between 0 and 2"));
            }
        }
        let mut names = HashSet::new();
        for tool in &self.tools {
            if tool.name.trim().is_empty() {
                return Err(bad_request("tool name is blank"));
            }
            if !names.insert(tool.name.as_str()) {
                return Err(bad_request(format!("tool name {} is repeated", tool.name)));
            }
        }
        Ok(())
    }

    /// Joins `system` and the text of every system-role message, in order,
    /// with blank lines between them. Every protocol sends this one text.
    /// Returns `None` when there is no system text.
    pub fn system_text(&self) -> Option<String> {
        let parts: Vec<String> = self
            .system
            .iter()
            .cloned()
            .chain(self.messages.iter().filter(|m| m.role == Role::System).map(Message::text_content))
            .filter(|s| !s.is_empty())
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n\n"))
        }
    }
}

/// Why a model stopped producing output.
/// Vendor reasons are normalized; unknown ones are kept as `Other`.
/// Serializes in snake case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Refusal,
    Other(String),
}

/// The finished reply to one `CompletionRequest`: the assistant message,
/// token usage (cost filled in by the cost meter) and the stop reason.
/// Unary and streamed calls return the same value for the same reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub message: Message,
    pub usage: Usage,
    pub stop: StopReason,
}

/// One piece of a reply as it arrives. Tool calls are keyed by `index`
/// and their JSON arguments arrive as fragments; usage values are running
/// totals. Serializes with a `type` tag in snake case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Delta {
    TextDelta { text: String },
    ToolCallStarted { index: u32, id: String, name: String },
    ToolCallArgs { index: u32, json: String },
    Usage { input_tokens: Option<u64>, output_tokens: Option<u64> },
    Stop { reason: StopReason },
}
```

**File:** `troupekit/crates/troupekit-providers/src/fold.rs`

```rust
use crate::request::{CompletionResponse, Delta, StopReason};
use crate::status::decode_error;
use serde_json::Value;
use std::collections::BTreeMap;
use troupekit_core::error::{ProviderError, ProviderErrorKind};
use troupekit_core::event::Usage;
use troupekit_core::message::{ContentPart, Message, Role};

/// Builds one `CompletionResponse` from the deltas of one reply.
/// Unary and streamed replies both go through it, so they cannot drift.
/// Feed deltas in order with `push`, then call `finish` once.
#[derive(Debug, Default)]
pub struct Fold {
    parts: Vec<ContentPart>,
    calls: BTreeMap<u32, (usize, String)>,
    input_tokens: u64,
    output_tokens: u64,
    stop: Option<StopReason>,
}

impl Fold {
    /// Starts an empty fold for one reply.
    /// Returns the fold.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one `delta`: text joins the previous text part, argument
    /// fragments join their call, usage overwrites the totals so far.
    /// Returns `Decode` for a call started twice or arguments for no call.
    pub fn push(&mut self, delta: &Delta) -> Result<(), ProviderError> {
        match delta {
            Delta::TextDelta { text } => {
                if text.is_empty() {
                    return Ok(());
                }
                match self.parts.last_mut() {
                    Some(ContentPart::Text { text: last }) => last.push_str(text),
                    _ => self.parts.push(ContentPart::Text { text: text.clone() }),
                }
            }
            Delta::ToolCallStarted { index, id, name } => {
                if id.is_empty() || name.is_empty() {
                    return Err(decode_error("tool call without an id or a name"));
                }
                if self.calls.contains_key(index) {
                    return Err(decode_error(format!("tool call {index} started twice")));
                }
                self.calls.insert(*index, (self.parts.len(), String::new()));
                self.parts.push(ContentPart::ToolCall { id: id.clone(), name: name.clone(), arguments: Value::Null });
            }
            Delta::ToolCallArgs { index, json } => match self.calls.get_mut(index) {
                Some((_, raw)) => raw.push_str(json),
                None => return Err(decode_error(format!("arguments for unknown tool call {index}"))),
            },
            Delta::Usage { input_tokens, output_tokens } => {
                if let Some(n) = input_tokens {
                    self.input_tokens = *n;
                }
                if let Some(n) = output_tokens {
                    self.output_tokens = *n;
                }
            }
            Delta::Stop { reason } => self.stop = Some(reason.clone()),
        }
        Ok(())
    }

    /// Ends the reply and builds the response. Tool arguments are parsed
    /// as JSON (empty means `{}`), and an end-turn stop with tool calls
    /// becomes `ToolUse`. Returns `Network` when no stop reason arrived.
    pub fn finish(self) -> Result<CompletionResponse, ProviderError> {
        let Fold { mut parts, calls, input_tokens, output_tokens, stop } = self;
        let Some(mut stop) = stop else {
            return Err(ProviderError::new(ProviderErrorKind::Network, "reply ended before a stop reason"));
        };
        for (pos, raw) in calls.values() {
            let parsed = if raw.trim().is_empty() {
                Value::Object(Default::default())
            } else {
                serde_json::from_str(raw).map_err(|e| decode_error(format!("tool arguments are not JSON: {e}")))?
            };
            match &mut parts[*pos] {
                ContentPart::ToolCall { arguments, .. } => *arguments = parsed,
                _ => unreachable!("a call slot always points at its tool call part"),
            }
        }
        if !calls.is_empty() && stop == StopReason::EndTurn {
            stop = StopReason::ToolUse;
        }
        if parts.is_empty() {
            parts.push(ContentPart::Text { text: String::new() });
        }
        let message = Message::new(Role::Assistant, parts).map_err(|e| decode_error(e.to_string()))?;
        Ok(CompletionResponse { message, usage: Usage { input_tokens, output_tokens, cost_usd_micros: 0 }, stop })
    }
}
```

- [ ] **Step 6: Run, expect pass**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit && kiri run --rm troupekit-test-p3`
Expected: `boundary ok`, all `status`, `request`, `fold` tests pass.

- [ ] **Step 7: Commit**

`git add troupekit && git commit -m "feat(troupekit-providers): request types, error helpers and fold"`

---
### Task 2: SSE framer and transports

**Files:**
- Create: `src/sse.rs`, `src/transport.rs`, `src/reqwest_transport.rs`, `tests/sse.rs`, `tests/transport.rs`, `tests/reqwest_transport.rs`
- Modify: `src/lib.rs` (add `pub mod sse;`, `pub mod transport;`, and `#[cfg(feature = "reqwest")] pub mod reqwest_transport;`)

**Interfaces:**
- `sse::SseEvent { event: Option<String>, data: String }`; `sse::SseParser { new(), push(&[u8]) -> Result<Vec<SseEvent>, ProviderError>, finish(self) -> Result<Vec<SseEvent>, ProviderError> }`; `sse::MAX_LINE_BYTES`.
- `transport::HttpRequest { url, headers: Vec<(String, String)>, body: Vec<u8> }` with `header(name) -> Option<&str>` and a redacting `Debug`.
- `transport::Body` (async `chunk() -> Result<Option<Vec<u8>>, ProviderError>`), `transport::HttpResponse { status: u16, headers, body: Box<dyn Body> }`, `transport::Transport` (async `send(HttpRequest) -> Result<HttpResponse, ProviderError>`), both object safe through `async_trait`.
- `transport::read_body(&mut dyn Body, limit) -> Result<Vec<u8>, ProviderError>`.
- `transport::Scripted { Reply { status, headers, chunks }, Fail(ProviderError) }` with `body(status, bytes)`, `chunked(text, size)`, `with_header(name, value)`; `transport::ScriptedTransport { new(Vec<Scripted>), sent() -> Vec<HttpRequest> }`.
- `reqwest_transport::ReqwestTransport { new(), with_timeouts(connect, read), from_client(reqwest::Client) }`.

- [ ] **Step 1: Write the failing tests**

**File:** `troupekit/crates/troupekit-providers/tests/sse.rs`

```rust
use troupekit_core::error::ProviderErrorKind;
use troupekit_providers::sse::{SseEvent, SseParser, MAX_LINE_BYTES};

fn ev(event: Option<&str>, data: &str) -> SseEvent {
    SseEvent { event: event.map(String::from), data: data.into() }
}

fn parse_in_chunks(bytes: &[u8], size: usize) -> Vec<SseEvent> {
    let mut parser = SseParser::new();
    let mut out = Vec::new();
    for chunk in bytes.chunks(size) {
        out.extend(parser.push(chunk).unwrap());
    }
    out.extend(parser.finish().unwrap());
    out
}

#[test]
fn same_events_for_every_chunk_size() {
    let text = "event: a\ndata: {\"t\":\"h\u{e9}llo \u{1f600}\"}\n\n: comment\n\ndata: x\r\ndata: y\r\n\r\nevent: ping\n\ndata:tight\n\n";
    let want = vec![ev(Some("a"), "{\"t\":\"h\u{e9}llo \u{1f600}\"}"), ev(None, "x\ny"), ev(None, "tight")];
    for size in 1..=text.len() {
        assert_eq!(parse_in_chunks(text.as_bytes(), size), want, "chunk size {size}");
    }
}

#[test]
fn finish_flushes_an_unterminated_event() {
    assert_eq!(parse_in_chunks(b"data: last", 3), vec![ev(None, "last")]);
}

#[test]
fn rejects_invalid_utf8_and_huge_lines() {
    let mut parser = SseParser::new();
    assert_eq!(parser.push(b"data: \xff\n").unwrap_err().kind, ProviderErrorKind::Decode);
    let mut parser = SseParser::new();
    assert_eq!(parser.push(&vec![b'a'; MAX_LINE_BYTES + 1]).unwrap_err().kind, ProviderErrorKind::Decode);
}
```

**File:** `troupekit/crates/troupekit-providers/tests/transport.rs`

```rust
use troupekit_core::error::{ProviderError, ProviderErrorKind};
use troupekit_providers::transport::{read_body, HttpRequest, Scripted, ScriptedTransport, Transport};

fn req(url: &str) -> HttpRequest {
    HttpRequest {
        url: url.into(),
        headers: vec![("Authorization".into(), "Bearer sk-test-not-real".into())],
        body: b"{}".to_vec(),
    }
}

#[tokio::test]
async fn replays_in_order_and_records_requests() {
    let t = ScriptedTransport::new(vec![
        Scripted::chunked("abcdefg", 3).with_header("x-id", "1"),
        Scripted::Fail(ProviderError::new(ProviderErrorKind::Network, "down")),
    ]);
    let mut reply = t.send(req("https://a.test/1")).await.unwrap();
    assert_eq!(reply.status, 200);
    assert_eq!(reply.headers, vec![("x-id".to_string(), "1".to_string())]);
    assert_eq!(reply.body.chunk().await.unwrap(), Some(b"abc".to_vec()));
    assert_eq!(read_body(reply.body.as_mut(), 100).await.unwrap(), b"defg");
    assert_eq!(t.send(req("https://a.test/2")).await.err().unwrap().kind, ProviderErrorKind::Network);
    assert_eq!(t.send(req("https://a.test/3")).await.err().unwrap().message, "script has no more replies");
    let urls: Vec<String> = t.sent().into_iter().map(|r| r.url).collect();
    assert_eq!(urls, ["https://a.test/1", "https://a.test/2", "https://a.test/3"]);
}

#[tokio::test]
async fn read_body_enforces_its_limit() {
    let t = ScriptedTransport::new(vec![Scripted::chunked("0123456789", 4)]);
    let mut reply = t.send(req("https://a.test")).await.unwrap();
    assert_eq!(read_body(reply.body.as_mut(), 9).await.unwrap_err().kind, ProviderErrorKind::Decode);
}

#[test]
fn debug_never_shows_header_values() {
    let shown = format!("{:?}", req("https://a.test"));
    assert!(shown.contains("Authorization"));
    assert!(!shown.contains("sk-test-not-real"));
}

#[test]
fn header_lookup_ignores_case() {
    assert_eq!(req("u").header("authorization"), Some("Bearer sk-test-not-real"));
    assert_eq!(req("u").header("x-missing"), None);
}
```

**File:** `troupekit/crates/troupekit-providers/tests/reqwest_transport.rs`

```rust
#![cfg(feature = "reqwest")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;
use troupekit_core::error::ProviderErrorKind;
use troupekit_providers::reqwest_transport::ReqwestTransport;
use troupekit_providers::transport::{read_body, HttpRequest, Transport};

fn serve_once(reply: &'static str, delay: Duration) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1/messages", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut seen = Vec::new();
        let mut buf = [0u8; 4096];
        while !request_complete(&seen) {
            let n = socket.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            seen.extend_from_slice(&buf[..n]);
        }
        thread::sleep(delay);
        let _ = socket.write_all(reply.as_bytes());
        String::from_utf8_lossy(&seen).into_owned()
    });
    (url, handle)
}

fn request_complete(seen: &[u8]) -> bool {
    let text = String::from_utf8_lossy(seen);
    let Some((head, body)) = text.split_once("\r\n\r\n") else {
        return false;
    };
    let len = head
        .lines()
        .find_map(|line| {
            let line = line.to_ascii_lowercase();
            line.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))
        })
        .unwrap_or(0);
    body.len() >= len
}

fn post(url: &str) -> HttpRequest {
    HttpRequest {
        url: url.into(),
        headers: vec![("content-type".into(), "application/json".into()), ("x-api-key".into(), "test-key".into())],
        body: br#"{"model":"m"}"#.to_vec(),
    }
}

#[tokio::test]
async fn sends_a_post_and_reads_status_headers_and_body() {
    let (url, server) = serve_once(
        "HTTP/1.1 429 Too Many Requests\r\nretry-after: 3\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}",
        Duration::ZERO,
    );
    let t = ReqwestTransport::new().unwrap();
    let mut reply = t.send(post(&url)).await.unwrap();
    assert_eq!(reply.status, 429);
    assert!(reply.headers.iter().any(|(k, v)| k == "retry-after" && v == "3"));
    assert_eq!(read_body(reply.body.as_mut(), 1024).await.unwrap(), b"{}");
    let seen = server.join().unwrap();
    assert!(seen.starts_with("POST /v1/messages HTTP/1.1\r\n"));
    assert!(seen.to_ascii_lowercase().contains("x-api-key: test-key"));
    assert!(seen.ends_with(r#"{"model":"m"}"#));
}

#[tokio::test]
async fn a_silent_server_is_a_stall() {
    let (url, _server) = serve_once("HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n", Duration::from_millis(1500));
    let t = ReqwestTransport::with_timeouts(Duration::from_secs(5), Duration::from_millis(200)).unwrap();
    assert_eq!(t.send(post(&url)).await.err().unwrap().kind, ProviderErrorKind::Stalled);
}

#[tokio::test]
async fn a_refused_connection_is_a_network_error() {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let t = ReqwestTransport::new().unwrap();
    let err = t.send(post(&format!("http://127.0.0.1:{port}/x"))).await.err().unwrap();
    assert_eq!(err.kind, ProviderErrorKind::Network);
}
```

- [ ] **Step 2: Run, expect failure**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit`
Expected: build fails, `could not find sse in troupekit_providers`.

- [ ] **Step 3: Implement**

**File:** `troupekit/crates/troupekit-providers/src/sse.rs`

```rust
use crate::status::decode_error;
use troupekit_core::error::ProviderError;

/// Longest line, in bytes, the parser buffers before failing with `Decode`.
/// Bounds memory when a server never sends a newline.
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

/// One server-sent event: its optional `event:` name and its `data:`
/// lines joined with newlines. Events without data are never produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// Splits a server-sent-events byte stream into events, for every
/// streaming protocol. Chunks may break anywhere, even inside a line or
/// a UTF-8 character; comments and unknown fields are skipped.
#[derive(Debug, Default)]
pub struct SseParser {
    buf: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

impl SseParser {
    /// Starts a parser for one stream.
    /// Returns the parser.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds the next `chunk` of the stream.
    /// Returns the events it completed, or `Decode` for a line that is
    /// not UTF-8 or grows past `MAX_LINE_BYTES`.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, ProviderError> {
        let mut out = Vec::new();
        let mut search = self.buf.len();
        self.buf.extend_from_slice(chunk);
        let mut start = 0;
        while let Some(offset) = self.buf[search..].iter().position(|b| *b == b'\n') {
            let end = search + offset;
            let line = line_text(&self.buf[start..end])?;
            self.line(&line, &mut out);
            start = end + 1;
            search = start;
        }
        self.buf.drain(..start);
        if self.buf.len() > MAX_LINE_BYTES {
            return Err(decode_error("event stream line is too long"));
        }
        Ok(out)
    }

    /// Ends the stream, completing a last event that had no blank line.
    /// Returns the remaining events, or `Decode` for a bad last line.
    pub fn finish(mut self) -> Result<Vec<SseEvent>, ProviderError> {
        let mut out = Vec::new();
        if !self.buf.is_empty() {
            let rest = std::mem::take(&mut self.buf);
            let line = line_text(&rest)?;
            self.line(&line, &mut out);
        }
        self.line("", &mut out);
        Ok(out)
    }

    fn line(&mut self, line: &str, out: &mut Vec<SseEvent>) {
        if line.is_empty() {
            if !self.data.is_empty() {
                out.push(SseEvent { event: self.event.take(), data: self.data.join("\n") });
                self.data.clear();
            }
            self.event = None;
            return;
        }
        if line.starts_with(':') {
            return;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "data" => self.data.push(value.to_owned()),
            "event" => self.event = Some(value.to_owned()),
            _ => {}
        }
    }
}

fn line_text(raw: &[u8]) -> Result<String, ProviderError> {
    let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
    std::str::from_utf8(raw).map(str::to_owned).map_err(|_| decode_error("event stream line is not UTF-8"))
}
```

**File:** `troupekit/crates/troupekit-providers/src/transport.rs`

```rust
use crate::status::decode_error;
use async_trait::async_trait;
use std::collections::VecDeque;
use std::fmt;
use std::sync::{Mutex, MutexGuard};
use troupekit_core::error::{ProviderError, ProviderErrorKind};

/// An HTTP POST to a provider: URL, headers and the JSON body bytes.
/// Headers may carry an API key, so `Debug` prints the URL, the header
/// names and the body length, never a header value.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    /// Looks up header `name`, ignoring ASCII case.
    /// Returns its value, or `None` when the request has no such header.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(k, _)| k.as_str()).collect();
        f.debug_struct("HttpRequest")
            .field("url", &self.url)
            .field("headers", &names)
            .field("body_len", &self.body.len())
            .finish()
    }
}

/// The body of an HTTP reply, read chunk by chunk so streams stay streams.
/// `chunk` yields the next bytes, then `None` once the body has ended.
#[async_trait]
pub trait Body: Send {
    async fn chunk(&mut self) -> Result<Option<Vec<u8>>, ProviderError>;
}

/// An HTTP reply: status code, headers and a body still to be read.
/// Header names keep the case the transport saw.
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Box<dyn Body>,
}

/// Sends provider requests. HTTP clients, proxies, in-process runtimes
/// and test fakes implement it, so models never own an HTTP stack.
/// Failures are `Network`, or `Stalled` for timeouts.
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ProviderError>;
}

/// Reads all of `body` into memory.
/// Returns the bytes, or `Decode` as soon as more than `limit` bytes
/// have arrived, so a hostile server cannot exhaust memory.
pub async fn read_body(body: &mut dyn Body, limit: usize) -> Result<Vec<u8>, ProviderError> {
    let mut out = Vec::new();
    while let Some(chunk) = body.chunk().await? {
        if out.len() + chunk.len() > limit {
            return Err(decode_error(format!("reply body is over {limit} bytes")));
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// One reply a `ScriptedTransport` gives back: a status with headers and
/// body chunks, or a transport failure.
/// Build replies with `body`, `chunked` and `with_header`.
#[derive(Debug, Clone)]
pub enum Scripted {
    Reply { status: u16, headers: Vec<(String, String)>, chunks: Vec<Vec<u8>> },
    Fail(ProviderError),
}

impl Scripted {
    /// A reply with `status` and the whole `body` in one chunk.
    /// Returns the scripted reply, without headers.
    pub fn body(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Scripted::Reply { status, headers: Vec::new(), chunks: vec![body.into()] }
    }

    /// A 200 reply whose `body` arrives in chunks of `size` bytes, the way
    /// a slow stream would. Chunks may split UTF-8 characters.
    /// Panics when `size` is zero.
    pub fn chunked(body: &str, size: usize) -> Self {
        assert!(size > 0, "chunk size must be above zero");
        let chunks = body.as_bytes().chunks(size).map(<[u8]>::to_vec).collect();
        Scripted::Reply { status: 200, headers: Vec::new(), chunks }
    }

    /// Adds header `name` with `value` to a reply; failures are unchanged.
    /// Returns the scripted reply.
    pub fn with_header(self, name: &str, value: &str) -> Self {
        match self {
            Scripted::Reply { status, mut headers, chunks } => {
                headers.push((name.to_owned(), value.to_owned()));
                Scripted::Reply { status, headers, chunks }
            }
            fail => fail,
        }
    }
}

struct ChunkBody(VecDeque<Vec<u8>>);

#[async_trait]
impl Body for ChunkBody {
    async fn chunk(&mut self) -> Result<Option<Vec<u8>>, ProviderError> {
        Ok(self.0.pop_front())
    }
}

/// A fake transport that replays scripted replies in order and records
/// every request sent to it, so tests never touch the network.
/// Once the script runs out, `send` fails with `Network`.
#[derive(Debug, Default)]
pub struct ScriptedTransport {
    replies: Mutex<VecDeque<Scripted>>,
    sent: Mutex<Vec<HttpRequest>>,
}

impl ScriptedTransport {
    /// Builds a transport that answers with `replies`, first to last.
    /// Returns the transport.
    pub fn new(replies: Vec<Scripted>) -> Self {
        Self { replies: Mutex::new(replies.into()), sent: Mutex::new(Vec::new()) }
    }

    /// Copies every request sent so far, oldest first.
    /// Returns the copies.
    pub fn sent(&self) -> Vec<HttpRequest> {
        lock(&self.sent).clone()
    }
}

#[async_trait]
impl Transport for ScriptedTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ProviderError> {
        lock(&self.sent).push(request);
        let next = lock(&self.replies).pop_front();
        match next {
            Some(Scripted::Reply { status, headers, chunks }) => {
                Ok(HttpResponse { status, headers, body: Box::new(ChunkBody(chunks.into())) })
            }
            Some(Scripted::Fail(err)) => Err(err),
            None => Err(ProviderError::new(ProviderErrorKind::Network, "script has no more replies")),
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
```

**File:** `troupekit/crates/troupekit-providers/src/reqwest_transport.rs`

```rust
use crate::transport::{Body, HttpRequest, HttpResponse, Transport};
use async_trait::async_trait;
use std::time::Duration;
use troupekit_core::error::{ProviderError, ProviderErrorKind};

/// The default HTTP transport, on reqwest with rustls.
/// A connect timeout and a per-read timeout turn a silent server into
/// `Stalled`; other failures are `Network`, with URLs stripped.
#[derive(Debug, Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// Builds a transport with a 30 s connect timeout and a 300 s read
    /// timeout, long enough for a slow first token.
    /// Returns `Network` when the HTTP client cannot start.
    pub fn new() -> Result<Self, ProviderError> {
        Self::with_timeouts(Duration::from_secs(30), Duration::from_secs(300))
    }

    /// Builds a transport with the given `connect` timeout and per-read
    /// `read` timeout.
    /// Returns `Network` when the HTTP client cannot start.
    pub fn with_timeouts(connect: Duration, read: Duration) -> Result<Self, ProviderError> {
        let client = reqwest::Client::builder().connect_timeout(connect).read_timeout(read).build().map_err(map_error)?;
        Ok(Self { client })
    }

    /// Wraps an existing `client`, keeping its own settings.
    /// Returns the transport.
    pub fn from_client(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ProviderError> {
        let mut builder = self.client.post(&request.url).body(request.body);
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let reply = builder.send().await.map_err(map_error)?;
        let status = reply.status().as_u16();
        let headers = reply
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str().to_owned(), v.to_owned())))
            .collect();
        Ok(HttpResponse { status, headers, body: Box::new(ReqwestBody(reply)) })
    }
}

struct ReqwestBody(reqwest::Response);

#[async_trait]
impl Body for ReqwestBody {
    async fn chunk(&mut self) -> Result<Option<Vec<u8>>, ProviderError> {
        Ok(self.0.chunk().await.map_err(map_error)?.map(|bytes| bytes.to_vec()))
    }
}

fn map_error(err: reqwest::Error) -> ProviderError {
    let kind = if err.is_timeout() { ProviderErrorKind::Stalled } else { ProviderErrorKind::Network };
    let err = err.without_url();
    let mut message = err.to_string();
    let mut source = std::error::Error::source(&err);
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    ProviderError::new(kind, message)
}
```

- [ ] **Step 4: Run, expect pass**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit && kiri run --rm troupekit-test-p3`
Expected: `sse`, `transport` and `reqwest_transport` tests pass with the earlier tests.

- [ ] **Step 5: Commit**

`git add troupekit && git commit -m "feat(troupekit-providers): sse framer, transport trait, scripted and reqwest transports"`

---
### Task 3: Wire layer and Anthropic Messages

**Files:**
- Create: `src/wire/mod.rs`, `src/wire/anthropic.rs`, stub `src/wire/openai.rs` and `src/wire/gemini.rs` (filled in Tasks 4 and 5), `tests/common/mod.rs`, `tests/anthropic.rs`, `tests/golden/anthropic_request.json`, `tests/golden/anthropic_reply.json`, `tests/golden/anthropic_stream.sse`
- Modify: `src/lib.rs` (add `pub mod wire;`)

**Interfaces:**
- `wire::Protocol { Anthropic, OpenAi, Gemini, OpenAiCompatible }` (serde `anthropic`, `openai`, `gemini`, `openai_compatible`).
- `wire::AuthStyle { Bearer, XApiKey, XGoogApiKey, NoKey }` (serde `bearer`, `x-api-key`, `x-goog-api-key`, `none`).
- `wire::Quirks { max_tokens_field: MaxTokensField, stream_usage: bool, output_schema: SchemaMode, temperature: bool }`, defaults `max_tokens`, `true`, `json_schema`, `true`; `deny_unknown_fields`.
- `wire::encode(Protocol, &Quirks, model: &str, &CompletionRequest, stream: bool) -> Result<Value, ProviderError>`.
- `wire::decode_body(Protocol, &Value) -> Result<Vec<Delta>, ProviderError>`.
- `wire::Decoder { new(Protocol), event(&Value) -> Result<Vec<Delta>, ProviderError>, sse(&SseEvent) -> Result<Vec<Delta>, ProviderError> }`.
- `wire::anthropic::API_VERSION = "2026-10-21"`.

Until Tasks 4 and 5 land, `openai.rs` and `gemini.rs` hold the same function and type names returning `bad_request("not implemented")`, so the dispatch compiles; Task 4 and Task 5 replace them whole.

- [ ] **Step 1: Shared test fixtures**

**File:** `troupekit/crates/troupekit-providers/tests/common/mod.rs`

```rust
#![allow(dead_code)]

use serde_json::{json, Value};
use std::sync::Arc;
use troupekit_core::event::Usage;
use troupekit_core::message::{ContentPart, MediaSource, Message, Role};
use troupekit_core::secret::Secret;
use troupekit_core::tool::ToolSpec;
use troupekit_providers::fold::Fold;
use troupekit_providers::model::{Endpoint, HttpModel};
use troupekit_providers::request::{CompletionRequest, CompletionResponse, Delta, StopReason};
use troupekit_providers::sse::SseParser;
use troupekit_providers::transport::{Scripted, ScriptedTransport};
use troupekit_providers::wire::{AuthStyle, Decoder, Protocol, Quirks};

pub fn call(id: &str, name: &str) -> ContentPart {
    ContentPart::ToolCall { id: id.into(), name: name.into(), arguments: json!({"days": 7}) }
}

pub fn result(id: &str) -> ContentPart {
    ContentPart::ToolResult { call_id: id.into(), content: "free: mon, wed".into(), is_error: false }
}

pub fn rich_request() -> CompletionRequest {
    CompletionRequest {
        model: None,
        system: Some("You plan workouts.".into()),
        messages: vec![
            Message::new(Role::User, vec![
                ContentPart::Text { text: "Plan my week".into() },
                ContentPart::Image { media_type: "image/png".into(), source: MediaSource::Base64 { data: "aGk=".into() } },
            ])
            .unwrap(),
            Message::new(Role::Assistant, vec![ContentPart::Text { text: "Checking the calendar.".into() }, call("call_1", "calendar")])
                .unwrap(),
            Message::new(Role::Tool, vec![result("call_1")]).unwrap(),
        ],
        tools: vec![ToolSpec {
            name: "calendar".into(),
            description: "Reads free slots".into(),
            schema: json!({"type":"object","properties":{"days":{"type":"integer"}},"required":["days"]}),
        }],
        max_tokens: 1024,
        temperature: Some(0.5),
        output_schema: Some(json!({"type":"object","properties":{"plan":{"type":"string"}}})),
    }
}

pub fn golden_text(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

pub fn golden(name: &str) -> Value {
    serde_json::from_str(&golden_text(name)).unwrap()
}

pub fn fold_all(deltas: &[Delta]) -> CompletionResponse {
    let mut fold = Fold::new();
    for d in deltas {
        fold.push(d).unwrap();
    }
    fold.finish().unwrap()
}

pub fn stream_deltas(protocol: Protocol, text: &str) -> (Vec<Delta>, CompletionResponse) {
    let mut parser = SseParser::new();
    let mut events = Vec::new();
    for chunk in text.as_bytes().chunks(7) {
        events.extend(parser.push(chunk).unwrap());
    }
    events.extend(parser.finish().unwrap());
    let mut decoder = Decoder::new(protocol);
    let mut deltas = Vec::new();
    for event in &events {
        deltas.extend(decoder.sse(event).unwrap());
    }
    let response = fold_all(&deltas);
    (deltas, response)
}

pub fn expected_reply(call_id: &str) -> CompletionResponse {
    CompletionResponse {
        message: Message::new(Role::Assistant, vec![
            ContentPart::Text { text: "Mon: run 5k.".into() },
            ContentPart::ToolCall { id: call_id.into(), name: "calendar".into(), arguments: json!({"days": 3}) },
        ])
        .unwrap(),
        usage: Usage { input_tokens: 12, output_tokens: 30, cost_usd_micros: 0 },
        stop: StopReason::ToolUse,
    }
}

pub fn test_key() -> Option<Secret> {
    Some(Secret::new("test-key-123".into()).unwrap())
}

pub fn scripted_model(protocol: Protocol, replies: Vec<Scripted>) -> (HttpModel, Arc<ScriptedTransport>) {
    let transport = Arc::new(ScriptedTransport::new(replies));
    let endpoint = Endpoint { protocol, base_url: "https://h.test".into(), auth: AuthStyle::Bearer, quirks: Quirks::default() };
    let model = HttpModel::new("vendor/m-1", "m-1", endpoint, test_key(), transport.clone()).unwrap();
    (model, transport)
}
```

`scripted_model` and `test_key` are used from Task 6 on; until `model.rs` exists, leave those two functions and the `model`, `secret`, `transport` and `Arc` imports out of this file and add them in Task 6 Step 1.

- [ ] **Step 2: Goldens**

**File:** `troupekit/crates/troupekit-providers/tests/golden/anthropic_request.json`

```json
{
  "model": "claude-opus-5-5",
  "max_tokens": 1024,
  "system": "You plan workouts.",
  "messages": [
    {
      "role": "user",
      "content": [
        {"type": "text", "text": "Plan my week"},
        {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "aGk="}}
      ]
    },
    {
      "role": "assistant",
      "content": [
        {"type": "text", "text": "Checking the calendar."},
        {"type": "tool_use", "id": "call_1", "name": "calendar", "input": {"days": 7}}
      ]
    },
    {
      "role": "user",
      "content": [
        {"type": "tool_result", "tool_use_id": "call_1", "content": "free: mon, wed", "is_error": false}
      ]
    }
  ],
  "tools": [
    {
      "name": "calendar",
      "description": "Reads free slots",
      "input_schema": {"type": "object", "properties": {"days": {"type": "integer"}}, "required": ["days"]}
    }
  ],
  "output_config": {
    "format": {"type": "json_schema", "schema": {"type": "object", "properties": {"plan": {"type": "string"}}}}
  }
}
```

**File:** `troupekit/crates/troupekit-providers/tests/golden/anthropic_reply.json`

```json
{
  "id": "msg_1",
  "type": "message",
  "role": "assistant",
  "model": "claude-opus-5-5",
  "content": [
    {"type": "thinking", "thinking": "", "signature": "sig"},
    {"type": "text", "text": "Mon: run 5k."},
    {"type": "tool_use", "id": "toolu_1", "name": "calendar", "input": {"days": 3}}
  ],
  "stop_reason": "tool_use",
  "usage": {"input_tokens": 10, "cache_read_input_tokens": 2, "output_tokens": 30}
}
```

**File:** `troupekit/crates/troupekit-providers/tests/golden/anthropic_stream.sse`

```text
event: message_start
data: {"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[],"usage":{"input_tokens":10,"cache_read_input_tokens":2,"output_tokens":1}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}

event: ping
data: {"type":"ping"}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Mon: run "}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"5k."}}

event: content_block_stop
data: {"type":"content_block_stop","index":1}

event: content_block_start
data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_1","name":"calendar","input":{}}}

event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"days\":"}}

event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"3}"}}

event: content_block_stop
data: {"type":"content_block_stop","index":2}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":30}}

event: message_stop
data: {"type":"message_stop"}

```

- [ ] **Step 3: Write the failing tests**

**File:** `troupekit/crates/troupekit-providers/tests/anthropic.rs`

```rust
mod common;

use common::*;
use serde_json::json;
use troupekit_core::error::ProviderErrorKind;
use troupekit_core::message::{Message, Role};
use troupekit_providers::request::{CompletionRequest, Delta, StopReason};
use troupekit_providers::wire::{decode_body, encode, Decoder, Protocol, Quirks, SchemaMode};

fn anthropic_quirks() -> Quirks {
    Quirks { temperature: false, ..Quirks::default() }
}

#[test]
fn encodes_the_golden_request() {
    let body = encode(Protocol::Anthropic, &anthropic_quirks(), "claude-opus-5-5", &rich_request(), false).unwrap();
    assert_eq!(body, golden("anthropic_request.json"));
}

#[test]
fn stream_flag_temperature_and_schema_follow_mode_and_quirks() {
    let req = rich_request();
    let body = encode(Protocol::Anthropic, &Quirks::default(), "m", &req, true).unwrap();
    assert_eq!(body["stream"], json!(true));
    assert_eq!(body["temperature"], json!(0.5));
    let plain = Quirks { output_schema: SchemaMode::Unsupported, ..Quirks::default() };
    assert!(encode(Protocol::Anthropic, &plain, "m", &req, false).unwrap().get("output_config").is_none());
}

#[test]
fn merges_consecutive_tool_results_into_one_user_turn() {
    let mut req = CompletionRequest::user("go", 10);
    req.messages.push(Message::new(Role::Assistant, vec![call("a", "t"), call("b", "t")]).unwrap());
    req.messages.push(Message::new(Role::Tool, vec![result("a")]).unwrap());
    req.messages.push(Message::new(Role::Tool, vec![result("b")]).unwrap());
    let body = encode(Protocol::Anthropic, &anthropic_quirks(), "m", &req, false).unwrap();
    let turns = body["messages"].as_array().unwrap();
    assert_eq!(turns.len(), 3);
    assert_eq!(turns[2]["role"], "user");
    assert_eq!(turns[2]["content"].as_array().unwrap().len(), 2);
}

#[test]
fn unary_and_stream_fold_to_the_same_reply() {
    let unary = fold_all(&decode_body(Protocol::Anthropic, &golden("anthropic_reply.json")).unwrap());
    let (deltas, streamed) = stream_deltas(Protocol::Anthropic, &golden_text("anthropic_stream.sse"));
    assert_eq!(unary, expected_reply("toolu_1"));
    assert_eq!(streamed, unary);
    assert!(deltas.contains(&Delta::TextDelta { text: "Mon: run ".into() }));
}

#[test]
fn maps_stream_error_events() {
    let mut decoder = Decoder::new(Protocol::Anthropic);
    let err = decoder.event(&json!({"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}})).unwrap_err();
    assert_eq!((err.kind, err.message.as_str()), (ProviderErrorKind::Server, "Overloaded"));
    let err = decoder.event(&json!({"type":"error","error":{"type":"rate_limit_error","message":"slow"}})).unwrap_err();
    assert_eq!(err.kind, ProviderErrorKind::RateLimited);
    let err = decode_body(Protocol::Anthropic, &json!({"type":"error","error":{"type":"invalid_request_error","message":"bad"}}));
    assert_eq!(err.unwrap_err().kind, ProviderErrorKind::BadRequest);
}

#[test]
fn maps_stop_reasons() {
    let table = [
        ("end_turn", StopReason::EndTurn),
        ("stop_sequence", StopReason::EndTurn),
        ("max_tokens", StopReason::MaxTokens),
        ("refusal", StopReason::Refusal),
        ("pause_turn", StopReason::Other("pause_turn".into())),
    ];
    for (wire, want) in table {
        let body = json!({"type":"message","content":[],"stop_reason":wire,"usage":{"input_tokens":1,"output_tokens":1}});
        assert_eq!(fold_all(&decode_body(Protocol::Anthropic, &body).unwrap()).stop, want, "{wire}");
    }
}
```

- [ ] **Step 4: Run, expect failure**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit`
Expected: build fails, `could not find wire in troupekit_providers`.

- [ ] **Step 5: Implement**

**File:** `troupekit/crates/troupekit-providers/src/wire/mod.rs`

```rust
//! The wire protocols: pure encode and decode functions, no I/O.
//! Each protocol module turns a `CompletionRequest` into JSON and wire
//! JSON back into `Delta`s; `Fold` turns deltas into the response.

pub mod anthropic;
pub mod gemini;
pub mod openai;

use crate::request::{CompletionRequest, Delta};
use crate::sse::SseEvent;
use crate::status::{decode_error, kind_for_status};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use troupekit_core::error::{ProviderError, ProviderErrorKind};

/// The wire protocols Troupekit speaks. OpenAI-compatible vendors use
/// the OpenAI Chat Completions encoding with their own `Quirks`.
/// Serializes as `anthropic`, `openai`, `gemini`, `openai_compatible`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "gemini")]
    Gemini,
    #[serde(rename = "openai_compatible")]
    OpenAiCompatible,
}

/// How a vendor takes its API key: a bearer token, an `x-api-key` or
/// `x-goog-api-key` header, or no key at all for local servers.
/// Serializes as `bearer`, `x-api-key`, `x-goog-api-key`, `none`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthStyle {
    #[serde(rename = "bearer")]
    Bearer,
    #[serde(rename = "x-api-key")]
    XApiKey,
    #[serde(rename = "x-goog-api-key")]
    XGoogApiKey,
    #[serde(rename = "none")]
    NoKey,
}

/// Which JSON field carries the output token cap on the OpenAI wire.
/// OpenAI's own reasoning models want `max_completion_tokens`.
/// Serializes as `max_tokens` or `max_completion_tokens`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaxTokensField {
    #[default]
    MaxTokens,
    MaxCompletionTokens,
}

/// How a vendor takes a requested output JSON Schema: as a JSON Schema,
/// as plain JSON mode, or not at all (the schema is then left out and
/// the caller validates the output). Serializes as `json_schema`, `json_object`, `none`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchemaMode {
    #[default]
    #[serde(rename = "json_schema")]
    JsonSchema,
    #[serde(rename = "json_object")]
    JsonObject,
    #[serde(rename = "none")]
    Unsupported,
}

/// Per-vendor differences inside one protocol, read from the catalog.
/// Every field has a default, so most vendors set none of them; unknown
/// fields are rejected so a typo cannot pass silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Quirks {
    pub max_tokens_field: MaxTokensField,
    pub stream_usage: bool,
    pub output_schema: SchemaMode,
    pub temperature: bool,
}

impl Default for Quirks {
    fn default() -> Self {
        Self { max_tokens_field: MaxTokensField::MaxTokens, stream_usage: true, output_schema: SchemaMode::JsonSchema, temperature: true }
    }
}

/// Encodes `req` as the JSON body `protocol` expects for wire `model`.
/// Pure, so goldens pin the exact shape. Returns `BadRequest` when the
/// request cannot be expressed on this wire.
pub fn encode(protocol: Protocol, quirks: &Quirks, model: &str, req: &CompletionRequest, stream: bool) -> Result<Value, ProviderError> {
    match protocol {
        Protocol::Anthropic => anthropic::encode(quirks, model, req, stream),
        Protocol::OpenAi | Protocol::OpenAiCompatible => openai::encode(quirks, model, req, stream),
        Protocol::Gemini => gemini::encode(quirks, model, req, stream),
    }
}

/// Turns a whole unary reply `body` into the deltas a stream of the
/// same reply would yield, so both paths share one `Fold`.
/// Returns `Decode` for a malformed body, or the error the body carries.
pub fn decode_body(protocol: Protocol, body: &Value) -> Result<Vec<Delta>, ProviderError> {
    match protocol {
        Protocol::Anthropic => anthropic::decode_body(body),
        Protocol::OpenAi | Protocol::OpenAiCompatible => openai::decode_body(body),
        Protocol::Gemini => gemini::decode_body(body),
    }
}

/// The streaming decoder for one reply, chosen by protocol.
/// Make a new one per reply; it keeps per-reply state such as which tool
/// calls have started.
#[derive(Debug)]
pub enum Decoder {
    Anthropic(anthropic::StreamDecoder),
    OpenAi(openai::StreamDecoder),
    Gemini(gemini::StreamDecoder),
}

impl Decoder {
    /// Starts a decoder for one streamed reply of `protocol`.
    /// Returns the decoder.
    pub fn new(protocol: Protocol) -> Self {
        match protocol {
            Protocol::Anthropic => Decoder::Anthropic(Default::default()),
            Protocol::OpenAi | Protocol::OpenAiCompatible => Decoder::OpenAi(Default::default()),
            Protocol::Gemini => Decoder::Gemini(Default::default()),
        }
    }

    /// Decodes the parsed `data` of one stream event.
    /// Returns its deltas, or the error an error event carries.
    pub fn event(&mut self, data: &Value) -> Result<Vec<Delta>, ProviderError> {
        match self {
            Decoder::Anthropic(d) => d.event(data),
            Decoder::OpenAi(d) => d.event(data),
            Decoder::Gemini(d) => d.event(data),
        }
    }

    /// Decodes one server-sent `event`: skips empty data and the OpenAI
    /// `[DONE]` marker, parses the JSON and hands it to `event`.
    /// Returns `Decode` when the data is not JSON.
    pub fn sse(&mut self, event: &SseEvent) -> Result<Vec<Delta>, ProviderError> {
        let data = event.data.trim();
        if data.is_empty() || data == "[DONE]" {
            return Ok(Vec::new());
        }
        let value: Value = serde_json::from_str(data).map_err(|e| decode_error(format!("event data is not JSON: {e}")))?;
        self.event(&value)
    }
}

pub(crate) fn push_turn(turns: &mut Vec<Value>, role: &str, key: &str, items: Vec<Value>) {
    if items.is_empty() {
        return;
    }
    if let Some(last) = turns.last_mut() {
        if last["role"] == role {
            if let Some(existing) = last[key].as_array_mut() {
                existing.extend(items);
                return;
            }
        }
    }
    let mut turn = Map::new();
    turn.insert("role".into(), Value::from(role));
    turn.insert(key.into(), Value::Array(items));
    turns.push(Value::Object(turn));
}

pub(crate) fn str_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

pub(crate) fn u64_at(value: &Value, pointer: &str) -> Option<u64> {
    value.pointer(pointer).and_then(Value::as_u64)
}

pub(crate) fn index_at(value: &Value, pointer: &str) -> Result<u32, ProviderError> {
    u64_at(value, pointer)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| decode_error(format!("missing or bad index at {pointer}")))
}

pub(crate) fn position(i: usize) -> Result<u32, ProviderError> {
    u32::try_from(i).map_err(|_| decode_error("too many parts in one reply"))
}

pub(crate) fn text_delta(text: Option<&str>) -> Option<Delta> {
    text.filter(|t| !t.is_empty()).map(|t| Delta::TextDelta { text: t.to_owned() })
}

pub(crate) fn error_object(error: &Value) -> ProviderError {
    let kind = u64_at(error, "/code")
        .and_then(|c| u16::try_from(c).ok())
        .map(kind_for_status)
        .unwrap_or(ProviderErrorKind::Server);
    ProviderError::new(kind, str_at(error, "/message").unwrap_or("provider error").to_owned())
}
```

**File:** `troupekit/crates/troupekit-providers/src/wire/anthropic.rs`

```rust
use super::{index_at, position, push_turn, str_at, text_delta, u64_at, Quirks, SchemaMode};
use crate::request::{CompletionRequest, Delta, StopReason};
use crate::status::decode_error;
use serde_json::{json, Value};
use troupekit_core::error::{ProviderError, ProviderErrorKind};
use troupekit_core::message::{ContentPart, MediaSource, Role};

/// The `anthropic-version` header this encoder targets.
/// On it Claude rejects sampling parameters, so the catalog sets the
/// `temperature` quirk to false for Anthropic.
pub const API_VERSION: &str = "2026-10-21";

/// Encodes `req` as an Anthropic Messages body for `model`.
/// Tool results ride in user turns and same-role turns are merged.
/// Returns the body; every request fits this wire.
pub fn encode(quirks: &Quirks, model: &str, req: &CompletionRequest, stream: bool) -> Result<Value, ProviderError> {
    let mut messages = Vec::new();
    for m in &req.messages {
        let role = match m.role {
            Role::System => continue,
            Role::Assistant => "assistant",
            Role::User | Role::Tool => "user",
        };
        push_turn(&mut messages, role, "content", m.parts.iter().map(block).collect());
    }
    let mut body = json!({"model": model, "max_tokens": req.max_tokens, "messages": messages});
    if let Some(system) = req.system_text() {
        body["system"] = json!(system);
    }
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.schema}))
            .collect();
    }
    if let (true, Some(t)) = (quirks.temperature, req.temperature) {
        body["temperature"] = json!(t);
    }
    if let (Some(schema), false) = (&req.output_schema, quirks.output_schema == SchemaMode::Unsupported) {
        body["output_config"] = json!({"format": {"type": "json_schema", "schema": schema}});
    }
    if stream {
        body["stream"] = json!(true);
    }
    Ok(body)
}

/// Turns a whole Messages reply into the deltas its stream would yield.
/// Thinking and unknown blocks are skipped. Returns `Decode` without a
/// content array, or the mapped error for an error body.
pub fn decode_body(body: &Value) -> Result<Vec<Delta>, ProviderError> {
    if body["type"] == "error" {
        return Err(error_event(body));
    }
    let blocks = body["content"].as_array().ok_or_else(|| decode_error("reply has no content"))?;
    let mut out = vec![usage(&body["usage"])];
    for (i, b) in blocks.iter().enumerate() {
        let index = position(i)?;
        match str_at(b, "/type") {
            Some("text") => out.extend(text_delta(str_at(b, "/text"))),
            Some("tool_use") => {
                out.push(started(index, b));
                out.push(Delta::ToolCallArgs { index, json: b["input"].to_string() });
            }
            _ => {}
        }
    }
    out.push(Delta::Stop { reason: stop_reason(str_at(body, "/stop_reason").unwrap_or("end_turn")) });
    Ok(out)
}

/// Decodes one Anthropic Messages stream. Wire block indexes key tool
/// calls; thinking, signature, ping and stop events yield nothing.
/// Make one per streamed reply.
#[derive(Debug, Default)]
pub struct StreamDecoder;

impl StreamDecoder {
    /// Decodes one event's parsed `data`.
    /// Returns its deltas, or the mapped error for an `error` event.
    pub fn event(&mut self, data: &Value) -> Result<Vec<Delta>, ProviderError> {
        Ok(match str_at(data, "/type").unwrap_or("") {
            "message_start" => vec![usage(&data["message"]["usage"])],
            "content_block_start" => {
                let b = &data["content_block"];
                match str_at(b, "/type") {
                    Some("tool_use") => vec![started(index_at(data, "/index")?, b)],
                    Some("text") => text_delta(str_at(b, "/text")).into_iter().collect(),
                    _ => Vec::new(),
                }
            }
            "content_block_delta" => match str_at(data, "/delta/type") {
                Some("text_delta") => text_delta(str_at(data, "/delta/text")).into_iter().collect(),
                Some("input_json_delta") => vec![Delta::ToolCallArgs {
                    index: index_at(data, "/index")?,
                    json: str_at(data, "/delta/partial_json").unwrap_or_default().to_owned(),
                }],
                _ => Vec::new(),
            },
            "message_delta" => {
                let mut out = vec![usage(&data["usage"])];
                if let Some(reason) = str_at(data, "/delta/stop_reason") {
                    out.push(Delta::Stop { reason: stop_reason(reason) });
                }
                out
            }
            "error" => return Err(error_event(data)),
            _ => Vec::new(),
        })
    }
}

fn block(part: &ContentPart) -> Value {
    match part {
        ContentPart::Text { text } => json!({"type": "text", "text": text}),
        ContentPart::Image { source: MediaSource::Url { url }, .. } => {
            json!({"type": "image", "source": {"type": "url", "url": url}})
        }
        ContentPart::Image { media_type, source: MediaSource::Base64 { data } } => {
            json!({"type": "image", "source": {"type": "base64", "media_type": media_type, "data": data}})
        }
        ContentPart::ToolCall { id, name, arguments } => json!({"type": "tool_use", "id": id, "name": name, "input": arguments}),
        ContentPart::ToolResult { call_id, content, is_error } => {
            json!({"type": "tool_result", "tool_use_id": call_id, "content": content, "is_error": is_error})
        }
    }
}

fn started(index: u32, block: &Value) -> Delta {
    Delta::ToolCallStarted {
        index,
        id: str_at(block, "/id").unwrap_or_default().to_owned(),
        name: str_at(block, "/name").unwrap_or_default().to_owned(),
    }
}

fn usage(u: &Value) -> Delta {
    let input = ["/input_tokens", "/cache_creation_input_tokens", "/cache_read_input_tokens"]
        .iter()
        .filter_map(|p| u64_at(u, p))
        .reduce(u64::saturating_add);
    Delta::Usage { input_tokens: input, output_tokens: u64_at(u, "/output_tokens") }
}

fn stop_reason(reason: &str) -> StopReason {
    match reason {
        "end_turn" | "stop_sequence" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        "refusal" => StopReason::Refusal,
        other => StopReason::Other(other.to_owned()),
    }
}

fn error_event(data: &Value) -> ProviderError {
    let kind = match str_at(data, "/error/type").unwrap_or("") {
        "rate_limit_error" => ProviderErrorKind::RateLimited,
        "authentication_error" | "permission_error" => ProviderErrorKind::Auth,
        "invalid_request_error" | "not_found_error" | "request_too_large" => ProviderErrorKind::BadRequest,
        "timeout_error" => ProviderErrorKind::Stalled,
        _ => ProviderErrorKind::Server,
    };
    ProviderError::new(kind, str_at(data, "/error/message").unwrap_or("stream error").to_owned())
}
```

Temporary `openai.rs` and `gemini.rs` for this task (each, with its own module name in the message):

```rust
use super::Quirks;
use crate::request::{CompletionRequest, Delta};
use crate::status::bad_request;
use serde_json::Value;
use troupekit_core::error::ProviderError;

pub fn encode(_: &Quirks, _: &str, _: &CompletionRequest, _: bool) -> Result<Value, ProviderError> {
    Err(bad_request("not implemented"))
}

pub fn decode_body(_: &Value) -> Result<Vec<Delta>, ProviderError> {
    Err(bad_request("not implemented"))
}

#[derive(Debug, Default)]
pub struct StreamDecoder;

impl StreamDecoder {
    pub fn event(&mut self, _: &Value) -> Result<Vec<Delta>, ProviderError> {
        Err(bad_request("not implemented"))
    }
}
```

- [ ] **Step 6: Run, expect pass**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit && kiri run --rm troupekit-test-p3`
Expected: `anthropic` tests pass with all earlier tests.

- [ ] **Step 7: Commit**

`git add troupekit && git commit -m "feat(troupekit-providers): wire layer and anthropic messages protocol"`

---
### Task 4: OpenAI Chat Completions and OpenAI-compatible

**Files:**
- Replace: `src/wire/openai.rs`
- Create: `tests/openai.rs`, `tests/golden/openai_request.json`, `tests/golden/openai_reply.json`, `tests/golden/openai_stream.sse`

**Interfaces:** `wire::openai::{encode, decode_body, StreamDecoder}` with the signatures of Task 3. `Protocol::OpenAiCompatible` uses the same functions; differences come only from `Quirks`.

- [ ] **Step 1: Goldens**

**File:** `troupekit/crates/troupekit-providers/tests/golden/openai_request.json`

```json
{
  "model": "gpt-6.2-sol",
  "messages": [
    {"role": "system", "content": "You plan workouts."},
    {
      "role": "user",
      "content": [
        {"type": "text", "text": "Plan my week"},
        {"type": "image_url", "image_url": {"url": "data:image/png;base64,aGk="}}
      ]
    },
    {
      "role": "assistant",
      "content": "Checking the calendar.",
      "tool_calls": [
        {"id": "call_1", "type": "function", "function": {"name": "calendar", "arguments": "{\"days\":7}"}}
      ]
    },
    {"role": "tool", "tool_call_id": "call_1", "content": "free: mon, wed"}
  ],
  "max_completion_tokens": 1024,
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "calendar",
        "description": "Reads free slots",
        "parameters": {"type": "object", "properties": {"days": {"type": "integer"}}, "required": ["days"]}
      }
    }
  ],
  "response_format": {
    "type": "json_schema",
    "json_schema": {"name": "output", "schema": {"type": "object", "properties": {"plan": {"type": "string"}}}}
  }
}
```

**File:** `troupekit/crates/troupekit-providers/tests/golden/openai_reply.json`

```json
{
  "id": "chatcmpl-1",
  "object": "chat.completion",
  "model": "gpt-6.2-sol",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Mon: run 5k.",
        "tool_calls": [
          {"id": "toolu_1", "type": "function", "function": {"name": "calendar", "arguments": "{\"days\":3}"}}
        ]
      },
      "finish_reason": "tool_calls"
    }
  ],
  "usage": {"prompt_tokens": 12, "completion_tokens": 30, "total_tokens": 42}
}
```

**File:** `troupekit/crates/troupekit-providers/tests/golden/openai_stream.sse`

```text
data: {"id":"c","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}],"usage":null}

data: {"id":"c","choices":[{"index":0,"delta":{"content":"Mon: run "},"finish_reason":null}],"usage":null}

data: {"id":"c","choices":[{"index":0,"delta":{"content":"5k."},"finish_reason":null}],"usage":null}

data: {"id":"c","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"toolu_1","type":"function","function":{"name":"calendar","arguments":""}}]},"finish_reason":null}],"usage":null}

data: {"id":"c","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"days\":"}}]},"finish_reason":null}],"usage":null}

data: {"id":"c","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"3}"}}]},"finish_reason":null}],"usage":null}

data: {"id":"c","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":null}

data: {"id":"c","choices":[],"usage":{"prompt_tokens":12,"completion_tokens":30,"total_tokens":42}}

data: [DONE]

```

- [ ] **Step 2: Write the failing tests**

**File:** `troupekit/crates/troupekit-providers/tests/openai.rs`

```rust
mod common;

use common::*;
use serde_json::json;
use troupekit_core::error::ProviderErrorKind;
use troupekit_core::message::{ContentPart, Message, Role};
use troupekit_providers::request::{CompletionRequest, Delta, StopReason};
use troupekit_providers::wire::{decode_body, encode, Decoder, MaxTokensField, Protocol, Quirks, SchemaMode};

fn openai_quirks() -> Quirks {
    Quirks { max_tokens_field: MaxTokensField::MaxCompletionTokens, temperature: false, ..Quirks::default() }
}

#[test]
fn encodes_the_golden_request() {
    let body = encode(Protocol::OpenAi, &openai_quirks(), "gpt-6.2-sol", &rich_request(), false).unwrap();
    assert_eq!(body, golden("openai_request.json"));
}

#[test]
fn compatible_vendors_follow_their_quirks() {
    let req = rich_request();
    let plain = Quirks { output_schema: SchemaMode::Unsupported, stream_usage: false, ..Quirks::default() };
    let body = encode(Protocol::OpenAiCompatible, &plain, "m", &req, true).unwrap();
    assert_eq!(body["max_tokens"], json!(1024));
    assert!(body.get("max_completion_tokens").is_none());
    assert!(body.get("response_format").is_none());
    assert_eq!(body["temperature"], json!(0.5));
    assert_eq!(body["stream"], json!(true));
    assert!(body.get("stream_options").is_none());
    let json_mode = Quirks { output_schema: SchemaMode::JsonObject, ..Quirks::default() };
    let body = encode(Protocol::OpenAiCompatible, &json_mode, "m", &req, true).unwrap();
    assert_eq!(body["response_format"], json!({"type": "json_object"}));
    assert_eq!(body["stream_options"], json!({"include_usage": true}));
}

#[test]
fn text_only_user_turns_are_plain_strings() {
    let body = encode(Protocol::OpenAiCompatible, &Quirks::default(), "m", &CompletionRequest::user("hi", 5), false).unwrap();
    assert_eq!(body["messages"], json!([{"role": "user", "content": "hi"}]));
}

#[test]
fn rejects_parts_the_wire_cannot_carry() {
    let mut req = CompletionRequest::user("go", 10);
    req.messages.push(Message::new(Role::Tool, vec![result("a"), ContentPart::Text { text: "note".into() }]).unwrap());
    let err = encode(Protocol::OpenAi, &Quirks::default(), "m", &req, false).unwrap_err();
    assert_eq!(err.kind, ProviderErrorKind::BadRequest);
}

#[test]
fn unary_and_stream_fold_to_the_same_reply() {
    let unary = fold_all(&decode_body(Protocol::OpenAi, &golden("openai_reply.json")).unwrap());
    let (deltas, streamed) = stream_deltas(Protocol::OpenAi, &golden_text("openai_stream.sse"));
    assert_eq!(unary, expected_reply("toolu_1"));
    assert_eq!(streamed, unary);
    assert!(deltas.contains(&Delta::TextDelta { text: "5k.".into() }));
}

#[test]
fn a_tool_call_id_repeated_in_every_fragment_starts_once() {
    let mut decoder = Decoder::new(Protocol::OpenAiCompatible);
    let chunk = |args: &str| json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"calendar","arguments":args}}]}}]});
    let mut deltas = decoder.event(&chunk("{\"days\":")).unwrap();
    deltas.extend(decoder.event(&chunk("3}")).unwrap());
    deltas.extend(decoder.event(&json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]})).unwrap());
    let reply = fold_all(&deltas);
    assert_eq!(reply.message.tool_calls().count(), 1);
    assert_eq!(reply.stop, StopReason::ToolUse);
}

#[test]
fn maps_errors_and_finish_reasons() {
    let mut decoder = Decoder::new(Protocol::OpenAi);
    let err = decoder.event(&json!({"error":{"message":"quota","code":429}})).unwrap_err();
    assert_eq!((err.kind, err.message.as_str()), (ProviderErrorKind::RateLimited, "quota"));
    assert_eq!(decode_body(Protocol::OpenAi, &json!({"choices":[]})).unwrap_err().kind, ProviderErrorKind::Decode);
    let table = [
        ("stop", StopReason::EndTurn),
        ("length", StopReason::MaxTokens),
        ("content_filter", StopReason::Refusal),
        ("weird", StopReason::Other("weird".into())),
    ];
    for (wire, want) in table {
        let body = json!({"choices":[{"message":{"content":"x"},"finish_reason":wire}]});
        assert_eq!(fold_all(&decode_body(Protocol::OpenAi, &body).unwrap()).stop, want, "{wire}");
    }
}
```

- [ ] **Step 3: Run, expect failure**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit && kiri run --rm troupekit-test-p3`
Expected: `openai` tests fail with `BadRequest: not implemented`.

- [ ] **Step 4: Implement**

**File:** `troupekit/crates/troupekit-providers/src/wire/openai.rs`

```rust
use super::{error_object, index_at, position, str_at, text_delta, u64_at, MaxTokensField, Quirks, SchemaMode};
use crate::request::{CompletionRequest, Delta, StopReason};
use crate::status::{bad_request, decode_error};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use troupekit_core::error::ProviderError;
use troupekit_core::message::{ContentPart, MediaSource, Message, Role};

/// Encodes `req` as an OpenAI Chat Completions body for `model`, shaped
/// by the vendor `quirks`. Returns `BadRequest` for parts this wire
/// cannot carry, such as text in a tool turn or images from the assistant.
pub fn encode(quirks: &Quirks, model: &str, req: &CompletionRequest, stream: bool) -> Result<Value, ProviderError> {
    let mut messages = Vec::new();
    if let Some(system) = req.system_text() {
        messages.push(json!({"role": "system", "content": system}));
    }
    for m in &req.messages {
        match m.role {
            Role::System => {}
            Role::User => messages.push(json!({"role": "user", "content": user_content(m)?})),
            Role::Assistant => messages.push(assistant(m)?),
            Role::Tool => {
                for part in &m.parts {
                    match part {
                        ContentPart::ToolResult { call_id, content, .. } => {
                            messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": content}))
                        }
                        _ => return Err(bad_request("a tool turn may only hold tool results on this wire")),
                    }
                }
            }
        }
    }
    let mut body = json!({"model": model, "messages": messages});
    let cap = match quirks.max_tokens_field {
        MaxTokensField::MaxTokens => "max_tokens",
        MaxTokensField::MaxCompletionTokens => "max_completion_tokens",
    };
    body[cap] = json!(req.max_tokens);
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.schema}}))
            .collect();
    }
    if let (true, Some(t)) = (quirks.temperature, req.temperature) {
        body["temperature"] = json!(t);
    }
    if let Some(schema) = &req.output_schema {
        match quirks.output_schema {
            SchemaMode::JsonSchema => {
                body["response_format"] = json!({"type": "json_schema", "json_schema": {"name": "output", "schema": schema}})
            }
            SchemaMode::JsonObject => body["response_format"] = json!({"type": "json_object"}),
            SchemaMode::Unsupported => {}
        }
    }
    if stream {
        body["stream"] = json!(true);
        if quirks.stream_usage {
            body["stream_options"] = json!({"include_usage": true});
        }
    }
    Ok(body)
}

/// Turns a whole Chat Completions reply into the deltas its stream would
/// yield, reading the first choice only.
/// Returns `Decode` without choices, or the error an error body carries.
pub fn decode_body(body: &Value) -> Result<Vec<Delta>, ProviderError> {
    if let Some(err) = body.get("error").filter(|e| !e.is_null()) {
        return Err(error_object(err));
    }
    let choice = body.pointer("/choices/0").ok_or_else(|| decode_error("reply has no choices"))?;
    let message = &choice["message"];
    let mut out: Vec<Delta> = text_delta(str_at(message, "/content")).into_iter().collect();
    if let Some(calls) = message["tool_calls"].as_array() {
        for (i, c) in calls.iter().enumerate() {
            let index = position(i)?;
            out.push(Delta::ToolCallStarted {
                index,
                id: str_at(c, "/id").unwrap_or_default().to_owned(),
                name: str_at(c, "/function/name").unwrap_or_default().to_owned(),
            });
            out.push(Delta::ToolCallArgs { index, json: arguments(&c["function"]["arguments"]) });
        }
    }
    out.push(usage(&body["usage"]));
    out.push(Delta::Stop { reason: finish_reason(str_at(choice, "/finish_reason").unwrap_or("stop")) });
    Ok(out)
}

/// Decodes one Chat Completions stream. Tool calls are keyed by their
/// wire index and start once, even when a server repeats the id in every
/// fragment. Make one per streamed reply.
#[derive(Debug, Default)]
pub struct StreamDecoder {
    started: BTreeSet<u32>,
}

impl StreamDecoder {
    /// Decodes one chunk's parsed `data`, reading the first choice only.
    /// Returns its deltas, or the error an error chunk carries.
    pub fn event(&mut self, data: &Value) -> Result<Vec<Delta>, ProviderError> {
        if let Some(err) = data.get("error").filter(|e| !e.is_null()) {
            return Err(error_object(err));
        }
        let mut out = Vec::new();
        if let Some(choice) = data.pointer("/choices/0") {
            let delta = &choice["delta"];
            out.extend(text_delta(str_at(delta, "/content")));
            if let Some(calls) = delta["tool_calls"].as_array() {
                for (i, c) in calls.iter().enumerate() {
                    let index = if c.get("index").is_some() { index_at(c, "/index")? } else { position(i)? };
                    if let Some(id) = str_at(c, "/id").filter(|id| !id.is_empty()) {
                        if self.started.insert(index) {
                            out.push(Delta::ToolCallStarted {
                                index,
                                id: id.to_owned(),
                                name: str_at(c, "/function/name").unwrap_or_default().to_owned(),
                            });
                        }
                    }
                    let json = arguments(&c["function"]["arguments"]);
                    if !json.is_empty() {
                        out.push(Delta::ToolCallArgs { index, json });
                    }
                }
            }
            if let Some(reason) = str_at(choice, "/finish_reason") {
                out.push(Delta::Stop { reason: finish_reason(reason) });
            }
        }
        if data.get("usage").is_some_and(Value::is_object) {
            out.push(usage(&data["usage"]));
        }
        Ok(out)
    }
}

fn user_content(m: &Message) -> Result<Value, ProviderError> {
    if m.parts.iter().all(|p| matches!(p, ContentPart::Text { .. })) {
        return Ok(json!(m.text_content()));
    }
    let mut items = Vec::new();
    for part in &m.parts {
        items.push(match part {
            ContentPart::Text { text } => json!({"type": "text", "text": text}),
            ContentPart::Image { media_type, source } => {
                let url = match source {
                    MediaSource::Url { url } => url.clone(),
                    MediaSource::Base64 { data } => format!("data:{media_type};base64,{data}"),
                };
                json!({"type": "image_url", "image_url": {"url": url}})
            }
            _ => return Err(bad_request("a user turn may only hold text and images on this wire")),
        });
    }
    Ok(Value::Array(items))
}

fn assistant(m: &Message) -> Result<Value, ProviderError> {
    let mut calls = Vec::new();
    for part in &m.parts {
        match part {
            ContentPart::Text { .. } => {}
            ContentPart::ToolCall { id, name, arguments } => calls.push(json!({
                "id": id,
                "type": "function",
                "function": {"name": name, "arguments": arguments.to_string()}
            })),
            _ => return Err(bad_request("an assistant turn may only hold text and tool calls on this wire")),
        }
    }
    let text = m.text_content();
    let mut message = json!({"role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) }});
    if !calls.is_empty() {
        message["tool_calls"] = Value::Array(calls);
    }
    Ok(message)
}

fn arguments(raw: &Value) -> String {
    match raw {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn usage(u: &Value) -> Delta {
    Delta::Usage { input_tokens: u64_at(u, "/prompt_tokens"), output_tokens: u64_at(u, "/completion_tokens") }
}

fn finish_reason(reason: &str) -> StopReason {
    match reason {
        "stop" => StopReason::EndTurn,
        "tool_calls" | "function_call" => StopReason::ToolUse,
        "length" => StopReason::MaxTokens,
        "content_filter" => StopReason::Refusal,
        other => StopReason::Other(other.to_owned()),
    }
}
```

- [ ] **Step 5: Run, expect pass**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit && kiri run --rm troupekit-test-p3`
Expected: `openai` tests pass with all earlier tests.

- [ ] **Step 6: Commit**

`git add troupekit && git commit -m "feat(troupekit-providers): openai chat completions and compatible dialects"`

---
### Task 5: Gemini generateContent

**Files:**
- Replace: `src/wire/gemini.rs`
- Create: `tests/gemini.rs`, `tests/golden/gemini_request.json`, `tests/golden/gemini_reply.json`, `tests/golden/gemini_stream.sse`

**Interfaces:** `wire::gemini::{encode, decode_body, StreamDecoder}` with the signatures of Task 3. The model id goes in the URL (Task 6), not the body. Streamed chunks have the unary shape, so `decode_body` is one `StreamDecoder` event.

- [ ] **Step 1: Goldens**

**File:** `troupekit/crates/troupekit-providers/tests/golden/gemini_request.json`

```json
{
  "systemInstruction": {"parts": [{"text": "You plan workouts."}]},
  "contents": [
    {"role": "user", "parts": [{"text": "Plan my week"}, {"inlineData": {"mimeType": "image/png", "data": "aGk="}}]},
    {"role": "model", "parts": [{"text": "Checking the calendar."}, {"functionCall": {"name": "calendar", "args": {"days": 7}}}]},
    {"role": "user", "parts": [{"functionResponse": {"name": "calendar", "response": {"output": "free: mon, wed"}}}]}
  ],
  "tools": [
    {
      "functionDeclarations": [
        {
          "name": "calendar",
          "description": "Reads free slots",
          "parametersJsonSchema": {"type": "object", "properties": {"days": {"type": "integer"}}, "required": ["days"]}
        }
      ]
    }
  ],
  "generationConfig": {
    "maxOutputTokens": 1024,
    "temperature": 0.5,
    "responseMimeType": "application/json",
    "responseJsonSchema": {"type": "object", "properties": {"plan": {"type": "string"}}}
  }
}
```

**File:** `troupekit/crates/troupekit-providers/tests/golden/gemini_reply.json`

```json
{
  "candidates": [
    {
      "content": {
        "role": "model",
        "parts": [
          {"text": "Weighing rest days.", "thought": true},
          {"text": "Mon: run 5k."},
          {"functionCall": {"name": "calendar", "args": {"days": 3}}}
        ]
      },
      "finishReason": "STOP",
      "index": 0
    }
  ],
  "usageMetadata": {"promptTokenCount": 12, "candidatesTokenCount": 25, "thoughtsTokenCount": 5, "totalTokenCount": 42}
}
```

**File:** `troupekit/crates/troupekit-providers/tests/golden/gemini_stream.sse`

```text
data: {"candidates":[{"content":{"role":"model","parts":[{"text":"Mon: run "}]},"index":0}],"usageMetadata":{"promptTokenCount":12,"candidatesTokenCount":3,"totalTokenCount":15}}

data: {"candidates":[{"content":{"role":"model","parts":[{"text":"5k."}]},"index":0}],"usageMetadata":{"promptTokenCount":12,"candidatesTokenCount":6,"totalTokenCount":18}}

data: {"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"calendar","args":{"days":3}}}]},"finishReason":"STOP","index":0}],"usageMetadata":{"promptTokenCount":12,"candidatesTokenCount":25,"thoughtsTokenCount":5,"totalTokenCount":42}}

```

- [ ] **Step 2: Write the failing tests**

**File:** `troupekit/crates/troupekit-providers/tests/gemini.rs`

```rust
mod common;

use common::*;
use serde_json::json;
use troupekit_core::error::ProviderErrorKind;
use troupekit_core::message::{ContentPart, Message, Role};
use troupekit_providers::request::{CompletionRequest, Delta, StopReason};
use troupekit_providers::wire::{decode_body, encode, Decoder, Protocol, Quirks};

#[test]
fn encodes_the_golden_request() {
    let body = encode(Protocol::Gemini, &Quirks::default(), "gemini-3.8-flash", &rich_request(), false).unwrap();
    assert_eq!(body, golden("gemini_request.json"));
}

#[test]
fn tool_results_need_an_earlier_call_and_take_its_latest_name() {
    let mut req = CompletionRequest::user("go", 10);
    req.messages.push(Message::new(Role::Tool, vec![result("call_0")]).unwrap());
    assert_eq!(encode(Protocol::Gemini, &Quirks::default(), "m", &req, false).unwrap_err().kind, ProviderErrorKind::BadRequest);

    let mut req = CompletionRequest::user("go", 10);
    req.messages.push(Message::new(Role::Assistant, vec![call("call_0", "calendar")]).unwrap());
    req.messages.push(Message::new(Role::Tool, vec![result("call_0")]).unwrap());
    req.messages.push(Message::new(Role::Assistant, vec![call("call_0", "weather")]).unwrap());
    let failed = ContentPart::ToolResult { call_id: "call_0".into(), content: "offline".into(), is_error: true };
    req.messages.push(Message::new(Role::Tool, vec![failed]).unwrap());
    let body = encode(Protocol::Gemini, &Quirks::default(), "m", &req, false).unwrap();
    assert_eq!(body["contents"][2]["parts"][0]["functionResponse"]["name"], "calendar");
    assert_eq!(body["contents"][4]["parts"][0]["functionResponse"], json!({"name": "weather", "response": {"error": "offline"}}));
}

#[test]
fn unary_and_stream_fold_to_the_same_reply() {
    let unary = fold_all(&decode_body(Protocol::Gemini, &golden("gemini_reply.json")).unwrap());
    let (deltas, streamed) = stream_deltas(Protocol::Gemini, &golden_text("gemini_stream.sse"));
    assert_eq!(unary, expected_reply("call_0"));
    assert_eq!(streamed, unary);
    assert!(deltas.contains(&Delta::TextDelta { text: "Mon: run ".into() }));
}

#[test]
fn keeps_wire_call_ids_and_numbers_the_rest() {
    let body = json!({"candidates":[{"content":{"parts":[
        {"functionCall":{"id":"fc-9","name":"a","args":{}}},
        {"functionCall":{"name":"b"}}
    ]},"finishReason":"STOP"}]});
    let reply = fold_all(&decode_body(Protocol::Gemini, &body).unwrap());
    let ids: Vec<&str> = reply.message.tool_calls().map(|(id, _, _)| id).collect();
    assert_eq!(ids, ["fc-9", "call_1"]);
    assert_eq!(reply.stop, StopReason::ToolUse);
}

#[test]
fn maps_errors_blocks_and_finish_reasons() {
    let mut decoder = Decoder::new(Protocol::Gemini);
    let err = decoder.event(&json!({"error":{"code":429,"message":"quota","status":"RESOURCE_EXHAUSTED"}})).unwrap_err();
    assert_eq!((err.kind, err.message.as_str()), (ProviderErrorKind::RateLimited, "quota"));
    let blocked = json!({"promptFeedback":{"blockReason":"SAFETY"},"usageMetadata":{"promptTokenCount":4}});
    assert_eq!(fold_all(&decode_body(Protocol::Gemini, &blocked).unwrap()).stop, StopReason::Refusal);
    let table = [("MAX_TOKENS", StopReason::MaxTokens), ("SAFETY", StopReason::Refusal), ("OTHER", StopReason::Other("OTHER".into()))];
    for (wire, want) in table {
        let body = json!({"candidates":[{"content":{"parts":[{"text":"x"}]},"finishReason":wire}]});
        assert_eq!(fold_all(&decode_body(Protocol::Gemini, &body).unwrap()).stop, want, "{wire}");
    }
}
```

- [ ] **Step 3: Run, expect failure**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit && kiri run --rm troupekit-test-p3`
Expected: `gemini` tests fail with `BadRequest: not implemented`.

- [ ] **Step 4: Implement**

**File:** `troupekit/crates/troupekit-providers/src/wire/gemini.rs`

```rust
use super::{error_object, push_turn, str_at, text_delta, u64_at, Quirks, SchemaMode};
use crate::request::{CompletionRequest, Delta, StopReason};
use crate::status::{bad_request, decode_error};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use troupekit_core::error::ProviderError;
use troupekit_core::message::{ContentPart, MediaSource, Role};

/// Encodes `req` as a Gemini generateContent body; the model id goes in
/// the URL. Returns `BadRequest` for a tool result whose call id matches
/// no earlier tool call, since Gemini needs the function name.
pub fn encode(quirks: &Quirks, _model: &str, req: &CompletionRequest, _stream: bool) -> Result<Value, ProviderError> {
    let mut names: HashMap<&str, &str> = HashMap::new();
    let mut contents = Vec::new();
    for m in &req.messages {
        let role = match m.role {
            Role::System => continue,
            Role::Assistant => "model",
            Role::User | Role::Tool => "user",
        };
        let mut parts = Vec::new();
        for part in &m.parts {
            parts.push(match part {
                ContentPart::Text { text } => json!({"text": text}),
                ContentPart::Image { media_type, source: MediaSource::Url { url } } => {
                    json!({"fileData": {"mimeType": media_type, "fileUri": url}})
                }
                ContentPart::Image { media_type, source: MediaSource::Base64 { data } } => {
                    json!({"inlineData": {"mimeType": media_type, "data": data}})
                }
                ContentPart::ToolCall { id, name, arguments } => {
                    names.insert(id.as_str(), name.as_str());
                    json!({"functionCall": {"name": name, "args": arguments}})
                }
                ContentPart::ToolResult { call_id, content, is_error } => {
                    let name = names
                        .get(call_id.as_str())
                        .ok_or_else(|| bad_request(format!("tool result {call_id} matches no earlier tool call")))?;
                    let mut response = Map::new();
                    response.insert(if *is_error { "error" } else { "output" }.into(), json!(content));
                    json!({"functionResponse": {"name": name, "response": response}})
                }
            });
        }
        push_turn(&mut contents, role, "parts", parts);
    }
    let mut config = json!({"maxOutputTokens": req.max_tokens});
    if let (true, Some(t)) = (quirks.temperature, req.temperature) {
        config["temperature"] = json!(t);
    }
    if let Some(schema) = &req.output_schema {
        match quirks.output_schema {
            SchemaMode::JsonSchema => {
                config["responseMimeType"] = json!("application/json");
                config["responseJsonSchema"] = schema.clone();
            }
            SchemaMode::JsonObject => config["responseMimeType"] = json!("application/json"),
            SchemaMode::Unsupported => {}
        }
    }
    let mut body = json!({"contents": contents, "generationConfig": config});
    if let Some(system) = req.system_text() {
        body["systemInstruction"] = json!({"parts": [{"text": system}]});
    }
    if !req.tools.is_empty() {
        let declarations: Vec<Value> = req
            .tools
            .iter()
            .map(|t| json!({"name": t.name, "description": t.description, "parametersJsonSchema": t.schema}))
            .collect();
        body["tools"] = json!([{"functionDeclarations": declarations}]);
    }
    Ok(body)
}

/// Turns a whole generateContent reply into deltas. Streamed chunks have
/// the same shape, so this is one event of a fresh `StreamDecoder`.
/// Returns the error an error body carries.
pub fn decode_body(body: &Value) -> Result<Vec<Delta>, ProviderError> {
    StreamDecoder::default().event(body)
}

/// Decodes one Gemini stream. Thought parts are skipped, each function
/// call arrives whole, and calls without a wire id get `call_<n>`,
/// numbered per reply. Make one per streamed reply.
#[derive(Debug, Default)]
pub struct StreamDecoder {
    next_call: u32,
}

impl StreamDecoder {
    /// Decodes one chunk's parsed `data`, reading the first candidate.
    /// Usage values are running totals; output counts thought tokens.
    /// Returns its deltas, or the error an error chunk carries.
    pub fn event(&mut self, data: &Value) -> Result<Vec<Delta>, ProviderError> {
        if let Some(err) = data.get("error").filter(|e| !e.is_null()) {
            return Err(error_object(err));
        }
        let mut out = Vec::new();
        if let Some(candidate) = data.pointer("/candidates/0") {
            for part in candidate.pointer("/content/parts").and_then(Value::as_array).into_iter().flatten() {
                if part.get("thought").and_then(Value::as_bool) == Some(true) {
                    continue;
                }
                out.extend(text_delta(str_at(part, "/text")));
                if let Some(call) = part.get("functionCall") {
                    let index = self.next_call;
                    self.next_call = index.checked_add(1).ok_or_else(|| decode_error("too many function calls"))?;
                    let id = str_at(call, "/id").filter(|id| !id.is_empty()).map(str::to_owned).unwrap_or_else(|| format!("call_{index}"));
                    out.push(Delta::ToolCallStarted { index, id, name: str_at(call, "/name").unwrap_or_default().to_owned() });
                    let args = call.get("args").cloned().unwrap_or_else(|| json!({}));
                    out.push(Delta::ToolCallArgs { index, json: args.to_string() });
                }
            }
            if let Some(reason) = str_at(candidate, "/finishReason") {
                out.push(Delta::Stop { reason: finish_reason(reason) });
            }
        }
        if str_at(data, "/promptFeedback/blockReason").is_some() {
            out.push(Delta::Stop { reason: StopReason::Refusal });
        }
        if let Some(u) = data.get("usageMetadata") {
            let output = [u64_at(u, "/candidatesTokenCount"), u64_at(u, "/thoughtsTokenCount")]
                .into_iter()
                .flatten()
                .reduce(u64::saturating_add);
            out.push(Delta::Usage { input_tokens: u64_at(u, "/promptTokenCount"), output_tokens: output });
        }
        Ok(out)
    }
}

fn finish_reason(reason: &str) -> StopReason {
    match reason {
        "STOP" => StopReason::EndTurn,
        "MAX_TOKENS" => StopReason::MaxTokens,
        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" | "IMAGE_SAFETY" => StopReason::Refusal,
        other => StopReason::Other(other.to_owned()),
    }
}
```

- [ ] **Step 5: Run, expect pass**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit && kiri run --rm troupekit-test-p3`
Expected: `gemini` tests pass with all earlier tests.

- [ ] **Step 6: Commit**

`git add troupekit && git commit -m "feat(troupekit-providers): gemini generatecontent protocol"`

---
### Task 6: Model trait, Endpoint and HttpModel

**Files:**
- Create: `src/model.rs`, `tests/http_model.rs`
- Modify: `src/lib.rs` (add `pub mod model;`), `tests/common/mod.rs` (add `test_key`, `scripted_model` and their imports, as listed in Task 3 Step 1)

**Interfaces:**
- `model::Model: Send + Sync` (async_trait, object safe): `id(&self) -> &str`, `complete(&self, &CompletionRequest) -> Result<CompletionResponse, ProviderError>`, `stream(&self, &CompletionRequest, &mut (dyn for<'d> FnMut(&'d Delta) + Send)) -> Result<CompletionResponse, ProviderError>`.
- `model::Endpoint { protocol, base_url, auth, quirks }` with `request(&self, model, Option<&Secret>, &CompletionRequest, stream) -> Result<HttpRequest, ProviderError>`.
- `model::HttpModel::new(id, model, Endpoint, Option<Secret>, Arc<dyn Transport>) -> Result<HttpModel, ProviderError>`.
- `model::valid_model_id(&str) -> bool`; `model::MAX_REPLY_BYTES`.

URLs: Anthropic `{base}/v1/messages`; OpenAI and compatible `{base}/chat/completions`; Gemini `{base}/models/{model}:generateContent`, streamed `{base}/models/{model}:streamGenerateContent?alt=sse`. Headers: `content-type: application/json`, `accept: text/event-stream` when streaming, `anthropic-version` for Anthropic, then the auth header.

- [ ] **Step 1: Write the failing tests**

**File:** `troupekit/crates/troupekit-providers/tests/http_model.rs`

```rust
mod common;

use common::*;
use serde_json::Value;
use std::sync::Arc;
use troupekit_core::error::{ProviderError, ProviderErrorKind};
use troupekit_providers::model::{valid_model_id, Endpoint, HttpModel, Model};
use troupekit_providers::request::{CompletionRequest, Delta};
use troupekit_providers::transport::{Scripted, ScriptedTransport};
use troupekit_providers::wire::{anthropic, AuthStyle, Protocol, Quirks};

fn endpoint(protocol: Protocol, auth: AuthStyle) -> Endpoint {
    Endpoint { protocol, base_url: "https://h.test/".into(), auth, quirks: Quirks::default() }
}

#[test]
fn builds_urls_and_auth_headers_per_protocol() {
    let req = CompletionRequest::user("hi", 10);
    let cases = [
        (Protocol::Anthropic, AuthStyle::XApiKey, false, "https://h.test/v1/messages", "x-api-key", "test-key-123"),
        (Protocol::OpenAi, AuthStyle::Bearer, true, "https://h.test/chat/completions", "authorization", "Bearer test-key-123"),
        (Protocol::OpenAiCompatible, AuthStyle::Bearer, false, "https://h.test/chat/completions", "authorization", "Bearer test-key-123"),
        (Protocol::Gemini, AuthStyle::XGoogApiKey, false, "https://h.test/models/m-1:generateContent", "x-goog-api-key", "test-key-123"),
        (Protocol::Gemini, AuthStyle::XGoogApiKey, true, "https://h.test/models/m-1:streamGenerateContent?alt=sse", "x-goog-api-key", "test-key-123"),
    ];
    for (protocol, auth, stream, url, header, value) in cases {
        let r = endpoint(protocol, auth).request("m-1", test_key().as_ref(), &req, stream).unwrap();
        assert_eq!(r.url, url);
        assert_eq!(r.header(header), Some(value));
        assert_eq!(r.header("content-type"), Some("application/json"));
        assert_eq!(r.header("accept").is_some(), stream);
        assert!(!r.url.contains("test-key-123"));
        assert!(!format!("{r:?}").contains("test-key-123"));
    }
    let r = endpoint(Protocol::Anthropic, AuthStyle::XApiKey).request("m", test_key().as_ref(), &req, false).unwrap();
    assert_eq!(r.header("anthropic-version"), Some(anthropic::API_VERSION));
}

#[test]
fn keyless_endpoints_send_no_auth_and_keyed_ones_need_a_key() {
    let req = CompletionRequest::user("hi", 10);
    let r = endpoint(Protocol::OpenAiCompatible, AuthStyle::NoKey).request("m", test_key().as_ref(), &req, false).unwrap();
    assert!(r.header("authorization").is_none());
    let err = endpoint(Protocol::OpenAi, AuthStyle::Bearer).request("m", None, &req, false).unwrap_err();
    assert_eq!(err.kind, ProviderErrorKind::Auth);
}

#[test]
fn rejects_unsafe_model_ids() {
    for bad in ["", "a b", "x?key=1", "../admin", "m#frag", "caf\u{e9}"] {
        assert!(!valid_model_id(bad), "{bad}");
    }
    for good in ["claude-opus-5-5", "anthropic/claude-opus-5.5", "qwen3:8b", "gpt-6.2-sol"] {
        assert!(valid_model_id(good), "{good}");
    }
    let transport = Arc::new(ScriptedTransport::default());
    assert!(HttpModel::new("v/x", "a b", endpoint(Protocol::OpenAi, AuthStyle::Bearer), None, transport).is_err());
}

#[tokio::test]
async fn complete_sends_one_unary_request_and_folds_the_body() {
    let (model, transport) = scripted_model(Protocol::OpenAi, vec![Scripted::body(200, golden_text("openai_reply.json"))]);
    assert_eq!(model.id(), "vendor/m-1");
    assert_eq!(model.complete(&rich_request()).await.unwrap(), expected_reply("toolu_1"));
    let sent = transport.sent();
    assert_eq!(sent.len(), 1);
    let body: Value = serde_json::from_slice(&sent[0].body).unwrap();
    assert_eq!(body["model"], "m-1");
    assert!(body.get("stream").is_none());
}

#[tokio::test]
async fn stream_hands_out_deltas_and_returns_the_unary_reply() {
    let cases = [
        (Protocol::Anthropic, "anthropic_stream.sse", "toolu_1"),
        (Protocol::OpenAi, "openai_stream.sse", "toolu_1"),
        (Protocol::Gemini, "gemini_stream.sse", "call_0"),
    ];
    for (protocol, fixture, call_id) in cases {
        let (model, transport) = scripted_model(protocol, vec![Scripted::chunked(&golden_text(fixture), 5)]);
        let mut seen = Vec::new();
        let reply = model.stream(&rich_request(), &mut |d: &Delta| seen.push(d.clone())).await.unwrap();
        assert_eq!(reply, expected_reply(call_id), "{fixture}");
        assert_eq!(fold_all(&seen), reply, "{fixture}");
        assert_eq!(transport.sent()[0].header("accept"), Some("text/event-stream"));
    }
}

#[tokio::test]
async fn maps_error_statuses_bodies_and_transport_failures() {
    let (model, _) = scripted_model(Protocol::OpenAi, vec![
        Scripted::body(429, r#"{"error":{"message":"slow down"}}"#).with_header("Retry-After", "2"),
        Scripted::body(503, "upstream down"),
        Scripted::body(200, "not json"),
        Scripted::Fail(ProviderError::new(ProviderErrorKind::Stalled, "read timed out")),
    ]);
    let req = CompletionRequest::user("hi", 10);
    let err = model.complete(&req).await.unwrap_err();
    assert_eq!((err.kind, err.retry_after_ms, err.message.as_str()), (ProviderErrorKind::RateLimited, Some(2000), "HTTP 429: slow down"));
    assert_eq!(model.complete(&req).await.unwrap_err().kind, ProviderErrorKind::Server);
    assert_eq!(model.complete(&req).await.unwrap_err().kind, ProviderErrorKind::Decode);
    assert_eq!(model.complete(&req).await.unwrap_err().kind, ProviderErrorKind::Stalled);
}

#[tokio::test]
async fn stream_maps_statuses_and_truncation() {
    let (model, _) = scripted_model(Protocol::Anthropic, vec![
        Scripted::body(401, r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#),
        Scripted::chunked("event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":1}}}\n\n", 4),
    ]);
    let req = CompletionRequest::user("hi", 10);
    let mut sink = |_: &Delta| {};
    let err = model.stream(&req, &mut sink).await.unwrap_err();
    assert_eq!((err.kind, err.message.as_str()), (ProviderErrorKind::Auth, "HTTP 401: invalid x-api-key"));
    assert_eq!(model.stream(&req, &mut sink).await.unwrap_err().kind, ProviderErrorKind::Network);
}

#[tokio::test]
async fn rejects_invalid_or_mismatched_requests_before_sending() {
    let (model, transport) = scripted_model(Protocol::OpenAi, vec![]);
    let mut req = CompletionRequest::user("hi", 0);
    assert_eq!(model.complete(&req).await.unwrap_err().kind, ProviderErrorKind::BadRequest);
    req.max_tokens = 10;
    req.model = Some("other".into());
    assert_eq!(model.complete(&req).await.unwrap_err().kind, ProviderErrorKind::BadRequest);
    assert!(transport.sent().is_empty());
    req.model = Some("m-1".into());
    assert_eq!(model.complete(&req).await.unwrap_err().kind, ProviderErrorKind::Network);
    assert_eq!(transport.sent().len(), 1);
}
```

- [ ] **Step 2: Run, expect failure**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit`
Expected: build fails, `could not find model in troupekit_providers`.

- [ ] **Step 3: Implement**

**File:** `troupekit/crates/troupekit-providers/src/model.rs`

```rust
use crate::fold::Fold;
use crate::request::{CompletionRequest, CompletionResponse, Delta};
use crate::sse::{SseEvent, SseParser};
use crate::status::{bad_request, decode_error, status_error};
use crate::transport::{read_body, HttpRequest, Transport};
use crate::wire::{self, anthropic, AuthStyle, Decoder, Protocol, Quirks};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use troupekit_core::error::{ProviderError, ProviderErrorKind};
use troupekit_core::secret::Secret;

/// Largest reply body or stream, in bytes, a model reads before failing
/// with `Decode`. Bounds memory against a runaway or hostile server.
/// Applies to unary bodies and to the total bytes of a stream.
pub const MAX_REPLY_BYTES: usize = 32 * 1024 * 1024;

/// A chat model behind one object-safe interface. Providers, fakes and
/// middleware all implement it, so callers hold `Box<dyn Model>`.
/// `stream` hands each delta to `on_delta` and returns what `complete` would.
#[async_trait]
pub trait Model: Send + Sync {
    fn id(&self) -> &str;
    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError>;
    async fn stream(
        &self,
        req: &CompletionRequest,
        on_delta: &mut (dyn for<'d> FnMut(&'d Delta) + Send),
    ) -> Result<CompletionResponse, ProviderError>;
}

/// Where and how to reach one vendor: protocol, base URL, auth style and
/// quirks. Catalog vendors turn into one; hosts may also build one for
/// a self-hosted server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub protocol: Protocol,
    pub base_url: String,
    pub auth: AuthStyle,
    pub quirks: Quirks,
}

impl Endpoint {
    /// Builds the HTTP request for one call of wire `model` with `req`:
    /// URL, content and auth headers, encoded body. Pure, no I/O. Returns
    /// `Auth` when a key is needed and `key` is `None`.
    pub fn request(&self, model: &str, key: Option<&Secret>, req: &CompletionRequest, stream: bool) -> Result<HttpRequest, ProviderError> {
        let body = wire::encode(self.protocol, &self.quirks, model, req, stream)?;
        let base = self.base_url.trim_end_matches('/');
        let url = match (self.protocol, stream) {
            (Protocol::Anthropic, _) => format!("{base}/v1/messages"),
            (Protocol::OpenAi | Protocol::OpenAiCompatible, _) => format!("{base}/chat/completions"),
            (Protocol::Gemini, false) => format!("{base}/models/{model}:generateContent"),
            (Protocol::Gemini, true) => format!("{base}/models/{model}:streamGenerateContent?alt=sse"),
        };
        let mut headers = vec![("content-type".to_owned(), "application/json".to_owned())];
        if stream {
            headers.push(("accept".into(), "text/event-stream".into()));
        }
        if self.protocol == Protocol::Anthropic {
            headers.push(("anthropic-version".into(), anthropic::API_VERSION.into()));
        }
        let auth = match (self.auth, key) {
            (AuthStyle::NoKey, _) => None,
            (_, None) => return Err(ProviderError::new(ProviderErrorKind::Auth, "this endpoint needs an API key")),
            (AuthStyle::Bearer, Some(k)) => Some(("authorization", format!("Bearer {}", k.expose()))),
            (AuthStyle::XApiKey, Some(k)) => Some(("x-api-key", k.expose().to_owned())),
            (AuthStyle::XGoogApiKey, Some(k)) => Some(("x-goog-api-key", k.expose().to_owned())),
        };
        if let Some((name, value)) = auth {
            headers.push((name.to_owned(), value));
        }
        let body = serde_json::to_vec(&body).map_err(|e| bad_request(format!("request does not serialize: {e}")))?;
        Ok(HttpRequest { url, headers, body })
    }
}

/// Reports whether `id` is safe to place in a URL path as a model id:
/// 1 to 200 ASCII letters, digits and `._:/-`, with no `..` segment.
/// Catalog and `HttpModel` both check it.
pub fn valid_model_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && !id.contains("..")
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
}

/// A model reached over HTTP through an injected `Transport`.
/// Unary and streamed calls share one encoder, one decoder per protocol
/// and one `Fold`, so both return the same response for the same reply.
pub struct HttpModel {
    id: String,
    model: String,
    endpoint: Endpoint,
    key: Option<Secret>,
    transport: Arc<dyn Transport>,
}

impl HttpModel {
    /// Builds a model named `id` (usually `vendor/model`) that sends wire
    /// `model` to `endpoint` with an optional `key` through `transport`.
    /// Returns `BadRequest` when `model` fails `valid_model_id`.
    pub fn new(
        id: impl Into<String>,
        model: impl Into<String>,
        endpoint: Endpoint,
        key: Option<Secret>,
        transport: Arc<dyn Transport>,
    ) -> Result<Self, ProviderError> {
        let model = model.into();
        if !valid_model_id(&model) {
            return Err(bad_request(format!("model id {model:?} is not allowed")));
        }
        Ok(Self { id: id.into(), model, endpoint, key, transport })
    }

    fn prepare(&self, req: &CompletionRequest, stream: bool) -> Result<HttpRequest, ProviderError> {
        req.validate()?;
        if let Some(asked) = req.model.as_deref().filter(|m| *m != self.model) {
            return Err(bad_request(format!("request names model {asked}, this handle serves {}", self.model)));
        }
        self.endpoint.request(&self.model, self.key.as_ref(), req, stream)
    }
}

#[async_trait]
impl Model for HttpModel {
    fn id(&self) -> &str {
        &self.id
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let request = self.prepare(req, false)?;
        let mut reply = self.transport.send(request).await?;
        let body = read_body(reply.body.as_mut(), MAX_REPLY_BYTES).await?;
        if !(200..300).contains(&reply.status) {
            return Err(status_error(reply.status, &reply.headers, &body));
        }
        let value: Value = serde_json::from_slice(&body).map_err(|e| decode_error(format!("reply is not JSON: {e}")))?;
        let mut fold = Fold::new();
        for delta in wire::decode_body(self.endpoint.protocol, &value)? {
            fold.push(&delta)?;
        }
        fold.finish()
    }

    async fn stream(
        &self,
        req: &CompletionRequest,
        on_delta: &mut (dyn for<'d> FnMut(&'d Delta) + Send),
    ) -> Result<CompletionResponse, ProviderError> {
        let request = self.prepare(req, true)?;
        let mut reply = self.transport.send(request).await?;
        if !(200..300).contains(&reply.status) {
            let body = read_body(reply.body.as_mut(), MAX_REPLY_BYTES).await.unwrap_or_default();
            return Err(status_error(reply.status, &reply.headers, &body));
        }
        let mut parser = SseParser::new();
        let mut decoder = Decoder::new(self.endpoint.protocol);
        let mut fold = Fold::new();
        let mut received = 0usize;
        while let Some(chunk) = reply.body.chunk().await? {
            received += chunk.len();
            if received > MAX_REPLY_BYTES {
                return Err(decode_error("reply stream is too large"));
            }
            apply(parser.push(&chunk)?, &mut decoder, &mut fold, on_delta)?;
        }
        apply(parser.finish()?, &mut decoder, &mut fold, on_delta)?;
        fold.finish()
    }
}

fn apply(
    events: Vec<SseEvent>,
    decoder: &mut Decoder,
    fold: &mut Fold,
    on_delta: &mut (dyn for<'d> FnMut(&'d Delta) + Send),
) -> Result<(), ProviderError> {
    for event in &events {
        for delta in decoder.sse(event)? {
            fold.push(&delta)?;
            on_delta(&delta);
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run, expect pass**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit && kiri run --rm troupekit-test-p3`
Expected: `http_model` tests pass with all earlier tests.

- [ ] **Step 5: Commit**

`git add troupekit && git commit -m "feat(troupekit-providers): model trait, endpoint and http model"`

---
### Task 7: Provider catalog and prices

**Files:**
- Create: `troupekit/catalog/providers.toml`, `src/catalog.rs`, `tests/catalog.rs`
- Modify: `src/lib.rs` (add `pub mod catalog;`)

**Interfaces:**
- `catalog::Catalog { load(&str) -> Result<Catalog, CatalogError>, vendors() -> &[VendorSpec], vendor(id) -> Option<&VendorSpec>, price(selector) -> Result<Price, CatalogError> }`; Task 8 adds `connect_with` and `connect`.
- `catalog::VendorSpec { id, name, protocol, base_url, auth, open_models, quirks, models: Vec<ModelSpec> }` with `endpoint() -> Endpoint`.
- `catalog::ModelSpec { id, context_window, input_usd_micros_per_mtok, output_usd_micros_per_mtok, price_verified, long_context: Option<LongContextPrice> }` with `price() -> Price`.
- `catalog::Price { input_usd_micros_per_mtok, output_usd_micros_per_mtok, verified, long_context }` with `FREE` and `cost_usd_micros(&Usage) -> u64`.
- `catalog::CatalogError { Parse, Invalid, BadSelector, UnknownVendor, UnknownModel, MissingKey, Transport }`.

Catalog TOML schema: `[[vendor]]` tables (`id`, `name`, `protocol`, `base_url`, `auth`, optional `open_models`, optional inline `quirks`), each with `[[vendor.model]]` rows. Validation: vendor ids lowercase letters, digits and dashes, unique; names not blank; base URL `https://`, or `http://` only for keyless vendors; model ids pass `valid_model_id` and are unique per vendor; a long-context threshold is above zero.

Data sources: Anthropic ids, 1M context windows and prices from the claude-api skill model table (cached 2026-11-03): Opus 5.5 $4/$20, Sonnet 5.5 $2/$10, Haiku 5.5 $0.10/$0.50 up to 100K input tokens and $0.50/$2.50 beyond. Other vendors' model ids, context windows, base URLs and quirks from rig `catalog/models.json` and `providers/openai/wire/dialects.rs` (rig `0a4dd16`); their prices are not verified here, so they are 0 with `price_verified = false`.

- [ ] **Step 1: Catalog data**

**File:** `troupekit/catalog/providers.toml`

```toml
[[vendor]]
id = "anthropic"
name = "Anthropic"
protocol = "anthropic"
base_url = "https://api.anthropic.com"
auth = "x-api-key"
quirks = { temperature = false }

[[vendor.model]]
id = "claude-opus-5-5"
context_window = 1000000
input_usd_micros_per_mtok = 4000000
output_usd_micros_per_mtok = 20000000
price_verified = true

[[vendor.model]]
id = "claude-sonnet-5-5"
context_window = 1000000
input_usd_micros_per_mtok = 2000000
output_usd_micros_per_mtok = 10000000
price_verified = true

[[vendor.model]]
id = "claude-haiku-5-5"
context_window = 1000000
input_usd_micros_per_mtok = 100000
output_usd_micros_per_mtok = 500000
price_verified = true
long_context = { above_input_tokens = 100000, input_usd_micros_per_mtok = 500000, output_usd_micros_per_mtok = 2500000 }

[[vendor]]
id = "openai"
name = "OpenAI"
protocol = "openai"
base_url = "https://api.openai.com/v1"
auth = "bearer"
quirks = { max_tokens_field = "max_completion_tokens", temperature = false }

[[vendor.model]]
id = "gpt-6.2-sol"
context_window = 1050000
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor.model]]
id = "gpt-6.1-terra"
context_window = 1050000
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor.model]]
id = "gpt-6-luna"
context_window = 1050000
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor]]
id = "google"
name = "Google Gemini"
protocol = "gemini"
base_url = "https://generativelanguage.googleapis.com/v1beta"
auth = "x-goog-api-key"

[[vendor.model]]
id = "gemini-3.8-flash"
context_window = 1048576
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor.model]]
id = "gemini-3.5-flash-lite"
context_window = 1048576
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor.model]]
id = "gemini-3.1-pro-preview"
context_window = 1048576
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor]]
id = "openrouter"
name = "OpenRouter"
protocol = "openai_compatible"
base_url = "https://openrouter.ai/api/v1"
auth = "bearer"
quirks = { stream_usage = false }

[[vendor.model]]
id = "anthropic/claude-opus-5.5"
context_window = 1000000
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor.model]]
id = "anthropic/claude-haiku-5.5"
context_window = 1000000
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor]]
id = "groq"
name = "Groq"
protocol = "openai_compatible"
base_url = "https://api.groq.com/openai/v1"
auth = "bearer"

[[vendor.model]]
id = "llama-3.3-70b-versatile"
context_window = 131072
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor.model]]
id = "openai/gpt-oss-120b"
context_window = 131072
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor]]
id = "deepseek"
name = "DeepSeek"
protocol = "openai_compatible"
base_url = "https://api.deepseek.com"
auth = "bearer"
quirks = { output_schema = "none" }

[[vendor.model]]
id = "deepseek-v4-pro"
context_window = 1048576
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor.model]]
id = "deepseek-v4-flash"
context_window = 1048576
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor]]
id = "mistral"
name = "Mistral"
protocol = "openai_compatible"
base_url = "https://api.mistral.ai/v1"
auth = "bearer"
quirks = { stream_usage = false }

[[vendor.model]]
id = "mistral-large-latest"
context_window = 1048576
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor.model]]
id = "mistral-small-latest"
context_window = 262144
input_usd_micros_per_mtok = 0
output_usd_micros_per_mtok = 0
price_verified = false

[[vendor]]
id = "ollama"
name = "Ollama (local)"
protocol = "openai_compatible"
base_url = "http://localhost:11434/v1"
auth = "none"
open_models = true

[[vendor]]
id = "lmstudio"
name = "LM Studio (local)"
protocol = "openai_compatible"
base_url = "http://localhost:1234/v1"
auth = "none"
open_models = true
quirks = { stream_usage = false }
```

- [ ] **Step 2: Write the failing tests**

**File:** `troupekit/crates/troupekit-providers/tests/catalog.rs`

```rust
use troupekit_core::event::Usage;
use troupekit_providers::catalog::{Catalog, CatalogError, Price};
use troupekit_providers::wire::{AuthStyle, MaxTokensField, Protocol, SchemaMode};

const CATALOG: &str = include_str!("../../../catalog/providers.toml");

fn usage(input_tokens: u64, output_tokens: u64) -> Usage {
    Usage { input_tokens, output_tokens, cost_usd_micros: 0 }
}

fn vendor(extra: &str) -> String {
    format!("[[vendor]]\nid = \"v\"\nname = \"V\"\nprotocol = \"openai_compatible\"\nbase_url = \"https://v.test/v1\"\nauth = \"bearer\"\n{extra}\n")
}

#[test]
fn shipped_catalog_loads_with_every_required_vendor() {
    let c = Catalog::load(CATALOG).unwrap();
    let ids: Vec<&str> = c.vendors().iter().map(|v| v.id.as_str()).collect();
    for want in ["anthropic", "openai", "google", "openrouter", "groq", "deepseek", "mistral", "ollama", "lmstudio"] {
        assert!(ids.contains(&want), "{want}");
    }
    let anthropic = c.vendor("anthropic").unwrap();
    assert_eq!((anthropic.protocol, anthropic.auth, anthropic.quirks.temperature), (Protocol::Anthropic, AuthStyle::XApiKey, false));
    for m in ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"] {
        assert!(c.price(&format!("anthropic/{m}")).unwrap().verified, "{m}");
    }
    assert_eq!(c.vendor("openai").unwrap().quirks.max_tokens_field, MaxTokensField::MaxCompletionTokens);
    assert_eq!(c.vendor("google").unwrap().protocol, Protocol::Gemini);
    assert_eq!(c.vendor("deepseek").unwrap().quirks.output_schema, SchemaMode::Unsupported);
    for local in ["ollama", "lmstudio"] {
        let v = c.vendor(local).unwrap();
        assert!(v.open_models && v.auth == AuthStyle::NoKey, "{local}");
    }
    for v in c.vendors() {
        for m in &v.models {
            if !m.price_verified {
                assert_eq!((m.input_usd_micros_per_mtok, m.output_usd_micros_per_mtok), (0, 0), "{}/{}", v.id, m.id);
            }
            assert!(m.context_window > 0, "{}/{}", v.id, m.id);
        }
    }
}

#[test]
fn prices_round_up_and_switch_to_long_context_rates() {
    let c = Catalog::load(CATALOG).unwrap();
    let opus = c.price("anthropic/claude-opus-5-5").unwrap();
    assert_eq!(opus.cost_usd_micros(&usage(1_000_000, 1_000_000)), 24_000_000);
    assert_eq!(opus.cost_usd_micros(&usage(1, 0)), 4);
    let haiku = c.price("anthropic/claude-haiku-5-5").unwrap();
    assert_eq!(haiku.cost_usd_micros(&usage(100_000, 0)), 10_000);
    assert_eq!(haiku.cost_usd_micros(&usage(100_001, 0)), 50_001);
    assert_eq!(haiku.cost_usd_micros(&usage(1, 1)), 1);
    assert_eq!(Price::FREE.cost_usd_micros(&usage(u64::MAX, u64::MAX)), 0);
    let max = Price { input_usd_micros_per_mtok: u64::MAX, output_usd_micros_per_mtok: u64::MAX, verified: true, long_context: None };
    assert_eq!(max.cost_usd_micros(&usage(u64::MAX, u64::MAX)), u64::MAX);
}

#[test]
fn resolves_selectors() {
    let c = Catalog::load(CATALOG).unwrap();
    assert!(c.price("openrouter/anthropic/claude-opus-5.5").is_ok());
    assert_eq!(c.price("ollama/qwen3:8b").unwrap(), Price::FREE);
    assert!(matches!(c.price("anthropic"), Err(CatalogError::BadSelector(_))));
    assert!(matches!(c.price("/x"), Err(CatalogError::BadSelector(_))));
    assert!(matches!(c.price("nobody/x"), Err(CatalogError::UnknownVendor(_))));
    assert!(matches!(c.price("anthropic/claude-nope"), Err(CatalogError::UnknownModel(_))));
    assert!(matches!(c.price("ollama/bad id"), Err(CatalogError::BadSelector(_))));
}

#[test]
fn rejects_bad_catalogs() {
    let model = "[[vendor.model]]\nid = \"m\"\ncontext_window = 1\ninput_usd_micros_per_mtok = 0\noutput_usd_micros_per_mtok = 0\nprice_verified = false\n";
    assert!(Catalog::load(&vendor(model)).is_ok());
    let parse = ["vendor = = 1".to_string(), vendor("color = \"red\""), vendor("quirks = { typo = true }"), vendor("protocol2 = 1")];
    for text in parse {
        assert!(matches!(Catalog::load(&text), Err(CatalogError::Parse(_))), "{text}");
    }
    let invalid = [
        format!("{}{}", vendor(""), vendor("")),
        vendor("").replace("id = \"v\"", "id = \"Bad Id\""),
        vendor("").replace("name = \"V\"", "name = \" \""),
        vendor("").replace("https://v.test/v1", "http://v.test/v1"),
        vendor("").replace("https://v.test/v1", "ftp://v.test"),
        vendor(&model.replace("id = \"m\"", "id = \"a b\"")),
        vendor(&format!("{model}{model}")),
        vendor(&format!("{model}long_context = {{ above_input_tokens = 0, input_usd_micros_per_mtok = 1, output_usd_micros_per_mtok = 1 }}\n")),
    ];
    for text in invalid {
        assert!(matches!(Catalog::load(&text), Err(CatalogError::Invalid(_))), "{text}");
    }
    let keyless_http = vendor("").replace("https://v.test/v1", "http://localhost:9/v1").replace("auth = \"bearer\"", "auth = \"none\"");
    assert!(Catalog::load(&keyless_http).is_ok());
}
```

- [ ] **Step 3: Run, expect failure**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit`
Expected: build fails, `could not find catalog in troupekit_providers`.

- [ ] **Step 4: Implement**

**File:** `troupekit/crates/troupekit-providers/src/catalog.rs`

```rust
use crate::model::{valid_model_id, Endpoint};
use crate::wire::{AuthStyle, Protocol, Quirks};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use troupekit_core::event::Usage;

/// Why a catalog could not be loaded or a model could not be connected.
/// Parse and Invalid come from `load`; the rest from selecting and
/// connecting a `vendor/model`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    #[error("catalog is not valid TOML: {0}")]
    Parse(String),
    #[error("invalid catalog: {0}")]
    Invalid(String),
    #[error("model selector must look like vendor/model: {0}")]
    BadSelector(String),
    #[error("unknown vendor: {0}")]
    UnknownVendor(String),
    #[error("unknown model: {0}")]
    UnknownModel(String),
    #[error("vendor {0} needs an API key")]
    MissingKey(String),
    #[error("transport: {0}")]
    Transport(String),
}

/// Higher rates a model charges for a whole call once its input passes
/// `above_input_tokens`, in millionths of a dollar per million tokens.
/// Written inline as `long_context = { ... }` on a catalog model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LongContextPrice {
    pub above_input_tokens: u64,
    pub input_usd_micros_per_mtok: u64,
    pub output_usd_micros_per_mtok: u64,
}

/// A model's prices in millionths of a US dollar per million tokens.
/// `verified` is false when nobody checked them against the vendor's
/// price page: budgets must not read an unverified zero as free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Price {
    pub input_usd_micros_per_mtok: u64,
    pub output_usd_micros_per_mtok: u64,
    pub verified: bool,
    pub long_context: Option<LongContextPrice>,
}

impl Price {
    /// A verified zero price, for local models that cost nothing to call.
    /// Open-model vendors such as Ollama use it for unlisted models.
    /// Every usage costs 0 under it.
    pub const FREE: Price = Price { input_usd_micros_per_mtok: 0, output_usd_micros_per_mtok: 0, verified: true, long_context: None };

    /// Cost of `usage` in millionths of a dollar, rounded up so budgets
    /// never under-count. Long-context rates apply to the whole call once
    /// input passes their threshold; the result saturates at `u64::MAX`.
    pub fn cost_usd_micros(&self, usage: &Usage) -> u64 {
        let (input, output) = match self.long_context {
            Some(lc) if usage.input_tokens > lc.above_input_tokens => (lc.input_usd_micros_per_mtok, lc.output_usd_micros_per_mtok),
            _ => (self.input_usd_micros_per_mtok, self.output_usd_micros_per_mtok),
        };
        let total = (u128::from(usage.input_tokens) * u128::from(input)).saturating_add(u128::from(usage.output_tokens) * u128::from(output));
        u64::try_from(total.div_ceil(1_000_000)).unwrap_or(u64::MAX)
    }
}

/// One model a vendor serves: its wire id, its context window in tokens
/// and its prices. A `[[vendor.model]]` row in the catalog.
/// `price` turns the price fields into a `Price`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSpec {
    pub id: String,
    pub context_window: u64,
    pub input_usd_micros_per_mtok: u64,
    pub output_usd_micros_per_mtok: u64,
    pub price_verified: bool,
    #[serde(default)]
    pub long_context: Option<LongContextPrice>,
}

impl ModelSpec {
    /// Collects this model's price fields into one value.
    /// Returns the price, verified or not as the catalog says.
    /// Used by the cost meter and budget checks.
    pub fn price(&self) -> Price {
        Price {
            input_usd_micros_per_mtok: self.input_usd_micros_per_mtok,
            output_usd_micros_per_mtok: self.output_usd_micros_per_mtok,
            verified: self.price_verified,
            long_context: self.long_context,
        }
    }
}

/// One vendor in the catalog: how to reach it and which models it serves.
/// `open_models` vendors (local servers) also accept model ids the
/// catalog does not list, at a verified zero price.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VendorSpec {
    pub id: String,
    pub name: String,
    pub protocol: Protocol,
    pub base_url: String,
    pub auth: AuthStyle,
    #[serde(default)]
    pub open_models: bool,
    #[serde(default)]
    pub quirks: Quirks,
    #[serde(default, rename = "model")]
    pub models: Vec<ModelSpec>,
}

impl VendorSpec {
    /// Builds the endpoint that reaches this vendor.
    /// Returns its protocol, base URL, auth style and quirks.
    /// Pure; `HttpModel` sends through it.
    pub fn endpoint(&self) -> Endpoint {
        Endpoint { protocol: self.protocol, base_url: self.base_url.clone(), auth: self.auth, quirks: self.quirks }
    }
}

/// The provider catalog: vendors as data, loaded from TOML `[[vendor]]`
/// tables with `[[vendor.model]]` rows. Settings screens list it, and
/// `connect` turns a `vendor/model` choice into a ready `Model`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    #[serde(default, rename = "vendor")]
    vendors: Vec<VendorSpec>,
}

impl Catalog {
    /// Parses and checks a catalog from TOML `text`.
    /// Returns `Parse` for bad TOML or unknown fields, `Invalid` for a
    /// bad id, base URL, repeated entry or zero long-context threshold.
    pub fn load(text: &str) -> Result<Self, CatalogError> {
        let catalog: Catalog = toml::from_str(text).map_err(|e| CatalogError::Parse(e.to_string()))?;
        catalog.validate()?;
        Ok(catalog)
    }

    /// Lists every vendor in catalog order, for a provider dropdown.
    /// Returns the vendors as a slice.
    /// Each carries its models and prices.
    pub fn vendors(&self) -> &[VendorSpec] {
        &self.vendors
    }

    /// Looks up the vendor with `id`.
    /// Returns it, or `None` when the catalog has no such vendor.
    /// Ids match exactly.
    pub fn vendor(&self, id: &str) -> Option<&VendorSpec> {
        self.vendors.iter().find(|v| v.id == id)
    }

    /// Prices `selector`, written `vendor/model`, for budgets and the
    /// cost meter. Returns `BadSelector`, `UnknownVendor` or
    /// `UnknownModel` when the selector names nothing in the catalog.
    pub fn price(&self, selector: &str) -> Result<Price, CatalogError> {
        self.resolve(selector).map(|(_, _, price)| price)
    }

    pub(crate) fn resolve(&self, selector: &str) -> Result<(&VendorSpec, String, Price), CatalogError> {
        let (vendor_id, model_id) = selector
            .split_once('/')
            .filter(|(v, m)| !v.is_empty() && !m.is_empty())
            .ok_or_else(|| CatalogError::BadSelector(selector.to_owned()))?;
        let vendor = self.vendor(vendor_id).ok_or_else(|| CatalogError::UnknownVendor(vendor_id.to_owned()))?;
        if let Some(model) = vendor.models.iter().find(|m| m.id == model_id) {
            return Ok((vendor, model.id.clone(), model.price()));
        }
        match (vendor.open_models, valid_model_id(model_id)) {
            (true, true) => Ok((vendor, model_id.to_owned(), Price::FREE)),
            (true, false) => Err(CatalogError::BadSelector(selector.to_owned())),
            (false, _) => Err(CatalogError::UnknownModel(selector.to_owned())),
        }
    }

    fn validate(&self) -> Result<(), CatalogError> {
        let mut vendor_ids = HashSet::new();
        for v in &self.vendors {
            if v.id.is_empty() || !v.id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
                return invalid(format!("vendor id {:?} must be lowercase letters, digits and dashes", v.id));
            }
            if !vendor_ids.insert(v.id.as_str()) {
                return invalid(format!("vendor {} is listed twice", v.id));
            }
            if v.name.trim().is_empty() {
                return invalid(format!("vendor {} has a blank name", v.id));
            }
            let plain = v.base_url.starts_with("http://");
            if !plain && !v.base_url.starts_with("https://") {
                return invalid(format!("vendor {} base_url must start with https://", v.id));
            }
            if plain && v.auth != AuthStyle::NoKey {
                return invalid(format!("vendor {} would send its key over plain http", v.id));
            }
            let mut model_ids = HashSet::new();
            for m in &v.models {
                if !valid_model_id(&m.id) {
                    return invalid(format!("vendor {} model id {:?} is not allowed", v.id, m.id));
                }
                if !model_ids.insert(m.id.as_str()) {
                    return invalid(format!("vendor {} lists model {} twice", v.id, m.id));
                }
                if m.long_context.is_some_and(|lc| lc.above_input_tokens == 0) {
                    return invalid(format!("vendor {} model {} has a zero long-context threshold", v.id, m.id));
                }
            }
        }
        Ok(())
    }
}

fn invalid(message: String) -> Result<(), CatalogError> {
    Err(CatalogError::Invalid(message))
}
```

- [ ] **Step 5: Run, expect pass**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit && kiri run --rm troupekit-test-p3`
Expected: `catalog` tests pass with all earlier tests.

- [ ] **Step 6: Commit**

`git add troupekit && git commit -m "feat(troupekit-providers): provider catalog with prices"`

---
### Task 8: Middleware and `connect`

**Files:**
- Create: `src/middleware.rs`, `tests/middleware.rs`, `tests/connect.rs`
- Modify: `src/lib.rs` (add `pub mod middleware;`), `src/catalog.rs` (add `connect_with`, `connect`)

**Interfaces:**
- `middleware::Sleep` (async `sleep(ms)`), `middleware::TokioSleep`.
- `middleware::RetryPolicy { max_attempts: u32, base_delay_ms: u64, max_delay_ms: u64 }`, default 3, 500, 30000, with `next_delay_ms(failures, &ProviderError) -> Option<u64>`.
- `middleware::RetryModel::new(Box<dyn Model>, RetryPolicy) -> Result<RetryModel, ProviderError>`, `with_sleep(Arc<dyn Sleep>) -> RetryModel`.
- `middleware::FallbackModel::new(first: Box<dyn Model>, rest: Vec<Box<dyn Model>>) -> FallbackModel`; id is the ids joined by `,`.
- `middleware::MeteredModel::new(Box<dyn Model>, Price) -> MeteredModel`, `price() -> Price`.
- `Catalog::connect_with(&self, selector, Option<Secret>, Arc<dyn Transport>) -> Result<Box<dyn Model>, CatalogError>` returns `MeteredModel(HttpModel)`; `Catalog::connect(&self, selector, Option<Secret>)` (feature `reqwest`) does the same over a new `ReqwestTransport`.

Rules: retry repeats only retryable kinds, at most `max_attempts` tries in all; the delay is the server's `retry_after_ms` when it is at most `max_delay_ms` (larger means give up), else `base_delay_ms * 2^(failures-1)` capped at `max_delay_ms`. Fallback tries each model in order and returns the last error. Retry and fallback on `stream` stop as soon as one delta reached the caller. The meter overwrites `usage.cost_usd_micros` with `Price::cost_usd_micros`.

- [ ] **Step 1: Write the failing tests**

**File:** `troupekit/crates/troupekit-providers/tests/middleware.rs`

```rust
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use troupekit_core::error::{ProviderError, ProviderErrorKind};
use troupekit_core::event::Usage;
use troupekit_core::message::{Message, Role};
use troupekit_providers::catalog::Price;
use troupekit_providers::middleware::{FallbackModel, MeteredModel, RetryModel, RetryPolicy, Sleep};
use troupekit_providers::model::Model;
use troupekit_providers::request::{CompletionRequest, CompletionResponse, Delta, StopReason};

enum Step {
    Reply,
    Fail(ProviderError),
    DeltaThenFail(ProviderError),
}

struct Fake {
    id: String,
    script: Mutex<VecDeque<Step>>,
    calls: Arc<AtomicUsize>,
}

fn fake(id: &str, steps: Vec<Step>) -> (Box<dyn Model>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let model = Fake { id: id.into(), script: Mutex::new(steps.into()), calls: calls.clone() };
    (Box::new(model), calls)
}

fn reply() -> CompletionResponse {
    CompletionResponse {
        message: Message::text(Role::Assistant, "ok"),
        usage: Usage { input_tokens: 1000, output_tokens: 2000, cost_usd_micros: 0 },
        stop: StopReason::EndTurn,
    }
}

impl Fake {
    fn next(&self) -> Step {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.script.lock().unwrap().pop_front().expect("script ran out")
    }
}

#[async_trait]
impl Model for Fake {
    fn id(&self) -> &str {
        &self.id
    }

    async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        match self.next() {
            Step::Reply => Ok(reply()),
            Step::Fail(e) | Step::DeltaThenFail(e) => Err(e),
        }
    }

    async fn stream(&self, _: &CompletionRequest, on_delta: &mut (dyn for<'d> FnMut(&'d Delta) + Send)) -> Result<CompletionResponse, ProviderError> {
        let text = Delta::TextDelta { text: "ok".into() };
        match self.next() {
            Step::Reply => {
                on_delta(&text);
                Ok(reply())
            }
            Step::Fail(e) => Err(e),
            Step::DeltaThenFail(e) => {
                on_delta(&text);
                Err(e)
            }
        }
    }
}

#[derive(Default)]
struct Recorder(Mutex<Vec<u64>>);

#[async_trait]
impl Sleep for Recorder {
    async fn sleep(&self, ms: u64) {
        self.0.lock().unwrap().push(ms);
    }
}

fn err(kind: ProviderErrorKind) -> ProviderError {
    ProviderError::new(kind, "x")
}

fn policy() -> RetryPolicy {
    RetryPolicy { max_attempts: 3, base_delay_ms: 100, max_delay_ms: 5000 }
}

fn retrying(inner: Box<dyn Model>, policy: RetryPolicy) -> (RetryModel, Arc<Recorder>) {
    let sleeps = Arc::new(Recorder::default());
    (RetryModel::new(inner, policy).unwrap().with_sleep(sleeps.clone()), sleeps)
}

fn req() -> CompletionRequest {
    CompletionRequest::user("hi", 10)
}

#[tokio::test]
async fn retries_retryable_errors_with_backoff_and_server_delays() {
    let limited = err(ProviderErrorKind::RateLimited).with_retry_after(1200);
    let (inner, calls) = fake("a", vec![Step::Fail(err(ProviderErrorKind::Server)), Step::Fail(limited), Step::Reply]);
    let (model, sleeps) = retrying(inner, policy());
    assert_eq!(model.complete(&req()).await.unwrap(), reply());
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(*sleeps.0.lock().unwrap(), vec![100, 1200]);
    assert_eq!(model.id(), "a");
}

#[tokio::test]
async fn stops_at_max_attempts_and_on_permanent_errors() {
    let server = || Step::Fail(err(ProviderErrorKind::Server));
    let (inner, calls) = fake("a", vec![server(), server(), server(), Step::Reply]);
    let (model, sleeps) = retrying(inner, policy());
    assert_eq!(model.complete(&req()).await.unwrap_err().kind, ProviderErrorKind::Server);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(*sleeps.0.lock().unwrap(), vec![100, 200]);

    let (inner, calls) = fake("a", vec![Step::Fail(err(ProviderErrorKind::Auth)), Step::Reply]);
    let (model, sleeps) = retrying(inner, policy());
    assert_eq!(model.complete(&req()).await.unwrap_err().kind, ProviderErrorKind::Auth);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(sleeps.0.lock().unwrap().is_empty());

    let too_long = err(ProviderErrorKind::RateLimited).with_retry_after(60_000);
    let (inner, calls) = fake("a", vec![Step::Fail(too_long), Step::Reply]);
    let (model, _) = retrying(inner, policy());
    assert_eq!(model.complete(&req()).await.unwrap_err().retry_after_ms, Some(60_000));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn stream_retries_only_before_the_first_delta() {
    let (inner, calls) = fake("a", vec![Step::Fail(err(ProviderErrorKind::Network)), Step::Reply]);
    let (model, _) = retrying(inner, policy());
    let mut seen = 0;
    assert!(model.stream(&req(), &mut |_: &Delta| seen += 1).await.is_ok());
    assert_eq!((calls.load(Ordering::SeqCst), seen), (2, 1));

    let (inner, calls) = fake("a", vec![Step::DeltaThenFail(err(ProviderErrorKind::Network)), Step::Reply]);
    let (model, _) = retrying(inner, policy());
    let mut seen = 0;
    assert_eq!(model.stream(&req(), &mut |_: &Delta| seen += 1).await.unwrap_err().kind, ProviderErrorKind::Network);
    assert_eq!((calls.load(Ordering::SeqCst), seen), (1, 1));
}

#[test]
fn checks_policies_and_caps_backoff() {
    for bad in [
        RetryPolicy { max_attempts: 0, ..policy() },
        RetryPolicy { max_attempts: 11, ..policy() },
        RetryPolicy { base_delay_ms: 6000, ..policy() },
    ] {
        let (inner, _) = fake("a", vec![]);
        assert_eq!(RetryModel::new(inner, bad).err().unwrap().kind, ProviderErrorKind::BadRequest);
    }
    let wide = RetryPolicy { max_attempts: 100, ..policy() };
    assert_eq!(wide.next_delay_ms(7, &err(ProviderErrorKind::Server)), Some(5000));
    assert_eq!(wide.next_delay_ms(90, &err(ProviderErrorKind::Server)), Some(5000));
    assert_eq!(wide.next_delay_ms(1, &err(ProviderErrorKind::Decode)), None);
}

#[test]
#[should_panic(expected = "before any failure")]
fn a_delay_before_any_failure_is_a_bug() {
    let _ = policy().next_delay_ms(0, &err(ProviderErrorKind::Server));
}

#[tokio::test]
async fn fallback_tries_models_in_order() {
    let (a, a_calls) = fake("a", vec![Step::Fail(err(ProviderErrorKind::Auth)), Step::Fail(err(ProviderErrorKind::Server))]);
    let (b, b_calls) = fake("b", vec![Step::Reply, Step::Fail(err(ProviderErrorKind::RateLimited))]);
    let model = FallbackModel::new(a, vec![b]);
    assert_eq!(model.id(), "a,b");
    assert_eq!(model.complete(&req()).await.unwrap(), reply());
    assert_eq!(model.complete(&req()).await.unwrap_err().kind, ProviderErrorKind::RateLimited);
    assert_eq!((a_calls.load(Ordering::SeqCst), b_calls.load(Ordering::SeqCst)), (2, 2));
}

#[tokio::test]
async fn fallback_stream_stops_after_a_delta() {
    let (a, _) = fake("a", vec![Step::DeltaThenFail(err(ProviderErrorKind::Server))]);
    let (b, b_calls) = fake("b", vec![Step::Reply]);
    let model = FallbackModel::new(a, vec![b]);
    let mut sink = |_: &Delta| {};
    assert_eq!(model.stream(&req(), &mut sink).await.unwrap_err().kind, ProviderErrorKind::Server);
    assert_eq!(b_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn meter_fills_in_the_cost() {
    let price = Price { input_usd_micros_per_mtok: 3_000_000, output_usd_micros_per_mtok: 15_000_000, verified: true, long_context: None };
    let (inner, _) = fake("a", vec![Step::Reply, Step::Reply]);
    let model = MeteredModel::new(inner, price);
    assert_eq!(model.price(), price);
    assert_eq!(model.complete(&req()).await.unwrap().usage.cost_usd_micros, 33_000);
    let mut sink = |_: &Delta| {};
    assert_eq!(model.stream(&req(), &mut sink).await.unwrap().usage.cost_usd_micros, 33_000);
}
```

**File:** `troupekit/crates/troupekit-providers/tests/connect.rs`

```rust
mod common;

use common::*;
use serde_json::Value;
use std::sync::Arc;
use troupekit_core::secret::Secret;
use troupekit_providers::catalog::{Catalog, CatalogError};
use troupekit_providers::request::CompletionRequest;
use troupekit_providers::transport::{Scripted, ScriptedTransport, Transport};

const CATALOG: &str = include_str!("../../../catalog/providers.toml");

fn key() -> Option<Secret> {
    Some(Secret::new("test-key".into()).unwrap())
}

#[tokio::test]
async fn connect_with_returns_a_metered_model_for_the_vendor() {
    let catalog = Catalog::load(CATALOG).unwrap();
    let transport = Arc::new(ScriptedTransport::new(vec![Scripted::body(200, golden_text("anthropic_reply.json"))]));
    let model = catalog.connect_with("anthropic/claude-opus-5-5", key(), transport.clone()).unwrap();
    assert_eq!(model.id(), "anthropic/claude-opus-5-5");
    let mut req = CompletionRequest::user("hi", 64);
    req.temperature = Some(0.3);
    let reply = model.complete(&req).await.unwrap();
    assert_eq!(reply.usage.cost_usd_micros, 648);
    let sent = &transport.sent()[0];
    assert_eq!(sent.url, "https://api.anthropic.com/v1/messages");
    assert_eq!(sent.header("x-api-key"), Some("test-key"));
    let body: Value = serde_json::from_slice(&sent.body).unwrap();
    assert_eq!(body["model"], "claude-opus-5-5");
    assert!(body.get("temperature").is_none());
}

#[tokio::test]
async fn local_vendors_take_unlisted_models_and_send_no_key() {
    let catalog = Catalog::load(CATALOG).unwrap();
    let transport = Arc::new(ScriptedTransport::new(vec![Scripted::body(200, golden_text("openai_reply.json"))]));
    let model = catalog.connect_with("ollama/qwen3:8b", key(), transport.clone()).unwrap();
    assert_eq!(model.complete(&CompletionRequest::user("hi", 64)).await.unwrap().usage.cost_usd_micros, 0);
    let sent = &transport.sent()[0];
    assert_eq!(sent.url, "http://localhost:11434/v1/chat/completions");
    assert!(sent.header("authorization").is_none());
}

#[test]
fn connect_errors_name_the_problem() {
    let catalog = Catalog::load(CATALOG).unwrap();
    let transport = || Arc::new(ScriptedTransport::default()) as Arc<dyn Transport>;
    assert!(matches!(catalog.connect_with("anthropic", key(), transport()), Err(CatalogError::BadSelector(_))));
    assert!(matches!(catalog.connect_with("nobody/x", key(), transport()), Err(CatalogError::UnknownVendor(_))));
    assert!(matches!(catalog.connect_with("anthropic/claude-nope", key(), transport()), Err(CatalogError::UnknownModel(_))));
    assert!(matches!(catalog.connect_with("anthropic/claude-opus-5-5", None, transport()), Err(CatalogError::MissingKey(_))));
    assert!(catalog.connect_with("lmstudio/local-model", None, transport()).is_ok());
    assert!(catalog.connect_with("openrouter/anthropic/claude-opus-5.5", key(), transport()).is_ok());
}

#[cfg(feature = "reqwest")]
#[test]
fn connect_builds_a_reqwest_backed_model_offline() {
    let catalog = Catalog::load(CATALOG).unwrap();
    let model = catalog.connect("google/gemini-3.8-flash", key()).unwrap();
    assert_eq!(model.id(), "google/gemini-3.8-flash");
}
```

- [ ] **Step 2: Run, expect failure**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit`
Expected: build fails, `could not find middleware in troupekit_providers` and `no method named connect_with`.

- [ ] **Step 3: Implement**

**File:** `troupekit/crates/troupekit-providers/src/middleware.rs`

```rust
use crate::catalog::Price;
use crate::model::Model;
use crate::request::{CompletionRequest, CompletionResponse, Delta};
use crate::status::bad_request;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use troupekit_core::error::ProviderError;

/// Waits between retries. `TokioSleep` is the real clock; tests inject a
/// recorder so no test sleeps.
/// `ms` is the wait in milliseconds.
#[async_trait]
pub trait Sleep: Send + Sync {
    async fn sleep(&self, ms: u64);
}

/// Sleeps on the tokio timer, the default for `RetryModel`.
/// Needs a running tokio runtime with the timer enabled.
/// Holds no state.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioSleep;

#[async_trait]
impl Sleep for TokioSleep {
    async fn sleep(&self, ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

/// How often and how long `RetryModel` retries: `max_attempts` tries in
/// all, exponential waits from `base_delay_ms`, never waiting longer than
/// `max_delay_ms`. Defaults to 3 tries, 500 ms base, 30 s cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self { max_attempts: 3, base_delay_ms: 500, max_delay_ms: 30_000 }
    }
}

impl RetryPolicy {
    /// Wait before the next try after `failures` failed tries, the last
    /// ending in `err`. Returns `None` to give up: not retryable, tries
    /// used up, or the server asked to wait past `max_delay_ms`.
    pub fn next_delay_ms(&self, failures: u32, err: &ProviderError) -> Option<u64> {
        assert!(failures >= 1, "asked for a retry delay before any failure");
        if !err.is_retryable() || failures >= self.max_attempts {
            return None;
        }
        match err.retry_after_ms {
            Some(ms) if ms <= self.max_delay_ms => Some(ms),
            Some(_) => None,
            None => Some(self.base_delay_ms.saturating_mul(1u64 << (failures - 1).min(32)).min(self.max_delay_ms)),
        }
    }
}

/// Retries an inner model on transient failures, honouring server retry
/// delays. A stream is retried only while no delta has reached the
/// caller, so a UI never sees a reply twice.
pub struct RetryModel {
    inner: Box<dyn Model>,
    policy: RetryPolicy,
    sleep: Arc<dyn Sleep>,
}

impl RetryModel {
    /// Wraps `inner` with `policy`, waiting on the tokio clock.
    /// Returns `BadRequest` when `max_attempts` is not 1 to 10 or the
    /// base delay is above the max delay.
    pub fn new(inner: Box<dyn Model>, policy: RetryPolicy) -> Result<Self, ProviderError> {
        if !(1..=10).contains(&policy.max_attempts) {
            return Err(bad_request("retry max_attempts must be 1 to 10"));
        }
        if policy.base_delay_ms > policy.max_delay_ms {
            return Err(bad_request("retry base delay is above its max delay"));
        }
        Ok(Self { inner, policy, sleep: Arc::new(TokioSleep) })
    }

    /// Replaces the clock with `sleep`, so tests record waits instead of
    /// waiting. Returns the model.
    /// The policy is unchanged.
    pub fn with_sleep(mut self, sleep: Arc<dyn Sleep>) -> Self {
        self.sleep = sleep;
        self
    }
}

#[async_trait]
impl Model for RetryModel {
    fn id(&self) -> &str {
        self.inner.id()
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let mut failures = 0;
        loop {
            let err = match self.inner.complete(req).await {
                Ok(reply) => return Ok(reply),
                Err(err) => err,
            };
            failures += 1;
            match self.policy.next_delay_ms(failures, &err) {
                Some(ms) => self.sleep.sleep(ms).await,
                None => return Err(err),
            }
        }
    }

    async fn stream(
        &self,
        req: &CompletionRequest,
        on_delta: &mut (dyn for<'d> FnMut(&'d Delta) + Send),
    ) -> Result<CompletionResponse, ProviderError> {
        let mut failures = 0;
        loop {
            let mut emitted = false;
            let result = self
                .inner
                .stream(req, &mut |d: &Delta| {
                    emitted = true;
                    on_delta(d);
                })
                .await;
            let err = match result {
                Ok(reply) => return Ok(reply),
                Err(err) if emitted => return Err(err),
                Err(err) => err,
            };
            failures += 1;
            match self.policy.next_delay_ms(failures, &err) {
                Some(ms) => self.sleep.sleep(ms).await,
                None => return Err(err),
            }
        }
    }
}

/// Tries an ordered list of models and returns the first success, or
/// the last error when all fail. A stream moves on only while no delta
/// has reached the caller.
pub struct FallbackModel {
    id: String,
    models: Vec<Box<dyn Model>>,
}

impl FallbackModel {
    /// Builds a fallback chain from `first` then each of `rest`, so the
    /// chain is never empty. Its id is the model ids joined by commas.
    /// Returns the chain.
    pub fn new(first: Box<dyn Model>, rest: Vec<Box<dyn Model>>) -> Self {
        let mut models = vec![first];
        models.extend(rest);
        let id = models.iter().map(|m| m.id()).collect::<Vec<_>>().join(",");
        Self { id, models }
    }
}

#[async_trait]
impl Model for FallbackModel {
    fn id(&self) -> &str {
        &self.id
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        assert!(!self.models.is_empty(), "a fallback chain always holds a model");
        let mut last = None;
        for model in &self.models {
            match model.complete(req).await {
                Ok(reply) => return Ok(reply),
                Err(err) => last = Some(err),
            }
        }
        Err(last.expect("the loop ran at least once"))
    }

    async fn stream(
        &self,
        req: &CompletionRequest,
        on_delta: &mut (dyn for<'d> FnMut(&'d Delta) + Send),
    ) -> Result<CompletionResponse, ProviderError> {
        assert!(!self.models.is_empty(), "a fallback chain always holds a model");
        let mut last = None;
        for model in &self.models {
            let mut emitted = false;
            let result = model
                .stream(req, &mut |d: &Delta| {
                    emitted = true;
                    on_delta(d);
                })
                .await;
            match result {
                Ok(reply) => return Ok(reply),
                Err(err) if emitted => return Err(err),
                Err(err) => last = Some(err),
            }
        }
        Err(last.expect("the loop ran at least once"))
    }
}

/// Fills in `usage.cost_usd_micros` from catalog prices on every reply.
/// `Catalog::connect` wraps each model in one; `price` exposes the rates
/// and whether they were verified.
pub struct MeteredModel {
    inner: Box<dyn Model>,
    price: Price,
}

impl MeteredModel {
    /// Wraps `inner`, pricing its replies with `price`.
    /// Returns the metered model.
    /// The inner model's id is kept.
    pub fn new(inner: Box<dyn Model>, price: Price) -> Self {
        Self { inner, price }
    }

    /// Returns the price this meter applies, including `verified`, so a
    /// budget can refuse to treat an unverified zero as free.
    /// Never changes after `new`.
    pub fn price(&self) -> Price {
        self.price
    }

    fn priced(&self, mut reply: CompletionResponse) -> CompletionResponse {
        reply.usage.cost_usd_micros = self.price.cost_usd_micros(&reply.usage);
        reply
    }
}

#[async_trait]
impl Model for MeteredModel {
    fn id(&self) -> &str {
        self.inner.id()
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        self.inner.complete(req).await.map(|reply| self.priced(reply))
    }

    async fn stream(
        &self,
        req: &CompletionRequest,
        on_delta: &mut (dyn for<'d> FnMut(&'d Delta) + Send),
    ) -> Result<CompletionResponse, ProviderError> {
        self.inner.stream(req, on_delta).await.map(|reply| self.priced(reply))
    }
}
```

Add to `src/catalog.rs`: imports `crate::middleware::MeteredModel`, `crate::model::{HttpModel, Model}`, `crate::transport::Transport`, `std::sync::Arc`, `troupekit_core::secret::Secret`, and these methods inside `impl Catalog`:

```rust
    /// Connects `selector` (`vendor/model`) through `transport`, using
    /// `key` unless the vendor takes none. Returns a metered `Model`, or
    /// a selector error, or `MissingKey` when a needed key is absent.
    pub fn connect_with(&self, selector: &str, key: Option<Secret>, transport: Arc<dyn Transport>) -> Result<Box<dyn Model>, CatalogError> {
        let (vendor, model, price) = self.resolve(selector)?;
        let key = match vendor.auth {
            AuthStyle::NoKey => None,
            _ => Some(key.ok_or_else(|| CatalogError::MissingKey(vendor.id.clone()))?),
        };
        let http = HttpModel::new(selector, model, vendor.endpoint(), key, transport).map_err(|e| CatalogError::BadSelector(e.message))?;
        Ok(Box::new(MeteredModel::new(Box::new(http), price)))
    }

    /// Connects `selector` like `connect_with`, over a new default
    /// `ReqwestTransport`. Returns `Transport` when the HTTP client
    /// cannot start, else what `connect_with` returns.
    #[cfg(feature = "reqwest")]
    pub fn connect(&self, selector: &str, key: Option<Secret>) -> Result<Box<dyn Model>, CatalogError> {
        let transport = crate::reqwest_transport::ReqwestTransport::new().map_err(|e| CatalogError::Transport(e.message))?;
        self.connect_with(selector, key, Arc::new(transport))
    }
```

- [ ] **Step 4: Run, expect pass**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit && kiri run --rm troupekit-test-p3`
Expected: `middleware` and `connect` tests pass with all earlier tests.

- [ ] **Step 5: Commit**

`git add troupekit && git commit -m "feat(troupekit-providers): retry, fallback and cost meter middleware, catalog connect"`

---

### Task 9: Public surface, docs check, final run

**Files:**
- Modify: `src/lib.rs` (final), `troupekit/README.md` (crate map line)

- [ ] **Step 1: Final `lib.rs`**

**File:** `troupekit/crates/troupekit-providers/src/lib.rs`

```rust
//! Model providers for Troupekit: four wire protocols, a provider
//! catalog, injected transports and middleware behind one `Model` trait.
//! Start with `Catalog::load` and `Catalog::connect("vendor/model", key)`.

pub mod catalog;
pub mod fold;
pub mod middleware;
pub mod model;
#[cfg(feature = "reqwest")]
pub mod reqwest_transport;
pub mod request;
pub mod sse;
pub mod status;
pub mod transport;
pub mod wire;

pub use catalog::{Catalog, CatalogError, Price};
pub use middleware::{FallbackModel, MeteredModel, RetryModel, RetryPolicy};
pub use model::{Endpoint, HttpModel, Model};
pub use request::{CompletionRequest, CompletionResponse, Delta, StopReason};
pub use transport::Transport;
```

- [ ] **Step 2: README crate map**

Add `troupekit-providers` (wire protocols, catalog, transports, middleware) to the crate list in `troupekit/README.md`.

- [ ] **Step 3: Docstring and style check**

Run a check over `troupekit/crates/troupekit-providers/src` that every `///` block is 3 to 5 lines, there is no `//` comment outside docstrings, and no em dash anywhere in `troupekit/`. Fix what it reports.

- [ ] **Step 4: Full run**

Run: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p3 troupekit && kiri run --rm troupekit-test-p3`
Expected: `boundary ok`, every test in `troupekit-core` and `troupekit-providers` passes, no warnings from `troupekit-providers`.

- [ ] **Step 5: Commit**

`git add troupekit && git commit -m "feat(troupekit-providers): public surface and readme"`

---

## Hand-off notes for later pods

- Pod 5 (agent): map `Delta::TextDelta` to `EventBody::TextDelta` and `Delta::ToolCallStarted` to `EventBody::ToolCallStarted`; read `MeteredModel::price().verified` or `Catalog::price(selector)?.verified` before trusting a zero cost.
- Pod 5 and core: `Message` cannot carry thinking blocks, so Anthropic tool loops drop prior thinking (accepted by the API, loses that reasoning). A provider-native passthrough part in core would fix it.
- Gemini synthesized call ids (`call_<n>`) repeat across turns; the encoder resolves each tool result against the latest earlier call with that id.
- Prices for non-Anthropic vendors are unverified zeros; fill them in from the vendors' price pages and set `price_verified = true`.
- After Task 9 the tree is the source of truth: the crate was run through `cargo fmt`, short docstrings were extended to 3 lines and `tests/request.rs` uses fn pointers, so the code blocks above differ from it in form only.
