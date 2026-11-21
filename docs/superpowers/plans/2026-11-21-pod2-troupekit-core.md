# Pod 2: Troupekit Workspace and troupekit-core

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A standalone `troupekit/` Cargo workspace with `troupekit-core`: the shared types (messages, secrets, errors, events, ids, clock) and the storage traits with in-memory implementations that every other Troupekit crate builds on.

**Architecture:** `troupekit/` is its own workspace, excluded from the Aroh root workspace, so it can be published and later split into its own repo. `troupekit-core` has no I/O. Storage is a set of object-safe async traits (`CheckpointStore`, `MemoryLog`, `VectorStore`, `BlobStore`); apps implement them on their own databases, and core ships in-memory versions for tests and examples. Designs follow `scratchpad/framework-refs.md`: rig's content-part message model and redacted `Secret`, rig-memory's conditional append, AutoAgents' single event enum.

**Tech Stack:** Rust 2021, serde, serde_json, thiserror 2, uuid v7, async-trait, tokio (tests only).

## Global Constraints

- License `MIT OR Apache-2.0`. Crate names `troupekit-<layer>`.
- Zero references to any `aroh` crate or the word `aroh` inside `troupekit/` (checked by `troupekit/scripts/check-boundary.sh`).
- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. No em dashes.
- `assert!` for internal invariants that should be impossible; input validation returns `Err`.
- One copy of every helper inside the workspace.
- Tests run in Kiri: `HOME=/Users/chaoticqubit kiri build -f troupekit/Dockerfile.test -t troupekit-test troupekit` then `HOME=/Users/chaoticqubit kiri run --rm troupekit-test`.
- Branch `feat/troupekit-core` in its own worktree. Conventional commits.

---

## File Structure

| Path | Responsibility |
|---|---|
| `troupekit/Cargo.toml` | workspace, shared dependency versions, license |
| `troupekit/README.md` | what Troupekit is, crate map |
| `troupekit/LICENSE-MIT`, `troupekit/LICENSE-APACHE` | licenses |
| `troupekit/Dockerfile.test` | container that runs boundary check and tests |
| `troupekit/scripts/check-boundary.sh` | fails if `aroh` appears in the workspace |
| `troupekit/crates/troupekit-core/src/lib.rs` | module list and re-exports |
| `.../src/error.rs` | `CoreError`, `ProviderError`, `ProviderErrorKind` |
| `.../src/secret.rs` | `Secret` |
| `.../src/message.rs` | `Role`, `ContentPart`, `MediaSource`, `Message` |
| `.../src/ids.rs` | `RunId` |
| `.../src/clock.rs` | `Clock`, `SystemClock`, `FixedClock` |
| `.../src/event.rs` | `Usage`, `Event`, `EventBody`, `PauseReason` |
| `.../src/store.rs` | storage traits and their value types |
| `.../src/inmem.rs` | in-memory implementations of every store trait |
| `Cargo.toml` (repo root) | add `exclude = ["troupekit"]` |

---

### Task 1: Workspace scaffold and boundary check

**Files:**
- Create: `troupekit/Cargo.toml`, `troupekit/README.md`, `troupekit/LICENSE-MIT`, `troupekit/LICENSE-APACHE`, `troupekit/Dockerfile.test`, `troupekit/scripts/check-boundary.sh`, `troupekit/crates/troupekit-core/Cargo.toml`, `troupekit/crates/troupekit-core/src/lib.rs`
- Modify: `Cargo.toml` (root)

- [ ] **Step 1: Root workspace excludes Troupekit**

In the root `Cargo.toml`, under `[workspace]`, add the line `exclude = ["troupekit"]`.

- [ ] **Step 2: `troupekit/Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
edition = "2021"
version = "0.1.0"
license = "MIT OR Apache-2.0"
repository = "https://github.com/ChaoticQubit/troupekit"
rust-version = "1.85"

[workspace.dependencies]
troupekit-core = { path = "crates/troupekit-core", version = "0.1.0" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v7", "serde"] }
async-trait = "0.1"
tokio = { version = "1", features = ["rt", "macros", "sync"] }
```

- [ ] **Step 3: `troupekit/crates/troupekit-core/Cargo.toml`**

```toml
[package]
name = "troupekit-core"
description = "Core types and storage traits for the Troupekit agent framework."
edition.workspace = true
version.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
uuid.workspace = true
async-trait.workspace = true
tokio.workspace = true

[dev-dependencies]
tokio = { workspace = true, features = ["rt-multi-thread", "macros"] }
```

