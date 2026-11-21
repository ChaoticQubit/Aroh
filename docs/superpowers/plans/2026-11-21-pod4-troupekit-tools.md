# Pod 4: troupekit-tools

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `troupekit-tools`, the tool layer of Troupekit: one object-safe `Tool` trait, a registry that validates arguments against each tool's JSON Schema before the call, typed tools from serde structs, an SSRF-guarded HTTP client, a `web_fetch` tool, a `web_search` tool over any number of search instances with fallback, and declarative HTTP tools from TOML.

**Architecture:** Every tool implements `Tool` (`spec() -> &ToolSpec`, async `call(args, &ToolContext)`). `ToolSpec` is the shared `troupekit_core::tool::ToolSpec` (pod 3 commit `7f113e7`, cherry-picked so there is one copy). All network access goes through the `HttpTransport` trait; the real implementation `GuardedHttp` is the salvaged SSRF-safe fetcher (public unicast addresses only, DNS resolved and pinned before connect, every redirect hop re-checked, size and time limits). Tests swap in a fake transport, so no test touches the network; guard tests use a loopback listener inside the test process. All third-party text (fetched pages, search hits, declarative responses) leaves the crate fenced as `<untrusted source="...">...</untrusted>` by one helper.

**Tech Stack:** Rust 2021, async-trait, serde, serde_json, thiserror 2, tokio (net, time), reqwest 0.13 (rustls, http2, same line as the old fetcher), url 2, percent-encoding 2, toml 1, schemars 1, jsonschema 0.60 with default features off.

## Global Constraints

- Lives in `troupekit/`; no file may contain the host app's name (`troupekit/scripts/check-boundary.sh`). This covers fixtures, the lock file and catalog TOML.
- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. No `#` comments in TOML. No em dashes.
- `assert!` for internal invariants; input validation returns `Err`.
- One copy of every helper. Reuse `troupekit_core::{secret::Secret, ids::RunId, tool::ToolSpec}`.
- Dependencies go in `crates/troupekit-tools/Cargo.toml` only; `troupekit/Cargo.toml` is not edited (other pods edit it in parallel; `members = ["crates/*"]` already picks the crate up). The updated `troupekit/Cargo.lock` is committed.
- No live network in tests.
- Tests in Kiri: `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p4 troupekit` then `kiri run --rm troupekit-test-p4`. Inner TDD loop: `kiri run --rm -v "$PWD/troupekit:/src" -v tk-p4-target:/target -v tk-p4-cargo:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/target -w /src rust:1-slim cargo test -p troupekit-tools`.
- Branch `feat/troupekit-tools`, conventional commits, no co-author trailer, commit and never push.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| ToolSpec | `troupekit_core::tool::ToolSpec`, cherry-picked from pod 3 | one copy, shared by requests and tools |
| SecretSource | `troupekit-tools` defines `SecretSource { fn get(&self, name) -> Option<Secret> }`, `EnvSecrets`, and an impl for `HashMap<String, Secret>`. Search keys and declarative header secrets are resolved once, when the toolkit is built | keeps `SearchProvider::search(query, limit)` as specified; per-user keys mean one toolkit per user |
| ToolContext | `ToolContext { run_id: Option<RunId> }`, `Clone + Default`, passed by reference to `Tool::call` and by value to typed closures | typed closures can use it across an `.await` with no lifetime trap |
| Schema validation | `jsonschema` 0.60, `default-features = false`; validators compiled once at register | well tested; with defaults off there is no remote or file `$ref` resolution, so a schema can never cause I/O |
| Err vs `is_error` | `Err(ToolError)`: unknown tool, schema-invalid args, blocked address, transport failure, bad config. `Ok(ToolOutput { is_error: true })`: the remote answered with a non-2xx status (declarative tools). `ToolOutput::from_result` folds any `Err` into an `is_error` output with `err.to_string()` for the agent loop | pod 5 sends both back to the model; the split tells hosts what to log as failures |
| Search fallback | the next instance is tried only when the current one returns `Err`; an empty hit list is a valid answer and stops the fallback | an empty list is not a failure; retrying it hides real "no results" |
| Private addresses | `GuardedHttp::new` is public-only. Address policy injection is `pub(crate)`. A search instance may set `allow_private = true` (default false) for a self-hosted SearXNG; the base URL is operator config, the model only supplies the query string | self-hosted SearXNG usually runs on localhost |
| Redirects | followed for GET only, at most 5, every hop re-resolved and re-checked; a cross-origin hop drops all request headers | stops secret headers leaking to another host |
| Fence | `fence_untrusted(source, text, truncated)` escapes `&`, `<`, `>` in the text and also `"` in the source attribute | fetched text cannot close the fence |
| Declarative URL templates | `{name}` placeholders must be required schema properties and must come after the host's first `/`; values are scalars, percent-encoded, `.` and `..` rejected; leftover args go to the query (GET, DELETE) or a JSON body (POST, PUT, PATCH) | the model can never change the host or walk the path |
| MCP client | out of scope, follow-up pod | per pod 4 brief |

