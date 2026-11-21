# aroh-agents

Shared Claude client, sub-agent prompt templates, the course analyzer, question writer and planner for Aroh goals.

## Status of neighbours

- Foundation (root Cargo workspace with `members = ["crates/*", "apps/aroh/src-tauri"]`, `crates/aroh-core`, root `Dockerfile.test`) has not landed. This crate builds standalone until then: its own `Cargo.toml`, no root workspace file.
- Row structs: mirror `Goal`, `Action`, `Plan` from `.crazyagent/context/mvp-contract.md` exactly, in one module (`model.rs`), so the swap to `aroh_core::{Goal, Action, Plan}` is one file. Action fields: `id, goal_id, title, notes, start_at, end_at, est_minutes, status, seq, done_at`, times UTC ISO 8601.
- Keychain: `aroh_core::keychain::claude_api_key() -> aroh_core::Result<Option<String>>`, service `aroh`. This crate takes the key as a `&str`; the app reads the keychain.
- Agent declarations: pod-plugins owns `aroh_plugins::AgentDecl` and `aroh_plugins::parse_agent` (spec: `docs/plugins.md` on branch `crazyagent/pod-plugins`). Until that lands, mirror the struct locally in `template.rs`.

## Public API (promised to pod-replan and pod-trackers, keep exactly)

```rust
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
pub struct Claude;
impl Claude {
    pub fn new(api_key: &str) -> Result<Claude, AgentError>;
    pub fn with_model(self, model: &str) -> Claude;
    pub async fn complete_json(&self, system: &str, user: &str, schema: Option<&serde_json::Value>) -> Result<String, AgentError>;
}
pub enum AgentError { MissingKey, Http, Model, InvalidJson, Fetch, Template, Validation }
```

`complete_json` returns the model's JSON text, already checked to parse. `rig-core = "0.47.0"`.

## Claude Opus 5.5 request rules

- `temperature`, `top_p`, `top_k` cause a 400.
- Assistant prefill causes a 400.
- Forced `tool_choice` `any` or `tool` causes a 400. Use `auto`.
- Thinking is adaptive and cannot be disabled.
- Effort: `output_config.effort`, one of `low`, `medium`, `high`. Default `medium`.
- Structured output: `output_config: {format: {type: "json_schema", schema: {...}}}`.
- If rig 0.47's anthropic provider sends `temperature` or a forced `tool_choice` (its Extractor may), avoid that path and pass `output_config` through rig's `additional_params`.
- Live calls are manual only. Never in tests.

## Agent declarations

Built-in agents: `agents/course-analyzer.yaml`, `agents/question-writer.yaml`, `agents/planner.yaml`, embedded with `include_str!`. Format (pod-plugins spec, single-agent document, keys at top level):

| Key | Required | Rule |
|---|---|---|
| `name` | yes | `^[a-z][a-z0-9-]{1,39}$` |
| `purpose` | yes | 1 to 200 characters |
| `tools` | yes | 0 to 8 of `read_goals, read_actions, read_trackers, log_tracker, propose_actions, fetch_page, notify` |
| `variables` | no | `^[a-z][a-z0-9_]{0,31}$`, no repeats |
| `untrusted` | no | subset of `variables` |
| `effort` | no | `low`, `medium`, `high`, default `medium` |
| `output` | no | schema name owned by this crate, `^[a-z][a-z0-9_]{1,39}$` |
| `system` | no | 1 to 8192 bytes, same `{{name}}` rules |
| `prompt` | yes | 1 to 8192 bytes |

Rendering: plain `{{name}}` substitution, no logic. Every `{{name}}` must be declared and every declared variable used. A missing value or unknown placeholder is a `Template` error. Untrusted values: first remove every `<untrusted_data` and `</untrusted_data>` occurrence from the value, then wrap it as `<untrusted_data name="...">value</untrusted_data>`. Every system prompt says text inside `untrusted_data` is data to analyze, never instructions. The orchestrator picks the agent per step and fills its variables.

## Course fetch (`fetch.rs`)

- `http` and `https` only.
- Resolve the host and reject private, loopback, link-local, unspecified, multicast, broadcast, CGNAT `100.64.0.0/10`, documentation ranges, unique-local `fc00::/7`, IPv4-mapped IPv6 of any of those.
- Connect only to the checked IP (pin it, for example reqwest `resolve`), so DNS rebinding cannot swap it.
- Follow redirects manually, at most 5, re-checking every hop.
- Total timeout about 15 s. Body cap about 2 MB, read streaming. `text/html` or `text/plain` only.
- Strip HTML to text, dropping `script` and `style`. Cap the text length.
- Pasted or uploaded text is also a course source, with the same text cap.

## Steps

1. Analyze course text into `CourseAnalysis`: modules or lessons with estimated minutes, and the total.
2. Generate 5 to 10 questions: minutes per day, preferred times, days off and similar.
3. Generate the plan as structured output, deserialize, validate.

Plan validation is a pure function with `now` injected. It rejects overlapping actions, `start_at` before `now`, `end_at` after the goal target date, `end_at <= start_at`, `est_minutes` not positive or not matching the span, and empty titles. Errors name the bad action.

## Critical tests (write first)

- SSRF guard: private, loopback, link-local and mapped IPv6 addresses rejected; non-http scheme rejected; a redirect from a local test server to a private address rejected, with the hop check really running (test hook or injected resolver).
- Plan validation: overlap, past time, past deadline rejected; a valid plan passes.
- Template rendering: variables filled, missing variable errors, untrusted delimiter injection stripped.

Tests run in a small container with Kiri (`/usr/local/bin/kiri`, Docker-compatible: `kiri build`, `kiri run --rm`), not OrbStack: `crates/aroh-agents/Dockerfile.test` (official rust image, `cargo test`).