- [ ] **Step 4: `lib.rs` placeholder module list**

```rust
//! Core types and storage traits shared by every Troupekit crate.
//! No I/O lives here: providers, tools and hosts build on these types.

pub mod clock;
pub mod error;
pub mod event;
pub mod ids;
pub mod inmem;
pub mod message;
pub mod secret;
pub mod store;
```

Create each listed file as an empty file so the crate compiles; later tasks fill them.

- [ ] **Step 5: Boundary check script**

`troupekit/scripts/check-boundary.sh`:

```sh
#!/bin/sh
set -eu
root="$(cd "$(dirname "$0")/.." && pwd)"
if grep -rniI --exclude-dir=target --exclude-dir=.git 'aroh' "$root"; then
  echo "troupekit must not reference aroh" >&2
  exit 1
fi
echo "boundary ok"
```

Run: `chmod +x troupekit/scripts/check-boundary.sh && troupekit/scripts/check-boundary.sh`
Expected: `boundary ok`.

- [ ] **Step 6: Dockerfile**

Create `troupekit/.dockerignore` containing the single line `target`.

`troupekit/Dockerfile.test`:

```dockerfile
FROM rust:1-slim
WORKDIR /src
COPY . .
RUN sh scripts/check-boundary.sh && cargo test --workspace --no-run
CMD ["sh", "-c", "sh scripts/check-boundary.sh && cargo test --workspace"]
```

- [ ] **Step 7: README and licenses**

`README.md`: one paragraph ("Troupekit is a Rust framework for building LLM agents from persona TOML files: providers, tools, memory, RAG and durable graph workflows.") and a crate table listing `troupekit-core` now and the planned crates `providers`, `tools`, `context`, `agent`, `graph`, `plugins`, `host`. Licenses: the standard MIT text with "Copyright (c) 2026 Troupekit contributors" and the standard Apache 2.0 text.

- [ ] **Step 8: Build in Kiri**

Run: `HOME=/Users/chaoticqubit kiri build -f troupekit/Dockerfile.test -t troupekit-test troupekit && HOME=/Users/chaoticqubit kiri run --rm troupekit-test`
Expected: `boundary ok`, then `test result: ok. 0 passed`.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml troupekit
git commit -m "feat(troupekit): workspace scaffold and boundary check"
```

### Task 2: Errors and Secret

**Files:**
- Modify: `troupekit/crates/troupekit-core/src/error.rs`, `troupekit/crates/troupekit-core/src/secret.rs`

**Interfaces:**
- Produces: `CoreError::{InvalidMessage(String), Conflict { expected: u64, actual: u64 }, DimensionMismatch { expected: usize, actual: usize }, NotFound(String), Store(String)}`; `ProviderError { kind: ProviderErrorKind, message: String, retry_after_ms: Option<u64> }` with `ProviderError::new(kind, message)`, `.with_retry_after(ms)`, `.is_retryable() -> bool`; `ProviderErrorKind::{RateLimited, Auth, BadRequest, Server, Network, Stalled, Decode}`; `Secret::new(String) -> Result<Secret, CoreError>`, `Secret::expose(&self) -> &str`.

- [ ] **Step 1: Failing tests at the bottom of `secret.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_prints_or_serializes_the_value() {
        let s = Secret::new("sk-live-123".into()).unwrap();
        assert_eq!(format!("{s:?}"), "Secret([redacted])");
        assert_eq!(serde_json::to_string(&s).unwrap(), "\"[redacted]\"");
        assert_eq!(s.expose(), "sk-live-123");
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        assert!(Secret::new(String::new()).is_err());
        assert!(Secret::new("  ".into()).is_err());
    }

    #[test]
    fn deserializes_from_a_plain_string() {
        let s: Secret = serde_json::from_str("\"abc\"").unwrap();
        assert_eq!(s.expose(), "abc");
    }
}
```

- [ ] **Step 2: Failing tests at the bottom of `error.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_kinds() {
        for k in [ProviderErrorKind::RateLimited, ProviderErrorKind::Server, ProviderErrorKind::Network, ProviderErrorKind::Stalled] {
            assert!(ProviderError::new(k, "x").is_retryable());
        }
        for k in [ProviderErrorKind::Auth, ProviderErrorKind::BadRequest, ProviderErrorKind::Decode] {
            assert!(!ProviderError::new(k, "x").is_retryable());
        }
    }

    #[test]
    fn carries_retry_after() {
        let e = ProviderError::new(ProviderErrorKind::RateLimited, "slow down").with_retry_after(1500);
        assert_eq!(e.retry_after_ms, Some(1500));
        assert_eq!(e.to_string(), "RateLimited: slow down");
    }
}
```

- [ ] **Step 3: Run to see failures**

Run: `cd troupekit && cargo test -p troupekit-core`
Expected: compile errors, `Secret`, `ProviderError` not found.

- [ ] **Step 4: Implement `error.rs`**

```rust
use serde::{Deserialize, Serialize};

