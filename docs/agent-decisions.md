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
| Q26-27 | Hosting | Deployment layer with host adapters, switchable between providers. One engine core, two run modes: worker (always-on loop: Mac, Fly, VPS) and step (each checkpointed graph step is one request, triggered by Supabase `pg_cron` plus `pg_net` and enqueue webhooks: Vercel, Cloud Run, Lambda). First host: Vercel Hobby step mode on the Rust runtime (Beta, Fluid compute, 300 s): `crates/aroh-vercel` wraps the engine router in `vercel_runtime`'s `VercelLayer`; pg_cron ticks `/v1/tick` and `/v1/push/tick` every minute, and a `jobs` trigger calls `/v1/hooks/enqueue` when a job is queued. Runbook `docs/deploy.md` (pod 21, 2026-11-22). |
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
| Q39 | Project workflow | Owner rule for this project: every ticket that needs a new component or a new visual pattern starts with a mockup written in real Svelte code. Screens composed only from approved components and the approved visual rule skip the mockup (owner, 2026-11-21, after slice 1). Owner reviews and iterates, then approves. Approved mockup code is ported into the app, not rewritten. Tests run in containers (Kiri). One pod per feature: PM plans and asks the owner, one engineer sub-agent writes tests, code and does the testing, pushes to its own branch, reports done. The leader takes a quick look and merges. No CodeRabbit, security or code reviews on this project; this overrides global gate 6 here. |
| Q40 | First slice | Framework spine (4 provider protocols plus catalog, fetch and search trait with 2 adapters, durable graph with checkpoints, persona TOML loader, memory and RAG on pgvector, step mode on Vercel) plus app (tokens and gallery for the four component families, Settings for provider, model and key, New Goal chat, Goals, Today, Calendar, Inbox), proved end to end on one fitness goal ("six pack by Feb 1") with check-ins and adaptation. Health, calendar triage, push and other categories follow. |

