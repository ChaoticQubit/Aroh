# Pod 29: web search and plan limits

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Research uses Claude's server-side web search and fetch (Q63). Plans never go over the owner's daily minutes, and a target that cannot be met gets a card with three choices instead of a plan (Q64). Plans carry session notes, and intake asks what an expert coach needs to know.

**Architecture:** Troupekit gets a generic hosted-tool notion. A `ToolSpec` can say it is hosted (web search, web fetch); only the Anthropic encoder turns it into a server tool. Other wires send the local fallback tool. Hosted reply blocks are kept unchanged as `ContentPart::Hosted`, so `pause_turn` and later turns send them back. Aroh adds the limit check in code: a tool node after `plan` and `revise` expands the draft, sums minutes per local day against the cap from typed answers or the profile, and computes the finish date from the planner's `needed_minutes`. A router then sends the run to approval or to a limits question card.

**Tech Stack:** Rust (troupekit, aroh-server), SvelteKit (questions card), Kiri pg suite.

## Global Constraints

- troupekit stays Aroh-free; no new crates; no live API calls in tests.
- Tool versions `web_search_20250305` and `web_fetch_20250910`: no beta header, every current model.
- Search cap 5 per agent node run, counted from hosted `web_search` calls in the run's messages.
- Search price $10 per 1,000 (10,000 micros per search), added to metered cost and the pre-call estimate.
- No code comments except 3-5 line docstrings; no em dashes; conventional commits; no Co-Authored-By.
- UI changes minimal and off the plan card component (pod 30).

---

### Task 1: Hosted tools in troupekit (Q63)

**Files:** `troupekit-core/src/tool.rs`, `troupekit-core/src/message.rs`, `troupekit-providers/src/{request.rs,fold.rs,wire/*.rs,middleware.rs,catalog.rs}`, `troupekit-agent/src/{run.rs,budget.rs}`, `troupekit-tools/src/registry.rs`, catalog `providers.toml`.

**Interfaces:**
- `ToolSpec.hosted: Option<HostedTool>` (serde default, skipped when none).
- `enum HostedTool { WebSearch { max_uses: u32 }, WebFetch { max_uses: u32, max_content_tokens: u32 } }`.
- `ContentPart::Hosted { tool: Option<String>, block: Value }`, assistant only.
- `Delta::Hosted { index, tool, block }`; `ToolCallArgs` for a hosted index fills `block.input`.
- `StopReason::PauseTurn`.
- `Price.search_usd_micros: u64`; `Message::hosted_calls(name) -> u64`.
- `troupekit_tools::hosted(tool, HostedTool) -> Arc<dyn Tool>`.

- [ ] Red: an Anthropic golden test checks that a hosted spec encodes as `{type, name, max_uses}`, that hosted blocks are sent back unchanged, that other wires skip hosted parts and send the function tool, and that `output_config` is left out when a hosted tool is present.
- [ ] Red: an SSE test with `server_tool_use` plus `input_json_delta`, `web_search_tool_result` and cited text folds into Hosted parts and text; unary gives the same result.
- [ ] Red: agent tests cover a `pause_turn` reply (call again, no repair message), the answer being the text after the last hosted part, the cap (5 minus used, the tool left out at 0), and a blank-reply guard that keeps hosted-only replies.
- [ ] Red: a meter test checks that cost includes searches x price and that the estimate adds `max_uses` x price.
- [ ] Implement, get green, commit `feat(troupekit): hosted web search and fetch tools`.

### Task 2: Aroh research on hosted tools (Q63)

**Files:** `crates/aroh-server/src/io.rs`, `personas/goal-researcher.toml`, `src/engine.rs` (cost display unchanged, check only).

- [ ] Red (lib test): the run registry's `web_search` and `web_fetch` specs are hosted, cap 5, with the local fallback (search kit or stub, client fetch) behind them.
- [ ] Remove the "when search is not available" persona line; keep the guardrail that pages are data. The `research` state reaches the planner inside `fence_untrusted`, the same fence attachments use.
- [ ] Commit `feat(research): Claude server-side web search and fetch (Q63)`.

### Task 3: Daily cap and finish date (Q64)

**Files:** `crates/aroh-server/src/limits.rs` (new), `src/tools.rs` (register `check_limits`), `src/io.rs` (router condition), `workflows/new-goal.toml`, plan personas `[output]`.

**Interfaces:**
- Plan output gains `daily_minutes: int?` and `needed_minutes: int?`.
- Intake reserves question id `daily_minutes` (number, minutes per day); the profile topic is `free_minutes_a_day`.
- `limits::cap(state) -> Option<i64>`: `limit_answers` raise, then answers `daily_minutes`, then a profile fact with that topic, then the plan's `daily_minutes`.
- `limits::check(plan, goal, cap, today) -> Limits { fits, over_days, finish_date, target_date, questions }`.
- State `limits` is intake-shaped (`questions`) plus `fits`; router `flag(limits.fits)`; human node `limit_choice` writes `limit_answers`; then `plan`.

- [ ] Red: three 20 min sessions on one day with a 20 min cap gives over_days; the Dec 14 example says "At 20 min/day you finish on Dec 14, 9 days after your target." and gives three choices; cap precedence works.
- [ ] Implement and wire the workflow, commit `feat(plan): plans never exceed daily minutes (Q64)`.

### Task 4: Limits card in the app (Q64)

**Files:** `apps/aroh/src/lib/app/engine.ts`, `apps/aroh/src/lib/app/chat.svelte.ts`, tests beside them.

- [ ] Red (vitest): `questionsOf` returns the questions of any waiting node whose data holds `{questions}` (intake or limits); submit posts to that node id.
- [ ] Implement, run `pnpm check`, commit `feat(app): limits choice card uses the question card (Q64)`.

### Task 5: Session notes and coach intake

**Files:** plan personas, `personas/intake-interviewer.toml`, `src/evals.rs`.

- [ ] Red: eval `notes_filled(items)` rejects an item with blank notes; eval `coach_questions(questions)` fails shallow fitness, reading and savings question sets.
- [ ] Prompt changes: the planner writes session content in notes and shows BMI and calorie arithmetic in the summary; intake learns from research what an expert needs and asks only that.
- [ ] Commit `feat(plan): session notes on every action` and `feat(intake): ask what an expert coach needs`.

### Task 6: Verify and hand off

- [ ] Run host `cargo test` for the touched crates, vitest, `pnpm check`, and the Kiri pg suite.
- [ ] `git merge master`, `graphify update .`, send DONE.