/// Errors from core types and storage: bad input, optimistic-lock conflicts,
/// vector dimension mismatches, missing records and backend failures.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CoreError {
    #[error("invalid message: {0}")]
    InvalidMessage(String),
    #[error("conflict: expected version {expected}, found {actual}")]
    Conflict { expected: u64, actual: u64 },
    #[error("vector dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("store error: {0}")]
    Store(String),
}

/// Kind of a failed model or provider call; decides retry behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderErrorKind {
    RateLimited,
    Auth,
    BadRequest,
    Server,
    Network,
    Stalled,
    Decode,
}

/// A failed provider call with an optional server-sent retry delay in milliseconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{kind:?}: {message}")]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub message: String,
    pub retry_after_ms: Option<u64>,
}

impl ProviderError {
    /// Builds an error of `kind` with a human-readable message and no retry delay.
    pub fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), retry_after_ms: None }
    }

    /// Returns the error with a retry delay taken from the provider's response.
    pub fn with_retry_after(mut self, ms: u64) -> Self {
        self.retry_after_ms = Some(ms);
        self
    }

    /// True for transient failures that a retry middleware may repeat.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.kind,
            ProviderErrorKind::RateLimited | ProviderErrorKind::Server | ProviderErrorKind::Network | ProviderErrorKind::Stalled
        )
    }
}
```

- [ ] **Step 5: Implement `secret.rs`**

```rust
use crate::error::CoreError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const REDACTED: &str = "[redacted]";

/// An API key or token. Debug, Display-free and Serialize all print
/// "[redacted]"; only `expose` returns the value, at the point of use.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wraps a non-blank secret value. Blank input is rejected.
    pub fn new(value: String) -> Result<Self, CoreError> {
        if value.trim().is_empty() {
            return Err(CoreError::InvalidMessage("secret is blank".into()));
        }
        Ok(Self(value))
    }

    /// Returns the raw value. Call only where the value is sent to its provider.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secret({REDACTED})")
    }
}

impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(REDACTED)
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Secret::new(raw).map_err(serde::de::Error::custom)
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cd troupekit && cargo test -p troupekit-core`
Expected: 5 passed.

- [ ] **Step 7: Commit**

```bash
git add troupekit
git commit -m "feat(troupekit-core): errors and redacted secret"
```

### Task 3: Messages

**Files:**
- Modify: `troupekit/crates/troupekit-core/src/message.rs`

**Interfaces:**
- Produces: `Role::{System, User, Assistant, Tool}`; `MediaSource::{Url { url: String }, Base64 { data: String }}`; `ContentPart::{Text { text }, Image { media_type, source }, ToolCall { id, name, arguments: serde_json::Value }, ToolResult { call_id, content, is_error }}`; `Message { role, parts }` with `Message::new(role, parts) -> Result<Message, CoreError>`, `Message::text(role, impl Into<String>) -> Message`, `.text_content() -> String`, `.tool_calls() -> impl Iterator<Item = (&str, &str, &serde_json::Value)>`.

- [ ] **Step 1: Failing tests at the bottom of `message.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_shape_is_tagged() {
        let m = Message::text(Role::User, "hi");
        assert_eq!(serde_json::to_value(&m).unwrap(), json!({"role":"user","parts":[{"type":"text","text":"hi"}]}));
    }

    #[test]
    fn round_trips_every_part() {
        let m = Message::new(Role::Assistant, vec![
            ContentPart::Text { text: "checking".into() },
            ContentPart::ToolCall { id: "c1".into(), name: "search".into(), arguments: json!({"q":"protein"}) },
        ]).unwrap();
        let back: Message = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(back, m);
        let calls: Vec<_> = back.tool_calls().collect();
        assert_eq!(calls, vec![("c1", "search", &json!({"q":"protein"}))]);
    }

    #[test]
    fn text_content_joins_text_parts_only() {
        let m = Message::new(Role::User, vec![
            ContentPart::Text { text: "a".into() },
            ContentPart::Image { media_type: "image/png".into(), source: MediaSource::Url { url: "https://x/y.png".into() } },
            ContentPart::Text { text: "b".into() },
        ]).unwrap();
        assert_eq!(m.text_content(), "a\nb");
    }

    #[test]
    fn rejects_bad_part_placement() {
        assert!(Message::new(Role::User, vec![]).is_err());
        let call = ContentPart::ToolCall { id: "c".into(), name: "n".into(), arguments: json!({}) };
        assert!(Message::new(Role::User, vec![call]).is_err());
        let result = ContentPart::ToolResult { call_id: "c".into(), content: "ok".into(), is_error: false };
        assert!(Message::new(Role::Assistant, vec![result.clone()]).is_err());
        assert!(Message::new(Role::Tool, vec![result]).is_ok());
    }
}
```

- [ ] **Step 2: Run to see failures**

Run: `cd troupekit && cargo test -p troupekit-core message`
Expected: compile errors, `Message` not found.

- [ ] **Step 3: Implement `message.rs`**

```rust
use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Who authored a message in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Where image or media bytes come from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MediaSource {
    Url { url: String },
    Base64 { data: String },
}

