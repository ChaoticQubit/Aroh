# Slice 1 Roadmap

> **For agentic workers:** this file orders the pods of slice 1. Each pod has its own plan file. Use superpowers:subagent-driven-development to execute a pod plan task by task.

**Goal:** Troupekit spine plus the Aroh app, proved end to end on one fitness goal ("six pack by Feb 1") with check-ins and adaptation.

**Architecture:** see `docs/architecture.html` (live at https://claude.ai/artifact/2GvdWKx4cyoC3WmqY8emDU) and `docs/agent-decisions.md` (Q1 to Q46).

## Global Constraints

- Framework name Troupekit, folder `troupekit/`, crates `troupekit-<layer>`, license `MIT OR Apache-2.0`, zero imports of any `aroh-*` crate.
- One copy of every function: shared code in `troupekit-core`, `crates/aroh-domain`, `apps/aroh/src/lib/ui`, imported everywhere else.
- No code comments, only docstrings directly above a declaration, 3 to 5 lines. No em dashes anywhere.
- Rust: `assert!` for internal invariants. Input validation returns errors, never asserts.
- Tests run in Kiri containers: `HOME=/Users/chaoticqubit kiri build -f <Dockerfile> -t <tag> .` then `HOME=/Users/chaoticqubit kiri run --rm <tag>`.
- Every ticket with UI starts with a Svelte mockup published as an artifact; the owner approves before it is ported. Backend-only tickets have no mockup.
- Pod process: PM plans, one engineer sub-agent writes tests first then code, commits on its own branch `<type>/<slug>`, reports DONE with branch, base..tip and VERIFIED or NOT VERIFIED. The leader takes a quick look and merges. No review tracks.
- Visual direction: simple, professional, calendar-first, neutral palette with one accent, no flair.
- Conventional commit messages.

## Pods and order

| # | Pod | Plan | Depends on | UI mockup |
|---|---|---|---|---|
| 1 | design: tokens, components, screen mockups | `2026-11-21-pod1-design-mockups.md` | none | yes, this is the mockup |
| 2 | troupekit-core and workspace | `2026-11-21-pod2-troupekit-core.md` | none | no |
| 3 | troupekit-providers: wire, decoder, transport, catalog, middleware | written after pod 2 merges | 2 | no |
| 4 | troupekit-tools: Tool trait, fetch (salvaged SSRF guard), search trait with Brave and SearXNG | written after pod 2 | 2 | no |
| 5 | troupekit-agent: persona TOML, run state machine, typed output, evals, budgets | written after pod 3 | 2, 3 | no |
| 6 | troupekit-graph and troupekit-host: workflows, checkpointer, human nodes, step mode | written after pod 5 | 5 | no |
| 7 | troupekit-context: memory scopes, ONNX embeddings, pgvector retrieval | written after pod 2 | 2 | no |
| 8 | aroh-domain, aroh-store, Supabase schema and sync (salvage HLC and LWW) | written after pod 2 | none | no |
| 9 | aroh-server: HTTPS API, jobs, Aroh toolkits, personas and workflows for the fitness goal | written after pods 6, 7, 8 | 6, 7, 8 | no |
| 10 | app screens ported from approved mockups: Settings, New Goal, Goals, Today, Calendar, Inbox | written after pod 1 approval and pod 8 | 1, 8 | ported |
| 11 | end to end: six-pack goal, check-ins, adaptation | written after pods 9, 10 | 9, 10 | no |

Pods 1, 2 and 8 start in parallel, each in its own worktree. At most 10 agents alive at once.

## Slice 1 acceptance

- Owner picks provider and model in Settings, enters a key; the server engine uses it; the app stores no key.
- "six pack by Feb 1" goes from one input to an approved plan with trackers and a dashboard built from library components.
- Check-ins collect weight and meals; a missed action or tracker rule produces an adaptation proposal handled per the goal's autonomy level.
- A graph run survives a server restart mid-run and resumes from its last checkpoint.
- `troupekit/` builds and tests with no Aroh imports.

## Live page upkeep

When a pod merges, the leader updates the build status table and change log in `docs/architecture.html` and republishes the artifact.
