# Aroh agent decisions

Durable record of decisions settled with the owner in grill-me sessions. Source of truth for planning. A changed decision replaces the old text, with the reason in one line.

## Rebuild architecture (grill 2026-11-21, branch `docs/engine-architecture`)

### Goal

Rebuild Aroh from the MVP into a goal engine for any human goal (course, habit, hobby, fitness, financial, project). The owner defines a goal with context; an orchestrator starts persona agents that research it, ask the owner what they need, design an action plan, build a per-goal view, collect regular data and adapt the plan as the owner acts.

### Constraints

- Phone first, desktop second, built in parallel from one codebase (Tauri 2).
- A Rust engine is the main driver: model provider, context management, agent orchestration, plugin management and deployment layers.
- The agent framework is our own and publishable on its own, with no Aroh code in it.
- Multi-tenant from day one, bring your own keys.

### Decisions

| # | Topic | Decision |
|---|---|---|
| Q1 | Engine host | One Rust engine, two hosts: app engine in the Tauri app, server engine in the cloud. A Supabase job table connects them. Reason: iOS cannot keep an app running in the background (`BGContinuedProcessingTask` is user-initiated and can expire, `BGProcessingTask` is system-scheduled), and agents must work while the app is closed. |
| Q2 | Providers | Any provider. The owner picks provider and model from a dropdown and supplies an API key or credential. Local models included. |
| Q3 | Engine split | All AI work (model calls, web search, RAG, orchestration, adaptation) runs in the server engine. Deterministic work, data, UI, offline capture, reminders and sync run in the app engine. The app talks to a server engine at a configurable URL; for Ollama the owner runs the server engine on the Mac. Keys are entered in the app, sent over TLS, stored encrypted by the server engine and decrypted only per call. The app holds no provider keys. |
| Q4 | Framework | Build our own Rust LLM agent framework. Ideas from rig and AutoAgents (implementation), Agno, LangChain and LangGraph (features). Agents are persona TOML files, testy-style (`../testy/personas/*.toml`: role, goal, instructions, tools, guardrails, evals, typed `[output]`). Not YAML. Reason: no mature Rust framework has declarative agents (anchor-chain: 6 stars, last push 2024-06, no YAML; ai-agents: 39 stars). |
| Q5 | Provider layer | Copy rig 0.47 design: I/O-free wire encode plus per-reply decoder with injected transport, one path for streaming and non-streaming; vendor dialects as data (TOML catalog of providers, protocol, base URL, models, prices); runtime `connect("vendor/model")` returning a type-erased model; rich provider errors with retry-after; redacted `Secret`. From AutoAgents: middleware layers (retry, fallback, cache, guardrail). Rejected: AutoAgents lossy message model, plain-text key store, compile-time provider generics. Reference report: scratchpad `framework-refs.md` (rig `0a4dd16`, AutoAgents `d1f8cc5`). |
| Q6 | Categories | Seeded set (Health and Fitness, Learning, Hobby and Skill, Financial, Career and Projects, Habit) plus owner-defined. One category per goal. The orchestrator suggests, the owner confirms. |
| Q7 | Goal links | Typed links: parent/sub-goal, depends-on (scheduling order), supports (shared context and data), conflicts (shared time). Orchestrator proposes, owner confirms. Used by replanning and retrieval. |
| Q8 | Context inputs | Text and chat, links (fetched, web search when thin), documents (PDF, DOCX, MD, TXT), images (vision model), audio and video (transcription, frames). Raw files in Supabase Storage, chunks in pgvector. |
| Q9 | Personas | Built-in persona library plus personas the orchestrator generates per goal, schema-validated. Lookup: goal folder, then user library, then built in. Owner can view and edit any persona. |
| Q10 | Goal folder | Virtual folder: structured parts as DB rows, files in Supabase Storage under `goals/<id>/`, exposed by the engine as a folder API. |
| Q11 | Orchestration | Durable graph, LangGraph-style, defined in TOML: steps, tools, human-approval nodes, parallel fan-out and join. Each run is a serializable state machine (rig `AgentRun` style), checkpointed to Postgres after every step, resumable after crash or deploy, paused at approval nodes. |
| Q12 | Memory | Four scopes: run history (append-only log, rig-memory style), goal knowledge (pgvector per goal), user profile memory (facts with source and date, editable, shared across goals), linked-goal retrieval. Hybrid vector plus full-text retrieval with recency weighting. |
| Q13 | Embeddings | Local open embedding model (ONNX) inside the server engine. A model change runs a re-embed job. |
| Q14 | Web search | Agno-style search toolkits: a `SearchProvider` trait with many adapters (Brave, Tavily, Exa, SearXNG, provider-native search, others), any number of configured instances with their own keys or self-hosted URLs, fallback order, configured in TOML or settings. Results pass the fetch tool (SSRF guard, size limits, untrusted-content fencing). |
| Q15 | Tools | One `Tool` trait, three sources: Rust toolkits (typed, derive-macro schemas), MCP client (stdio and HTTP), declarative HTTP tools in TOML. App tools register as toolkits. |
| Q16 | Plugins | A plugin is a TOML bundle: tracker schemas, a view layout of our components bound to those trackers, optional personas and tools. Agents generate it, the owner edits or reuses it, the plugin manager validates, versions and enables it. No plugin code. |
| Q17 | Components | Four families in the first library: inputs, data display, planning, agent surfaces. |
| Q18 | UI base | Tauri 2, Svelte 5, shadcn-svelte (Bits UI), a Svelte chart library. UI decoupled: talks to the app engine only through a typed command API; plugin views are renderer-neutral TOML component trees so the renderer can be replaced. Rejected: Rune (anomalyco/rune, created 2026-10-22, pre-release, desktop first, early iOS, no Android, no charts or pickers). |
| Q19 | Autonomy | Per-goal level: Ask, Balanced (default: small changes auto-apply with undo, big ones ask), Auto (all apply with undo plus daily digest). Hand-edited or locked actions are never moved without asking. |
| Q20 | Adaptation triggers | Action events, tracker rules, scheduled reviews (server, app closed), owner messages. |
| Q21 | Data capture | Scheduled check-ins per tracker (cadence set by persona, question card plus push), free-form logging (text, chips, voice, photo) mapped by the intake agent, Apple Health import on iPhone. No bank sync. |
| Q22 | Notifications | Server push (APNs now, FCM with Android) plus the next 24 h scheduled locally as offline fallback. |
| Q23-24 | External calendars | Read device calendars via EventKit, no write-back. On connect, titles, times and recurrence go to the server; a triage agent groups events, the owner keeps or ignores per group, the result becomes rules. Kept events sync as busy blocks with their group; new events match rules on device, only unmatched ones go to the agent. |
| Q25 | Users | Multi-tenant from day one: Supabase Auth, every row, file, vector and job scoped by `user_id` with RLS, per-job tenant isolation, per-user rate limits, BYOK. |
| Q26-27 | Hosting | Deployment layer with host adapters, switchable between providers. One engine core, two run modes: worker (always-on loop: Mac, Fly, VPS) and step (each checkpointed graph step is one request, triggered by Supabase `pg_cron` plus `pg_net` and enqueue webhooks: Vercel, Cloud Run, Lambda). First host: Vercel step mode (Rust runtime or Container runtime). |
| Q28 | App to server | Typed JSON HTTPS API (OpenAPI from Rust types) authed with the Supabase user JWT. Run events written to a `run_events` table, streamed to the app through Supabase Realtime. |
| Q29 | Migration | Rebuild in this repo, salvage proven parts with their tests: HLC plus LWW sync plus RLS, deterministic replan planner (becomes a tool), SSRF-safe fetch, Apple Speech plugin, reminder timing. Old YAML agent steps, YAML slot plugins and old screens are deleted once replaced. |
| Q30 | Framework layout | `troupekit/` is its own Cargo workspace of small crates, own README, CI, versioning, crates.io publishing, zero Aroh imports enforced by CI. Aroh depends on it by path, later by version. |
| Q31 | License | MIT OR Apache-2.0. |
| Q32 | Spend | Layered budgets: per call, per run, per goal per month, per user per month, priced from the provider catalog. Crossing a limit pauses at an approval node. Settings shows spend per goal and provider. |
| Q33 | Guardrails | testy-style per persona: strict typed output with one repair round, deterministic evals (no overlap, before deadline, within availability, daily cap), optional LLM-judge evals, guardrails (allowed tools, public URLs only, health and finance safety rules). A failed eval blocks the output. |
| Q34 | Recurrence | Action series with an RFC 5545 RRULE, duration and preferred time; rows materialized for a rolling window (for example 14 days); "this one" vs "this and following" edits split the series. |
| Q35 | Offline | Everything except AI works offline on local SQLite. AI work queues as jobs. LWW sync on reconnect. |
| Q36 | New goal UX | One input (text or voice plus attachments), chat thread with structured question cards, live research progress, then plan, trackers and dashboard for approval. The first plan always needs approval. |
| Q37 | Learning from edits | An analysis persona proposes a preference after 3+ similar edits; confirmed preferences go to user profile memory. A single edit only locks that action. |
| Q38 | Screens | Five tabs: Today, Calendar (filter by category and goal), Goals (plugin dashboard plus persona chat), Inbox (proposals, suggestions, triage, run progress), Log. Settings behind the avatar. |
| Q39 | Project workflow | Owner rule for this project: every ticket starts with a mockup written in real Svelte code. Owner reviews and iterates, then approves. Approved mockup code is ported into the app, not rewritten. Tests run in containers (Kiri). One pod per feature: PM plans and asks the owner, one engineer sub-agent writes tests, code and does the testing, pushes to its own branch, reports done. The leader takes a quick look and merges. No CodeRabbit, security or code reviews on this project; this overrides global gate 6 here. |
| Q40 | First slice | Framework spine (4 provider protocols plus catalog, fetch and search trait with 2 adapters, durable graph with checkpoints, persona TOML loader, memory and RAG on pgvector, step mode on Vercel) plus app (tokens and gallery for the four component families, Settings for provider, model and key, New Goal chat, Goals, Today, Calendar, Inbox), proved end to end on one fitness goal ("six pack by Feb 1") with check-ins and adaptation. Health, calendar triage, push and other categories follow. |