/// One piece of a message. A message holds several, so text, images,
/// tool calls and tool results are never squeezed into one string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    Image { media_type: String, source: MediaSource },
    ToolCall { id: String, name: String, arguments: Value },
    ToolResult { call_id: String, content: String, is_error: bool },
}

/// A provider-neutral chat message: a role and one or more content parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<ContentPart>,
}

impl Message {
    /// Builds a message, rejecting empty parts, tool calls outside the
    /// assistant role and tool results outside the tool role.
    pub fn new(role: Role, parts: Vec<ContentPart>) -> Result<Self, CoreError> {
        if parts.is_empty() {
            return Err(CoreError::InvalidMessage("message has no parts".into()));
        }
        for part in &parts {
            match part {
                ContentPart::ToolCall { .. } if role != Role::Assistant => {
                    return Err(CoreError::InvalidMessage("tool call outside assistant message".into()))
                }
                ContentPart::ToolResult { .. } if role != Role::Tool => {
                    return Err(CoreError::InvalidMessage("tool result outside tool message".into()))
                }
                _ => {}
            }
        }
        Ok(Self { role, parts })
    }

    /// Builds a single-text message. Text is valid for every role.
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self { role, parts: vec![ContentPart::Text { text: text.into() }] }
    }

    /// Joins the message's text parts with newlines, skipping other parts.
    pub fn text_content(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Yields `(id, name, arguments)` for every tool call in the message.
    pub fn tool_calls(&self) -> impl Iterator<Item = (&str, &str, &Value)> {
        self.parts.iter().filter_map(|p| match p {
            ContentPart::ToolCall { id, name, arguments } => Some((id.as_str(), name.as_str(), arguments)),
            _ => None,
        })
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cd troupekit && cargo test -p troupekit-core`
Expected: 9 passed.

- [ ] **Step 5: Commit**

```bash
git add troupekit
git commit -m "feat(troupekit-core): content-part message model"
```

### Task 4: Ids, clock and events

**Files:**
- Modify: `troupekit/crates/troupekit-core/src/ids.rs`, `clock.rs`, `event.rs`

**Interfaces:**
- Produces: `RunId(Uuid)` with `RunId::new()` (uuid v7), `Display`, serde as a string; `trait Clock: Send + Sync { fn now_ms(&self) -> i64; }`, `SystemClock`, `FixedClock::new(ms)`, `FixedClock::advance(&self, ms: i64)`; `Usage { input_tokens: u64, output_tokens: u64, cost_usd_micros: u64 }` with `Add` and `AddAssign`; `PauseReason::{Approval { node: String }, Question { node: String }, Budget { limit: String }}`; `EventBody::{RunStarted { workflow: String }, StepStarted { step: String }, TextDelta { text: String }, ToolCallStarted { call_id: String, name: String }, ToolCallFinished { call_id: String, is_error: bool }, StepFinished { step: String, usage: Usage }, RunPaused { reason: PauseReason }, RunFinished { usage: Usage }, RunFailed { error: String }}`; `Event { run_id: RunId, seq: u64, at_ms: i64, body: EventBody }`.

- [ ] **Step 1: Failing tests**

At the bottom of `ids.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_time_ordered() {
        let a = RunId::new();
        let b = RunId::new();
        assert_ne!(a, b);
        assert!(a.to_string() < b.to_string());
        let back: RunId = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(back, a);
    }
}
```

At the bottom of `clock.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_advances() {
        let c = FixedClock::new(1_000);
        c.advance(500);
        assert_eq!(c.now_ms(), 1_500);
        assert!(SystemClock.now_ms() > 1_700_000_000_000);
    }
}
```

At the bottom of `event.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn usage_adds() {
        let mut u = Usage { input_tokens: 10, output_tokens: 5, cost_usd_micros: 30 };
        u += Usage { input_tokens: 1, output_tokens: 2, cost_usd_micros: 3 };
        assert_eq!(u, Usage { input_tokens: 11, output_tokens: 7, cost_usd_micros: 33 });
    }

    #[test]
    fn event_json_is_tagged() {
        let id = RunId::new();
        let e = Event { run_id: id, seq: 3, at_ms: 9, body: EventBody::RunPaused { reason: PauseReason::Approval { node: "approve_plan".into() } } };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["body"], json!({"type":"run_paused","reason":{"kind":"approval","node":"approve_plan"}}));
        let back: Event = serde_json::from_value(v).unwrap();
        assert_eq!(back, e);
    }
}
```

- [ ] **Step 2: Run to see failures**

Run: `cd troupekit && cargo test -p troupekit-core`
Expected: compile errors for `RunId`, `FixedClock`, `Event`.

- [ ] **Step 3: Implement `ids.rs`**

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Identifier of one workflow run. UUID v7, so ids sort by creation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(pub Uuid);

impl RunId {
    /// Creates a new time-ordered run id.
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
```

