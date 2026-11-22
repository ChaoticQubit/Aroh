# Pod 19: Persona Library Screen

**Goal.** Q50: an app screen that lists the coaches per team, shows which coaches work on a goal, copies a built-in, edits the copy's name, style, instructions and rules, and saves it. Built-ins are read-only. `Safety:` guardrails show as locked (Q49).

**Branch.** `feat/persona-library-screen` off master `829e9b2`.

## Global Constraints

- Settled decisions: `docs/agent-decisions.md` Q3, Q9, Q33, Q39, Q41, Q49, Q50. Server side from pod 17 (`docs/superpowers/plans/2026-11-22-pod17-categories-personas.md`).
- No mockup (Q39): compose only from `apps/aroh/src/lib/ui` components and primitives (Card, Panel, Field, Button, Input, Textarea, Badge, Separator, Persona). Q41: neutral chrome, hairline borders, tables over card grids; category hue only as a dot or left edge (`var(--cat-{slot})`).
- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. TOML, YAML, shell carry no comments. No em dashes.
- `assert!` for internal invariants in Rust; input validation returns errors.
- Shared files take additive edits only: `docs/engine-api.yaml` (regenerated from the binary, never by hand), server router and `ApiDoc`, `lib.rs` handler list, `commands/mod.rs`, `GoalScreen.svelte`, `SettingsScreen.svelte`, their routes.
- `troupekit/`, `aroh-domain`, `aroh-store`, `supabase/` are not edited. `troupekit/` stays free of "aroh".
- All engine calls go through `Engine` in `apps/aroh/src/lib/app/engine.ts` (`app_engine` adds the user JWT in Rust, no allow-list). No provider keys in the app.
- Tests in Kiri with `export HOME=/Users/chaoticqubit`: server `sh crates/aroh-server/tests/run.sh`; frontend `pnpm check` and `pnpm vitest run` in `node:24-slim`. Own image and volume names with `p19`. Remove own images and volumes after each run. Never touch `pod-a-*` or other pods' resources.
- Simulator device `aroh-p19` (iPhone 15 Pro), dev port 1427 (never committed), own e2e compose project and ports, fake model transports. `git checkout -- apps/aroh/src-tauri/gen/apple` before every commit.
- Conventional commits, one per task, commit and never push, no `Co-Authored-By` trailer. Stage by path; `graphify-out` is an untracked symlink and never staged. Bring in master with `git merge master` before DONE, never rebase.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Field mapping | Name = `display_name`. Style = `role`. Instructions = `instructions`. Rules = `guardrails`. The slug `name`, `goal`, tools, evals and `[output]` show read-only or not at all. | `check` rejects a changed `name`. No persona has a style key; `role` follows "You are {display_name}." in `system_prompt`, so it sets how the coach speaks. Rejected: `context`, empty in every shipped persona and named for the owner's situation, not tone. |
| Edit transport | Server additive: `PersonaView` gains `instructions`, `guardrails` and `locked` (the built-in's `Safety:` guardrails). `EditPersona` gains optional `display_name`, `role`, `instructions`, `guardrails`; `toml` becomes optional. Exactly one of `toml` or the field set; the server applies the fields to the stored copy through `toml::Table`, then runs `check`. | The app has no TOML library and must not add one without asking. A TS rewriter of selected TOML keys is fragile (multi-line strings and arrays). Rust already parses TOML; `check` stays the one validator. Existing `{toml}` callers keep working. |
| Locked rules | `locked` lists the built-in's guardrails that start with `Safety:`. The editor shows them with a lock and no remove control and always sends them byte-identical. The server still rejects a dropped one (`personas.rs` `check`). | Q49; exact-string match on the server. |
| Error text | A 422 `invalid_persona: <file>: guardrails: keep `<rule>`` shows as "Safety rules stay on every coach: <rule>". Other 422s show the server message without the `invalid_persona: ` prefix. 409 on copy reloads the list and opens the existing copy. Offline shows `NEEDS_ENGINE`. | Owner-facing sentence; the raw code is for clients. |
| Grouping | Groups in order fitness, music, learning, finance, habits, then "Shared" for personas whose `teams` hold none of the five (general-only and no-team personas). A persona in several teams shows in each. Group header carries the dot of the seeded category of that team (`health-fitness`, `hobby-skill`, `learning`, `financial`, `habit`) through `categoryList` slots; Shared has none. | Owner brief lists five teams; `goal-researcher` and `goal-planner` also serve general, which needs no group of its own. |
| Coaches on a goal | The goal's persona rows of the snapshot unioned with the persona slugs of the goal's runs (`GET /v1/runs?goal_id=` gains `personas`, the sorted keys of the run's snapshot), once each, in library order, display names from the engine list; a slug the list lacks shows as the slug. Shown at `/personas?goal=<id>`, linked from the goal screen; no runs and no rows shows the empty state. | Changed 2026-11-22 (leader): snapshot rows with `goal_id` are written only by plugin bundles, so `runs.personas` is the real record. Rejected: category to team, since the team is chosen by title keywords. |
| Routes | `/personas` (library, optional `?goal=`), `/personas/[name]` (detail and editor). Reached from Settings (a "Coaches" section with a link) and from the goal screen ("Coaches on this goal"). No new tab. | Settings is reached from the avatar; tabs stay five. |
| Built-in vs copy | `source = builtin`: fields read-only, one "Copy to edit" button (POST copy, 201). `source = library`: editable fields, Save (PUT fields). | Server semantics. |

## File Structure

| Path | Responsibility |
|---|---|
| `crates/aroh-server/src/personas.rs` | view fields, `apply` of an edit to TOML, edit body |
| `crates/aroh-server/src/api.rs` | handler passes the body (additive) |
| `crates/aroh-server/Cargo.toml` | `toml` (workspace version as troupekit, already in Cargo.lock) |
| `crates/aroh-server/tests/api.rs` | PUT with fields, safety rejection, view fields |
| `docs/engine-api.yaml` | regenerated |
| `apps/aroh/src/lib/app/personas.ts`, `personas.test.ts` | types, grouping, coaches of a goal, error text |
| `apps/aroh/src/lib/app/engine.ts`, `engine.test.ts` | `personas()`, `copyPersona()`, `editPersona()` |
| `apps/aroh/src/lib/ui/screens/PersonasScreen.svelte`, `PersonaScreen.svelte` | list and detail |
| `apps/aroh/src/routes/personas/+page.svelte`, `apps/aroh/src/routes/personas/[name]/+page.svelte` | routes |
| `apps/aroh/src/lib/ui/screens/SettingsScreen.svelte`, `GoalScreen.svelte` | entry links (additive) |
| `e2e/pod19.js` (and a `pod19.sh` if the harness needs one) | simulator flow |
| `docs/screenshots/pod19/*.png` | screenshots |

## Tasks

### Task 1: Server edit fields

- [ ] Tests first. Unit: `apply` over every shipped persona with no field set returns TOML that parses to the same `Persona`; setting each field changes only that field; instructions with a `"`, a backslash and a newline round-trip; `toml` plus a field is an error; no field and no toml is an error. View: `locked` of `budget-coach` is its one `Safety:` line, of `tutor` empty. api.rs: copy then PUT fields answers 200 and the list shows the new display name and instructions; PUT fields without a safety line answers 422 naming `guardrails`; PUT `{toml}` still works.
- [ ] Implement; regenerate `docs/engine-api.yaml` with the binary.
- [ ] `sh crates/aroh-server/tests/run.sh` green. Commit `feat(server): structured persona edits`.

### Task 2: App client and logic

- [ ] vitest first: `Engine.personas`, `copyPersona` (201 ok, 409 maps to `exists`), `editPersona` (200, 422 safety sentence, other 422 stripped prefix, `needs_engine`) with a fake `Call`; `groupByTeam` order, multi-team, Shared; `coachesOn(views, rows, goalId)`; `isLocked`.
- [ ] Implement `personas.ts` and the engine methods. Commit `feat(app): persona library client`.

### Task 3: Screens and entry points

- [ ] `PersonasScreen` (table-style list per group, dot on header, row shows display name, role line, "Your copy" badge for library), `PersonaScreen` (read-only fields for built-in with Copy to edit; Input for name, Textarea for style and instructions, rules as a list with add and remove, locked rows with a lock icon and no remove; Save; status line for errors).
- [ ] Routes, Settings "Coaches" link, GoalScreen "Coaches on this goal" link. `pnpm check`, vitest, `pnpm build` in `node:24-slim`. Commit `feat(app): persona library screen`.

### Task 4: Simulator acceptance

- [ ] `aroh-p19`, port 1427, own compose project. Flow: Settings, Coaches list, open Budget coach (built-in, read-only, locked safety line), Copy to edit, change name and instructions (with a quote, a backslash and a newline), add a rule, Save, list shows the copy; try to remove the safety line (no control) and show the server refusal path if reachable; goal screen "Coaches on this goal" lists the goal's coaches.
- [ ] Screenshots to `docs/screenshots/pod19/`. Restore `gen/apple`. Commit `test(e2e): persona library flow`.

## Open Items

- `graphify update` after merge (the graph lives in the main checkout).