---

## File Structure

| Path | Responsibility |
|---|---|
| `troupekit/crates/troupekit-core/src/tool.rs` | `ToolSpec` (cherry-picked `7f113e7`) |
| `troupekit/crates/troupekit-tools/Cargo.toml` | crate manifest |
| `.../src/lib.rs` | module list and re-exports |
| `.../src/tool.rs` | `Tool`, `ToolContext`, `ToolOutput`, `ToolError` |
| `.../src/secrets.rs` | `SecretSource`, `EnvSecrets` |
| `.../src/registry.rs` | `Toolkit`, `ToolRegistry` |
| `.../src/typed.rs` | `typed_tool`, `TypedTool` |
| `.../src/text.rs` | `html_to_text`, `cap_chars`, `fence_untrusted`, `fence_capped`, `response_text`, `MAX_TEXT_CHARS`; every third-party text path (fetch, search, declarative) ends in `fence_capped` |
| `.../src/http.rs` | `Method`, `HttpRequest`, `HttpResponse`, `HttpTransport`, `HttpLimits`, `GuardedHttp`, `is_public_ip`, test-only `FakeHttp` |
| `.../src/fetch.rs` | `web_fetch` tool |
| `.../src/search.rs` | `SearchHit`, `SearchProvider`, `BraveSearch`, `SearxngSearch`, `SearchToolkit` |
| `.../src/declarative.rs` | `http_toolkit_from_toml` |
| `.../tests/fixtures/brave_web.json`, `brave_no_web.json`, `searxng.json` | response fixtures, shape from the Brave Web Search docs and SearXNG `get_json_response` |
| `troupekit/catalog/search.toml` | default search instances |
| `troupekit/README.md` | `troupekit-tools` row set to available |

---

### Task 1: Crate, Tool trait, registry

**Files:** Create `Cargo.toml`, `src/lib.rs`, `src/tool.rs`, `src/secrets.rs`, `src/registry.rs`.

**Interfaces:**
- `#[async_trait] pub trait Tool: Send + Sync { fn spec(&self) -> &ToolSpec; async fn call(&self, args: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError>; }`
- `pub struct ToolContext { pub run_id: Option<RunId> }` (Debug, Clone, Default)
- `pub struct ToolOutput { pub content: String, pub is_error: bool }` with `text(s)`, `error(s)`, `from_result(Result<ToolOutput, ToolError>) -> ToolOutput`
- `pub enum ToolError { InvalidArgs(String), UnknownTool(String), DuplicateTool(String), InvalidSpec(String), Config(String), Blocked(String), Http(String), Decode(String), Failed(String) }`
- `pub trait SecretSource: Send + Sync { fn get(&self, name: &str) -> Option<Secret>; }`, `pub struct EnvSecrets;`, `impl SecretSource for HashMap<String, Secret>`
- `pub struct Toolkit` with `new(name) -> Toolkit`, `with(self, Arc<dyn Tool>) -> Toolkit`, `name()`, `tools()`
- `pub struct ToolRegistry` with `new()`, `register(Arc<dyn Tool>) -> Result<(), ToolError>`, `register_toolkit(Toolkit) -> Result<(), ToolError>` (all or nothing), `specs() -> Vec<ToolSpec>` (registration order), `get(name) -> Option<Arc<dyn Tool>>`, `async call(name, args, &ToolContext) -> Result<ToolOutput, ToolError>`
- Name rule: 1 to 64 chars of `[A-Za-z0-9_-]`. Schema rule: a JSON object with `"type": "object"` that compiles.