- [ ] **Step 4: Implement `clock.rs`**

```rust
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Source of the current time in Unix milliseconds, injectable for tests.
pub trait Clock: Send + Sync {
    /// Current time in milliseconds since the Unix epoch.
    fn now_ms(&self) -> i64;
}

/// The real wall clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        let d = SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock before 1970");
        i64::try_from(d.as_millis()).expect("time fits in i64")
    }
}

/// A clock that only moves when told to, for deterministic tests.
#[derive(Debug, Default)]
pub struct FixedClock(AtomicI64);

impl FixedClock {
    /// Starts the clock at `ms`.
    pub fn new(ms: i64) -> Self {
        Self(AtomicI64::new(ms))
    }

    /// Moves the clock forward by `ms`, which must not be negative.
    pub fn advance(&self, ms: i64) {
        assert!(ms >= 0, "clock cannot go backwards");
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}
```

- [ ] **Step 5: Implement `event.rs`**

```rust
use crate::ids::RunId;
use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign};

/// Token and cost totals. Cost is in millionths of a US dollar to stay exact.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd_micros: u64,
}

impl Add for Usage {
    type Output = Usage;
    fn add(self, o: Usage) -> Usage {
        Usage {
            input_tokens: self.input_tokens + o.input_tokens,
            output_tokens: self.output_tokens + o.output_tokens,
            cost_usd_micros: self.cost_usd_micros + o.cost_usd_micros,
        }
    }
}

impl AddAssign for Usage {
    fn add_assign(&mut self, o: Usage) {
        *self = *self + o;
    }
}

/// Why a run stopped and waits: a human approval, a human answer, or a budget limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PauseReason {
    Approval { node: String },
    Question { node: String },
    Budget { limit: String },
}

/// What happened in a run. One enum feeds the UI stream, telemetry and checkpoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventBody {
    RunStarted { workflow: String },
    StepStarted { step: String },
    TextDelta { text: String },
    ToolCallStarted { call_id: String, name: String },
    ToolCallFinished { call_id: String, is_error: bool },
    StepFinished { step: String, usage: Usage },
    RunPaused { reason: PauseReason },
    RunFinished { usage: Usage },
    RunFailed { error: String },
}

/// One numbered event of one run. `seq` increases by one per event within a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub run_id: RunId,
    pub seq: u64,
    pub at_ms: i64,
    pub body: EventBody,
}
```

- [ ] **Step 6: Run tests**

Run: `cd troupekit && cargo test -p troupekit-core`
Expected: 13 passed.