| Q41 | Visual direction | Simple, professional, enterprise grade, intuitive. Calendar-first layout in the style of a plain web calendar, with cards layered on top. Neutral palette, one accent, no flair. |
| Q42 | Code shape | Simple modules. One copy of every function: shared logic lives in one common module (`troupekit-core`, `aroh-domain`, `apps/aroh/src/lib/ui`) and is imported, never duplicated. ponytail, software-practices and tiger-style still apply. |
| Q43 | Mockup review | Mockups are shown to the owner as an artifact; the owner approves before implementation. |
| Q44 | Live architecture | `docs/architecture.html`, published at https://claude.ai/artifact/2GvdWKx4cyoC3WmqY8emDU, updated as pods land (build status table and change log). |
| Q46 | Framework name | Troupekit: folder `troupekit/`, crates `troupekit-<layer>`. Free on crates.io, PyPI and npm, no GitHub repo of that name (checked 2026-11-21). Chosen over Dramaturg (hard to say), Rolecast, Castwork, Cuework. |
| Q45 | Android and web | Deferred, timing decided at implementation. |

### Rejected alternatives

- Device-only or server-only engine (Q1).
- Claude-only provider, OpenRouter-only, wrapping rig as a dependency (Q2, Q5).
- Adopting rig, AutoAgents or anchor-chain as the framework (Q4).
- Actor pub/sub or supervisor-loop orchestration (Q11).
- Real files or git repo per goal (Q10).
- Rune as UI base (Q18).
- Code plugins in WASM or JS (Q16).

### Risks

- Vercel Hobby is non-commercial and caps a request at 300 s; opening sign-ups needs Pro or another host.
- Local embedding model cold starts add seconds per cold request in step mode.
- Provider API keys and calendar titles on the server raise breach impact; keys encrypted at rest, decrypted per call.
- Building a framework from scratch is the largest scope item.
- Agent-generated personas and plugins need strict schema validation.
- Health and finance advice needs safety guardrails.

### Acceptance criteria (first slice)

- The owner picks a provider and model in Settings, enters a key, and the server engine uses it; the app stores no key.
- A "six pack by Feb 1" goal goes from one input to an approved plan with trackers and a dashboard view built from library components.
- Check-ins collect weight and meals; a missed action or a tracker rule triggers an adaptation proposal in Inbox, applied per the goal's autonomy level.
- A graph run survives a server restart mid-run and resumes from its last checkpoint.
- `troupekit/` builds and tests with no Aroh imports.

### Unresolved

- `docs/decisions.md` still holds the MVP contract and must be rewritten to match this record.