- [ ] **Step 1: Failing tests** in `registry.rs`:
  - `registers_and_lists_specs_in_order`: two echo tools, `specs()` names are `["a", "b"]`.
  - `rejects_duplicate_names`: second `register` of `a` is `Err(DuplicateTool("a"))`.
  - `toolkit_registration_is_all_or_nothing`: toolkit `[c, a]` with `a` present fails and `get("c")` is `None`.
  - `rejects_bad_names_and_schemas`: `""`, `"has space"`, 65 chars, `{"type":"string"}`, `{"type":"object","properties":{"x":{"type":"nope"}}}` are `InvalidSpec`.
  - `remote_ref_is_rejected_without_io`: `{"type":"object","properties":{"x":{"$ref":"http://192.0.2.1/s.json"}}}` is `InvalidSpec` and returns quickly.
  - `validates_args_before_call`: echo tool with required string `q`; `{"q":1}` and `{}` are `InvalidArgs`, the tool's call counter stays 0; `{"q":"hi"}` returns `"hi"`.
  - `unknown_tool_is_an_error`: `call("nope")` is `UnknownTool`.
  - `from_result_folds_errors`: `ToolOutput::from_result(Err(ToolError::Blocked("x".into())))` has `is_error` and content `"blocked: x"`.
  - In `secrets.rs`: `map_source_returns_named_secret`.
- [ ] **Step 2:** Run the inner loop. Expected: compile failure.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run. Expected: all pass.
- [ ] **Step 5:** `git commit -m "feat(troupekit-tools): tool trait and validating registry"`

### Task 2: Typed tools

**Files:** Create `src/typed.rs`.

**Interfaces:**
- `pub fn typed_tool<A, F, Fut>(name: &str, description: &str, f: F) -> TypedTool<A, F>` where `A: DeserializeOwned + JsonSchema + Send + 'static`, `F: Fn(A, ToolContext) -> Fut + Send + Sync + 'static`, `Fut: Future<Output = Result<ToolOutput, ToolError>> + Send + 'static`. The schema is `schemars::schema_for!(A)` without its top-level `$schema` and `title`.

- [ ] **Step 1: Failing tests:**
  - `schema_comes_from_the_struct`: `Args { city: String, days: Option<u8> }` gives `properties.city` and `required == ["city"]`, no `$schema`.
  - `typed_tool_runs_through_registry`: register, call with `{"city":"Oslo"}`, output `"Oslo:none"`; `{"city":3}` is `InvalidArgs` from the registry.
  - `closure_uses_context_across_await`: closure awaits `tokio::task::yield_now()` then reads `ctx.run_id`.
- [ ] **Step 2 to 4:** fail, implement, pass.
- [ ] **Step 5:** `git commit -m "feat(troupekit-tools): typed tools from serde structs"`

### Task 3: Guarded HTTP client and text helpers

**Files:** Create `src/http.rs`, `src/text.rs`.

Salvage: copy `crates/aroh-agents/src/fetch.rs` logic with these renames: `fetch_course` and `fetch_with_policy` become `GuardedHttp::send` with the policy held in the struct; `pinned_addr` stays; `AgentError::Fetch` becomes `ToolError::Http` (and `ToolError::Blocked` for scheme and address refusals); `CourseText`, `pasted_text` and `capped` become `cap_chars`; `html_to_text` and `decode_entities` move to `text.rs` unchanged. Constants: `MAX_TEXT_CHARS = 120_000`, `HttpLimits::default()` = 2 MiB body, 15 s total, 5 redirects.