- [ ] **Step 7: Commit**

```bash
git add troupekit
git commit -m "feat(troupekit-core): run ids, clock and event model"
```

### Task 5: Storage traits and in-memory stores

**Files:**
- Modify: `troupekit/crates/troupekit-core/src/store.rs`, `troupekit/crates/troupekit-core/src/inmem.rs`
- Test: `troupekit/crates/troupekit-core/tests/stores.rs`

**Interfaces:**
- Produces in `store.rs`:
  - `Checkpoint { run_id: RunId, step: u64, state: serde_json::Value }`
  - `#[async_trait] trait CheckpointStore: Send + Sync { async fn save(&self, cp: Checkpoint) -> Result<(), CoreError>; async fn latest(&self, run_id: RunId) -> Result<Option<Checkpoint>, CoreError>; }`; `save` returns `Conflict` unless `cp.step == last_step + 1` (first step is 0).
  - `#[async_trait] trait MemoryLog: Send + Sync { async fn append(&self, log: &str, expected_len: u64, entries: Vec<Message>) -> Result<u64, CoreError>; async fn read(&self, log: &str) -> Result<Vec<Message>, CoreError>; }`; `append` returns the new length, or `Conflict` when the log length is not `expected_len`.
  - `VectorRecord { id: String, vector: Vec<f32>, text: String, meta: BTreeMap<String, String> }`, `Scored { id: String, score: f32, text: String, meta: BTreeMap<String, String> }`, `Filter(BTreeMap<String, String>)` (all pairs must match).
  - `#[async_trait] trait VectorStore: Send + Sync { fn dims(&self) -> usize; async fn upsert(&self, records: Vec<VectorRecord>) -> Result<(), CoreError>; async fn search(&self, query: &[f32], k: usize, filter: &Filter) -> Result<Vec<Scored>, CoreError>; }`
  - `#[async_trait] trait BlobStore: Send + Sync { async fn put(&self, path: &str, bytes: Vec<u8>) -> Result<(), CoreError>; async fn get(&self, path: &str) -> Result<Vec<u8>, CoreError>; }`; `get` of an unknown path returns `NotFound`.
  - `fn cosine(a: &[f32], b: &[f32]) -> f32` (asserts equal length; 0.0 when either norm is 0).
- Produces in `inmem.rs`: `InMemCheckpoints`, `InMemMemory`, `InMemVectors::new(dims)`, `InMemBlobs`, each `Default` (except `InMemVectors`) and implementing its trait.

- [ ] **Step 1: Failing integration test `tests/stores.rs`**

```rust
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use troupekit_core::error::CoreError;
use troupekit_core::ids::RunId;
use troupekit_core::inmem::{InMemBlobs, InMemCheckpoints, InMemMemory, InMemVectors};
use troupekit_core::message::{Message, Role};
use troupekit_core::store::*;

#[tokio::test]
async fn checkpoints_must_advance_by_one() {
    let s = InMemCheckpoints::default();
    let run = RunId::new();
    s.save(Checkpoint { run_id: run, step: 0, state: json!({"a":1}) }).await.unwrap();
    s.save(Checkpoint { run_id: run, step: 1, state: json!({"a":2}) }).await.unwrap();
    let err = s.save(Checkpoint { run_id: run, step: 1, state: json!({}) }).await.unwrap_err();
    assert_eq!(err, CoreError::Conflict { expected: 2, actual: 1 });
    assert_eq!(s.latest(run).await.unwrap().unwrap().state, json!({"a":2}));
    assert!(s.latest(RunId::new()).await.unwrap().is_none());
}

#[tokio::test]
async fn second_concurrent_writer_conflicts() {
    let m = Arc::new(InMemMemory::default());
    assert_eq!(m.append("g1", 0, vec![Message::text(Role::User, "a")]).await.unwrap(), 1);
    let (a, b) = tokio::join!(
        m.append("g1", 1, vec![Message::text(Role::User, "b")]),
        m.append("g1", 1, vec![Message::text(Role::User, "c")]),
    );
    assert!(a.is_ok() != b.is_ok());
    assert_eq!(m.read("g1").await.unwrap().len(), 2);
}

#[tokio::test]
async fn vector_search_ranks_and_filters() {
    let v = InMemVectors::new(2);
    let meta = |g: &str| BTreeMap::from([("goal".to_string(), g.to_string())]);
    v.upsert(vec![
        VectorRecord { id: "a".into(), vector: vec![1.0, 0.0], text: "protein".into(), meta: meta("g1") },
        VectorRecord { id: "b".into(), vector: vec![0.7, 0.7], text: "sleep".into(), meta: meta("g1") },
        VectorRecord { id: "c".into(), vector: vec![1.0, 0.1], text: "piano".into(), meta: meta("g2") },
    ]).await.unwrap();
    let hits = v.search(&[1.0, 0.0], 2, &Filter(meta("g1"))).await.unwrap();
    assert_eq!(hits.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    assert!(hits[0].score > hits[1].score);
    let err = v.upsert(vec![VectorRecord { id: "d".into(), vector: vec![1.0], text: String::new(), meta: BTreeMap::new() }]).await.unwrap_err();
    assert_eq!(err, CoreError::DimensionMismatch { expected: 2, actual: 1 });
}

#[tokio::test]
async fn blobs_round_trip() {
    let b = InMemBlobs::default();
    b.put("goals/1/plan.pdf", vec![1, 2, 3]).await.unwrap();
    assert_eq!(b.get("goals/1/plan.pdf").await.unwrap(), vec![1, 2, 3]);
    assert!(matches!(b.get("nope").await, Err(CoreError::NotFound(_))));
}

#[test]
fn cosine_basics() {
    assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
    assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
    assert_eq!(cosine(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
}
```