| Q41 | Visual direction | Neutral enterprise chrome (greys, near-black primary, small radius, hairline borders, no shadows, tables over card grids, at most 5 type sizes, one font, no gradients or decorative color). Color only where it carries meaning: status (green done, blue planned, amber due, red missed or error, grey skipped), chart series (4 to 6 distinct hues, Apple Fitness style rings), and 6 category hues as dot and left edge. Reason: owner rejected round 1 as too colourful and round 2 as having no functional color (2026-11-21). |
| Q42 | Code shape | Simple modules. One copy of every function: shared logic lives in one common module (`troupekit-core`, `aroh-domain`, `apps/aroh/src/lib/ui`) and is imported, never duplicated. ponytail, software-practices and tiger-style still apply. |
| Q43 | Mockup review | Mockups are shown to the owner as an artifact; the owner approves before implementation. |
| Q44 | Live architecture | `docs/architecture.html`, published at https://claude.ai/artifact/2GvdWKx4cyoC3WmqY8emDU, updated as pods land (build status table and change log). |
| Q46 | Framework name | Troupekit: folder `troupekit/`, crates `troupekit-<layer>`. Free on crates.io, PyPI and npm, no GitHub repo of that name (checked 2026-11-21). Chosen over Dramaturg (hard to say), Rolecast, Castwork, Cuework. |
| Q45 | Android and web | Deferred, timing decided at implementation. |
| Q47 | All-day events | Must have (owner 2026-11-22). Device all-day events show as a chip row at the top of each day on Today and Calendar. What happens to that day's actions depends on the event's kind, which the triage persona proposes and the owner confirms once: Reminder (birthday, anniversary, holiday note): actions stay as planned. Busy day (an all-day event the owner attends): the day is blocked and its actions move to other days. Away (vacation, travel): the replanner adapts each goal by its category, for example fitness turns into a no-equipment workout, piano practice pauses and moves, habits continue, and sends the change to the Inbox as a proposal. Tapping a chip reopens the choice. Mockup approved 2026-11-22 (artifact v5). |
| Q48 | Calendar edit sheet | Must have, a primary flow (owner 2026-11-22). Tap an action on Calendar or Today to move it, resize it or delete it; the change syncs and feeds learning from repeated edits. New visual pattern, so mockup first. |
| Q49 | Persona safety | An edited copy of a built-in persona must keep every `Safety:` guardrail; the server rejects the edit otherwise (owner 2026-11-22). Reason: health and finance coaches must never give diagnoses or individual investment, tax or legal advice. |
| Q50 | Persona library screen | Must have (owner 2026-11-22). An app screen to list the coaches per team, see which coaches work on a goal, copy one, edit its name, style, instructions and rules, and save. Built from approved components, no mockup (Q39). Safety lines show as locked. |
| Q51 | Device holidays | Events from the phone's built-in holiday calendars default to Reminder: chip shown, day kept as planned, no question asked; tap the chip to change it (owner 2026-11-22). |
| Q52 | After slice 2 | Fix the five slice 2 follow-ups in one pod, set up production with the owner step by step now, then run Aroh on the owner's own iPhone for daily use (owner 2026-11-22). |
| Q53 | Fixed items | Device calendar events are read-only and planned around. A locked (fixed-time) Aroh action is never moved by any agent or mover, approval or not: agents and the Busy-day mover adjust only flexible actions around it, and a true clash becomes an Inbox note, not a move. A 'Fixed time' switch exists on the edit sheet and on each action while approving a plan; a hand edit still locks the action (owner 2026-11-22). Replaces the earlier rule that a proposal touching a locked action only forced approval. |
| Q54 | Travel and time zones | Daylight saving: actions keep their local wall time (already true). Travel: when the device time zone changes, flexible actions follow local time (a 07:00 workout stays 07:00 where the owner is), fixed actions keep their real instant (a 10:00 New York call shows at 16:00 in Paris), reminders are rescheduled, and the owner gets one Inbox note listing what shifted (owner 2026-11-22). |
| Q55 | Prod branch | GitHub branch `prod` is the production pipeline branch Vercel deploys from; the leader pushes local master to it (owner 2026-11-22). GitHub master keeps its rules (pull request, signed commits, linear history) and must later hold everything too; that needs signed commits or a signed squash PR, decided when it is done. |
| Q56 | Location awareness | Owner 2026-11-22. Aroh learns places and routines from location and nudges near them. Privacy: raw visits (iOS CLVisit and CLMonitor, Always permission) stay in the phone's local SQLite only and never sync; only named places and learned patterns sync so agents can plan with them; no GPS trail. Places: after repeated visits Aroh asks "Is this your gym?" and links the place to a goal or action; manual add by map search; unnamed places are never shown or used; at most 20 watched places (iOS limit), chosen by relevance. Near = within about 150 m and still there after 2 minutes; passing by is ignored. Nudge only when a linked action is due today or was missed and not done, at most one per place per day, never in quiet hours (22:00 to 06:00), fixed items untouched. Patterns: repeated visits plus repeated actions (same title, same place, regular gap) become an Inbox proposal for a repeating action linked to the place; nothing is created without approval; a declined pattern is not proposed again for a while. Generalizes to any place and routine. Routines without a goal live in one built-in "Routines" goal (Habits category) with its own dashboard; routines that belong to a goal attach to it. Controls: pause tracking, clear raw history, ignore a place, and a location log screen of recent visits. New visual patterns (place prompt, places screen with map search, location log, location settings), so mockup first. Mockup approved 2026-11-22 (artifact v6) with these defaults: place prompt in Inbox only; raw visits kept 90 days on the phone, then deleted; learned patterns stay on the phone until the owner approves them, then sync; map is native MapKit (no third party sees location, no key); routine streaks show a unit ("4 weeks"); category name is the app's existing "Habit". |

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