**Interfaces:**
- `pub enum Method { Get, Post, Put, Patch, Delete }`
- `pub struct HttpRequest { pub method: Method, pub url: String, pub headers: Vec<(String, String)>, pub body: Option<Vec<u8>>, pub accept: Vec<String> }` with `get(url)`; `Debug` prints header names only.
- `pub struct HttpResponse { pub status: u16, pub url: String, pub content_type: String, pub body: Vec<u8>, pub truncated: bool }`
- `#[async_trait] pub trait HttpTransport: Send + Sync { async fn send(&self, req: HttpRequest) -> Result<HttpResponse, ToolError>; }`
- `pub struct GuardedHttp`: `new(HttpLimits)`, `pub(crate) with_policy(HttpLimits, fn(IpAddr) -> bool)`. A 2xx body whose media type is not in `accept` (when `accept` is not empty) is `Err(Http)`. Non-2xx returns `Ok` with the status.
- `pub fn is_public_ip(IpAddr) -> bool`
- `pub fn html_to_text(&str) -> String`, `pub fn cap_chars(&str, usize) -> (String, bool)`, `pub fn fence_untrusted(source: &str, text: &str, truncated: bool) -> String`
- `#[cfg(test)] pub(crate) struct FakeHttp` (queued responses, recorded requests)

- [ ] **Step 1: Failing tests** (salvaged ones keep their bodies, with new names):
  - `blocks_non_public_addresses`, `rejects_bad_schemes_and_private_hosts` (now `Blocked`), `redirect_to_private_address_rejected_per_hop`, `follows_public_redirect`, `too_many_redirects_rejected`.
  - `cross_origin_redirect_drops_headers`: hop 1 on `127.0.0.1:<p1>` redirects to `127.0.0.1:<p2>`; the second server sees no `X-Key` header.
  - `body_is_capped_and_flagged`: 3 KiB body with a 1 KiB limit gives 1024 bytes and `truncated`.
  - `rejects_unaccepted_content_type`: `image/png` with `accept = ["text/html"]` is `Err(Http)`.
  - `non_get_redirect_is_returned_not_followed`.
  - `request_debug_hides_header_values`.
  - In `text.rs`: `strips_html_and_decodes_entities` (salvaged page), `fence_escapes_closing_tag` (`</untrusted>` in text and `"` in source are escaped, output has exactly one closing tag), `cap_chars_counts_chars_not_bytes`.
- [ ] **Step 2 to 4:** fail, implement, pass.
- [ ] **Step 5:** `git commit -m "feat(troupekit-tools): ssrf-guarded http client and text helpers"`

### Task 4: web_fetch tool

**Files:** Create `src/fetch.rs`.

**Interfaces:** `pub fn web_fetch(http: Arc<dyn HttpTransport>, max_chars: usize) -> Arc<dyn Tool>`, name `web_fetch`, args `{ url: String }`, accept `text/html`, `text/plain`. HTML goes through `html_to_text`, the text is capped at `max_chars`, and the output is `fence_untrusted(final_url, text, truncated)`. Non-2xx is `Err(Http("<url>: HTTP <status>"))`.

- [ ] **Step 1: Failing tests** with `FakeHttp`:
  - `fetches_and_fences_html`: page with `<script>` and `</untrusted>` in its text; output starts `<untrusted source="https://example.com/final">`, has no `alert`, one closing tag.
  - `caps_long_pages`: 50 chars with `max_chars = 10` gives `truncated="true"`.
  - `http_error_status_is_err`.
- [ ] **Step 2 to 4:** fail, implement, pass.
- [ ] **Step 5:** `git commit -m "feat(troupekit-tools): web_fetch tool"`

### Task 5: Search providers, SearchToolkit, catalog

**Files:** Create `src/search.rs`, `tests/fixtures/brave_web.json`, `tests/fixtures/searxng.json`, `troupekit/catalog/search.toml`.

**Interfaces:**
- `pub struct SearchHit { pub title: String, pub url: String, pub snippet: String }`
- `#[async_trait] pub trait SearchProvider: Send + Sync { fn name(&self) -> &str; async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, ToolError>; }`
- `BraveSearch::new(name, base_url, key: Secret, http)`: `GET {base}/res/v1/web/search?q=..&count=min(limit,20)`, header `X-Subscription-Token`, maps `web.results[].{title,url,description}`; a missing `web` is no hits; `<strong>` markup in descriptions goes through `html_to_text`.
- `SearxngSearch::new(name, base_url, http)`: `GET {base}/search?q=..&format=json`, maps `results[].{title,url,content}`, skips results without a URL, truncates to `limit` on our side (SearXNG has no count parameter).
- Both: non-2xx is `Err(Http("<name>: HTTP <status>"))`, a truncated or malformed body is `Err(Decode)`.
- `SearchToolkit::new(Vec<Arc<dyn SearchProvider>>) -> Result<Self, ToolError>` (non-empty, unique names), `from_toml(text, &dyn SecretSource, HttpLimits)`, `async search(query, limit)` with fallback on `Err` only, `into_toolkit() -> Toolkit` (toolkit `search`, one tool `web_search`, args `{ query: String, limit: Option<u8> }`, limit 1 to 20, default 5, output fenced with source `web_search`).
- TOML: `[[instance]]` with `name`, `adapter` (`brave` or `searxng`), `base_url`, `key_env` (required for brave, rejected for searxng), `allow_private` (default false). Unknown fields are rejected. An instance whose key env var is missing is skipped and listed by `SearchToolkit::skipped()`; `Err(Config)` naming the variables comes only when no instance is usable (coordinator change before merge).