- [ ] **Step 2: Run to see failures**

Run: `cd troupekit && cargo test -p troupekit-core --test stores`
Expected: compile errors, store types not found.

- [ ] **Step 3: Implement `store.rs`**

```rust
use crate::error::CoreError;
use crate::ids::RunId;
use crate::message::Message;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Saved state of a run after one step. Steps start at 0 and rise by one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub run_id: RunId,
    pub step: u64,
    pub state: Value,
}

/// Durable run snapshots. `save` accepts only the next step, so two
/// workers can never both advance the same run.
#[async_trait]
pub trait CheckpointStore: Send + Sync {
    async fn save(&self, cp: Checkpoint) -> Result<(), CoreError>;
    async fn latest(&self, run_id: RunId) -> Result<Option<Checkpoint>, CoreError>;
}

/// Append-only message logs keyed by name. `append` succeeds only when the
/// log still has `expected_len` entries and returns the new length.
#[async_trait]
pub trait MemoryLog: Send + Sync {
    async fn append(&self, log: &str, expected_len: u64, entries: Vec<Message>) -> Result<u64, CoreError>;
    async fn read(&self, log: &str) -> Result<Vec<Message>, CoreError>;
}

/// One embedded chunk with its text and string metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorRecord {
    pub id: String,
    pub vector: Vec<f32>,
    pub text: String,
    pub meta: BTreeMap<String, String>,
}

/// A search hit with its cosine similarity score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scored {
    pub id: String,
    pub score: f32,
    pub text: String,
    pub meta: BTreeMap<String, String>,
}

/// Equality filter on metadata: every pair must match.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Filter(pub BTreeMap<String, String>);

impl Filter {
    /// True when every filter pair is present in `meta` with the same value.
    pub fn matches(&self, meta: &BTreeMap<String, String>) -> bool {
        self.0.iter().all(|(k, v)| meta.get(k) == Some(v))
    }
}

/// Object-safe vector index of fixed dimension, so a store can be chosen at runtime.
#[async_trait]
pub trait VectorStore: Send + Sync {
    fn dims(&self) -> usize;
    async fn upsert(&self, records: Vec<VectorRecord>) -> Result<(), CoreError>;
    async fn search(&self, query: &[f32], k: usize, filter: &Filter) -> Result<Vec<Scored>, CoreError>;
}

/// Byte storage by path, for attached files and media.
#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, path: &str, bytes: Vec<u8>) -> Result<(), CoreError>;
    async fn get(&self, path: &str) -> Result<Vec<u8>, CoreError>;
}

/// Cosine similarity of two equal-length vectors; 0.0 when either is all zeros.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "cosine of unequal lengths");
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na * nb)
}
```

- [ ] **Step 4: Implement `inmem.rs`**