- [ ] **Step 1: Failing tests:**
  - `brave_maps_fixture_and_sends_key`: recorded request URL has `q=rust+tools` and `count=3`, the header is set, hits come from the fixture with markup stripped.
  - `brave_without_web_block_is_empty`.
  - `searxng_maps_fixture_and_truncates`.
  - `falls_back_on_error_only`: first provider fails, second answers; then the first returns empty and the second is never called.
  - `all_failing_lists_each_instance`.
  - `config_parses_catalog`: `include_str!("../../../catalog/search.toml")` with the Brave key in a map source gives two instances.
  - `config_errors`: unknown adapter, missing key env, `key_env` on searxng, unknown field, empty list.
  - `web_search_tool_fences_hits`: through `ToolRegistry`, a `limit` of 0 is `InvalidArgs`.
- [ ] **Step 2 to 4:** fail, implement, pass.
- [ ] **Step 5:** `git commit -m "feat(troupekit-tools): search providers and web_search toolkit"`

### Task 6: Declarative HTTP tools

**Files:** Create `src/declarative.rs`.

**Interfaces:** `pub fn http_toolkit_from_toml(toolkit: &str, text: &str, secrets: &dyn SecretSource, http: Arc<dyn HttpTransport>) -> Result<Toolkit, ToolError>`. TOML: `[[tool]]` with `name`, `description`, `method`, `url`, optional `headers` table (values may hold `${secret:NAME}`), `schema`. Secrets resolve at load into `Secret`; a missing one is `Err(Config)` naming it. 2xx bodies are fenced (`source` = final URL); non-2xx is `Ok(ToolOutput::error(...))` with the status and fenced body.

- [ ] **Step 1: Failing tests** with `FakeHttp`:
  - `get_fills_template_and_query`: `https://api.example.com/v1/{city}/forecast` with `{"city":"São Paulo/x","days":2}` requests `.../v1/S%C3%A3o%20Paulo%2Fx/forecast?days=2`.
  - `post_sends_leftover_args_as_json`: body is `{"note":"hi"}` and content type JSON.
  - `secret_header_resolved_and_hidden`: the request carries `Bearer k-123`; the tool's Debug and spec do not contain `k-123`.
  - `rejects_dot_segments_and_non_scalars`.
  - `load_errors`: placeholder in the host (`https://{h}.example.com/`), placeholder not required, unknown method, missing secret, non-http scheme.
  - `non_2xx_is_error_output`.
- [ ] **Step 2 to 4:** fail, implement, pass.
- [ ] **Step 5:** `git commit -m "feat(troupekit-tools): declarative http tools from toml"`

### Task 7: Full Kiri run, lock, README, DONE

- [ ] Check `troupekit/Cargo.lock` only adds entries: no existing pin changes.
- [ ] Set the `troupekit-tools` README row to available with its one-line summary.
- [ ] `kiri build -f troupekit/Dockerfile.test -t troupekit-test-p4 troupekit && kiri run --rm troupekit-test-p4`. Expected: `boundary ok` and every test passes.
- [ ] `git commit -m "chore(troupekit-tools): rustfmt and readme row"` (the lock file is committed with Task 1, where the dependencies first resolve)
- [ ] DONE line: `DONE feat/troupekit-tools <base>..<tip> VERIFIED` with counts, boundary output and these follow-ups: MCP client; fold `HttpTransport` with the pod 3 transport; move `SecretSource` to core if pod 3 or 5 needs it; provider-safe schema shaping (rig `schema.rs` style) belongs to pod 3.