```rust
use crate::error::CoreError;
use crate::ids::RunId;
use crate::message::Message;
use crate::store::*;
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::Mutex;

/// In-memory `CheckpointStore` keeping only the latest checkpoint per run.
#[derive(Default)]
pub struct InMemCheckpoints(Mutex<HashMap<RunId, Checkpoint>>);

#[async_trait]
impl CheckpointStore for InMemCheckpoints {
    async fn save(&self, cp: Checkpoint) -> Result<(), CoreError> {
        let mut map = self.0.lock().await;
        let expected = map.get(&cp.run_id).map_or(0, |c| c.step + 1);
        if cp.step != expected {
            return Err(CoreError::Conflict { expected, actual: cp.step });
        }
        map.insert(cp.run_id, cp);
        Ok(())
    }

    async fn latest(&self, run_id: RunId) -> Result<Option<Checkpoint>, CoreError> {
        Ok(self.0.lock().await.get(&run_id).cloned())
    }
}

/// In-memory `MemoryLog` with the same conditional-append rule as durable stores.
#[derive(Default)]
pub struct InMemMemory(Mutex<HashMap<String, Vec<Message>>>);

#[async_trait]
impl MemoryLog for InMemMemory {
    async fn append(&self, log: &str, expected_len: u64, entries: Vec<Message>) -> Result<u64, CoreError> {
        let mut map = self.0.lock().await;
        let list = map.entry(log.to_string()).or_default();
        let actual = list.len() as u64;
        if actual != expected_len {
            return Err(CoreError::Conflict { expected: expected_len, actual });
        }
        list.extend(entries);
        Ok(list.len() as u64)
    }

    async fn read(&self, log: &str) -> Result<Vec<Message>, CoreError> {
        Ok(self.0.lock().await.get(log).cloned().unwrap_or_default())
    }
}

/// In-memory `VectorStore` doing a full cosine scan; fine for tests and small corpora.
pub struct InMemVectors {
    dims: usize,
    records: Mutex<HashMap<String, VectorRecord>>,
}

impl InMemVectors {
    /// Creates an empty index for vectors of `dims` dimensions (must be positive).
    pub fn new(dims: usize) -> Self {
        assert!(dims > 0, "vector dimension must be positive");
        Self { dims, records: Mutex::new(HashMap::new()) }
    }

    fn check(&self, len: usize) -> Result<(), CoreError> {
        if len == self.dims {
            Ok(())
        } else {
            Err(CoreError::DimensionMismatch { expected: self.dims, actual: len })
        }
    }
}

#[async_trait]
impl VectorStore for InMemVectors {
    fn dims(&self) -> usize {
        self.dims
    }

    async fn upsert(&self, records: Vec<VectorRecord>) -> Result<(), CoreError> {
        for r in &records {
            self.check(r.vector.len())?;
        }
        let mut map = self.records.lock().await;
        for r in records {
            map.insert(r.id.clone(), r);
        }
        Ok(())
    }

    async fn search(&self, query: &[f32], k: usize, filter: &Filter) -> Result<Vec<Scored>, CoreError> {
        self.check(query.len())?;
        let map = self.records.lock().await;
        let mut hits: Vec<Scored> = map
            .values()
            .filter(|r| filter.matches(&r.meta))
            .map(|r| Scored { id: r.id.clone(), score: cosine(query, &r.vector), text: r.text.clone(), meta: r.meta.clone() })
            .collect();
        hits.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
        hits.truncate(k);
        Ok(hits)
    }
}

/// In-memory `BlobStore`.
#[derive(Default)]
pub struct InMemBlobs(Mutex<HashMap<String, Vec<u8>>>);

#[async_trait]
impl BlobStore for InMemBlobs {
    async fn put(&self, path: &str, bytes: Vec<u8>) -> Result<(), CoreError> {
        self.0.lock().await.insert(path.to_string(), bytes);
        Ok(())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>, CoreError> {
        self.0.lock().await.get(path).cloned().ok_or_else(|| CoreError::NotFound(path.to_string()))
    }
}
```

- [ ] **Step 5: Run all tests in Kiri**

Run: `HOME=/Users/chaoticqubit kiri build -f troupekit/Dockerfile.test -t troupekit-test troupekit && HOME=/Users/chaoticqubit kiri run --rm troupekit-test`
Expected: `boundary ok`, 13 unit tests and 5 integration tests pass.

- [ ] **Step 6: Commit and report DONE**

```bash
git add troupekit
git commit -m "feat(troupekit-core): storage traits and in-memory stores"
```

DONE line: `DONE feat/troupekit-core <base>..<tip> VERIFIED` with the Kiri test output summary.
